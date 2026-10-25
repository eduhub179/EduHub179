//! Web Push sender: browser notifications (Chrome, Firefox, Edge, Safari / iOS PWA).
//!
//! Each `device_tokens` row with platform `web` holds the browser's subscription JSON
//! (`WebPushSubscription::to_token`). A message is encrypted for that browser (aes128gcm,
//! RFC 8291), signed with our VAPID key (RFC 8292) and POSTed to the push service URL.
//!
//! Dependencies: `web-push` (hyper client), `serde_json`, `DeviceTokenRepository`.
//! Guarantees:
//! - no panics; provider errors are logged via `tracing` (never the subscription itself);
//! - a subscription the push service reports as gone (404/410) is deactivated, so the
//!   next notification does not retry it;
//! - one failing device does not stop delivery to the user's other devices;
//! - non-web platforms are skipped (mobile push is not implemented yet).

use std::sync::Arc;

use domain::errors::DomainError;
use domain::ports::notification_sender::{NotificationPayload, NotificationSender};
use domain::repositories::device_token_repository::DeviceTokenRepository;
use domain::value_objects::notification_platform::NotificationPlatform;
use serde::Deserialize;
use uuid::Uuid;
use web_push::{
    ContentEncoding, HyperWebPushClient, PartialVapidSignatureBuilder, SubscriptionInfo,
    Urgency, VapidSignatureBuilder, WebPushClient, WebPushError, WebPushMessageBuilder,
};

/// A notification older than a day is no longer useful: the push service may drop it.
const TTL_SECONDS: u32 = 24 * 60 * 60;
/// Push services accept ~3 KB of payload; keep the body well below that.
const MAX_BODY_CHARS: usize = 300;

/// VAPID settings (`VAPID_*` in backend/.env).
#[derive(Debug, Clone)]
pub struct WebPushSettings {
    /// Raw 32-byte P-256 private key, base64url without padding.
    /// Generate once: `openssl rand 32 | base64 | tr '+/' '-_' | tr -d '='`.
    pub private_key: String,
    /// Contact for push services, e.g. `mailto:admin@179.ru`.
    pub subject: String,
}

/// Browser subscription JSON as stored in `device_tokens.token`.
#[derive(Deserialize)]
struct StoredSubscription {
    endpoint: String,
    keys: StoredKeys,
}

#[derive(Deserialize)]
struct StoredKeys {
    p256dh: String,
    auth: String,
}

pub struct WebPushSender {
    device_tokens: Arc<dyn DeviceTokenRepository>,
    vapid: PartialVapidSignatureBuilder,
    subject: String,
    public_key: String,
    client: HyperWebPushClient,
}

impl WebPushSender {
    /// # Fail-safe поведение
    /// `InternalError` if the private key is not a valid base64url P-256 key
    /// (fail fast at startup, not on the first notification).
    pub fn new(
        device_tokens: Arc<dyn DeviceTokenRepository>,
        settings: &WebPushSettings,
    ) -> Result<Self, DomainError> {
        let vapid = VapidSignatureBuilder::from_base64_no_sub(settings.private_key.trim())
            .map_err(|error| {
                tracing::error!("web push: invalid VAPID_PRIVATE_KEY: {error}");
                DomainError::InternalError
            })?;
        let public_key = base64_url(&vapid.get_public_key());
        Ok(Self {
            device_tokens,
            vapid,
            subject: settings.subject.trim().to_string(),
            public_key,
            client: HyperWebPushClient::new(),
        })
    }

    /// Public VAPID key for `pushManager.subscribe({applicationServerKey})` in the browser.
    pub fn public_key(&self) -> &str {
        &self.public_key
    }

    fn message_json(payload: &NotificationPayload) -> String {
        let body: String = payload.body.chars().take(MAX_BODY_CHARS).collect();
        serde_json::json!({
            "title": payload.title,
            "body": body,
            "url": payload.data.get("url"),
            // Same tag replaces the previous notification instead of stacking a new one.
            "tag": payload.data.get("tag"),
        })
        .to_string()
    }
}

#[async_trait::async_trait]
impl NotificationSender for WebPushSender {
    /// # Fail-safe поведение
    /// `PushSubscriptionExpired` for a subscription the push service no longer knows,
    /// `InvalidDeviceTokenFormat` for a corrupted stored token, `InternalError` otherwise.
    async fn send_to_device(
        &self,
        token: &str,
        platform: NotificationPlatform,
        payload: &NotificationPayload,
    ) -> Result<(), DomainError> {
        if platform != NotificationPlatform::Web {
            return Ok(());
        }
        let stored: StoredSubscription =
            serde_json::from_str(token).map_err(|_| DomainError::InvalidDeviceTokenFormat)?;
        let subscription = SubscriptionInfo::new(stored.endpoint, stored.keys.p256dh, stored.keys.auth);

        let mut signature = self.vapid.clone().add_sub_info(&subscription);
        signature.add_claim("sub", self.subject.as_str());
        let signature = signature.build().map_err(|error| {
            tracing::error!("web push: cannot sign VAPID claims: {error}");
            DomainError::InternalError
        })?;

        let content = Self::message_json(payload);
        let mut message = WebPushMessageBuilder::new(&subscription);
        message.set_payload(ContentEncoding::Aes128Gcm, content.as_bytes());
        message.set_vapid_signature(signature);
        message.set_ttl(TTL_SECONDS);
        // High: the push service delivers at once instead of batching it for an idle
        // device (with Normal, FCM may hold a message for minutes). Homework
        // notifications are user-visible, which is what High is meant for.
        message.set_urgency(Urgency::High);
        let message = message.build().map_err(|error| {
            tracing::error!("web push: cannot build message: {error}");
            DomainError::InternalError
        })?;

        match self.client.send(message).await {
            Ok(()) => Ok(()),
            Err(WebPushError::EndpointNotValid(_) | WebPushError::EndpointNotFound(_)) => {
                Err(DomainError::PushSubscriptionExpired)
            }
            Err(error) => {
                tracing::warn!("web push: delivery failed: {error}");
                Err(DomainError::InternalError)
            }
        }
    }

    /// Sends to every active device of the user; a gone subscription is deactivated,
    /// other per-device failures are logged and skipped.
    ///
    /// # Fail-safe поведение
    /// `Err` only if the user's devices cannot be loaded from the DB.
    async fn send_to_user(&self, user_id: Uuid, payload: &NotificationPayload) -> Result<(), DomainError> {
        for device in self.device_tokens.get_active_by_user(user_id).await? {
            match self.send_to_device(&device.token, device.platform, payload).await {
                Ok(()) => {}
                Err(DomainError::PushSubscriptionExpired | DomainError::InvalidDeviceTokenFormat) => {
                    if let Err(error) = self.device_tokens.deactivate(device.id).await {
                        tracing::warn!("web push: cannot deactivate device {}: {error}", device.id);
                    }
                }
                // Already logged in send_to_device; keep going with other devices.
                Err(_) => {}
            }
        }
        Ok(())
    }
}

/// base64url without padding (what browsers expect for `applicationServerKey`).
fn base64_url(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use domain::entities::device_token::DeviceToken;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Test key from the web-push crate docs (never used outside tests).
    const TEST_PRIVATE_KEY: &str = "IQ9Ur0ykXoHS9gzfYX0aBjy9lvdrjx_PFUXmie9YRcY";
    /// Real browser keys (from the Web Push spec examples) so encryption succeeds.
    const P256DH: &str = "BNcRdreALRFXTkOOUHK1EtK2wtaz5Ry4YfYCA_0QTpQtUbVlUls0VJXg7A8u-Ts1XbjhazAkj7I99e8QcYP7DkM";
    const AUTH: &str = "tBHItJI5svbpez7KI4CCXg";

    struct Devices {
        devices: Vec<DeviceToken>,
        deactivated: Mutex<Vec<Uuid>>,
    }

    #[async_trait::async_trait]
    impl DeviceTokenRepository for Devices {
        async fn get_by_id(&self, _: Uuid) -> Result<DeviceToken, DomainError> {
            unimplemented!()
        }
        async fn get_by_user(&self, _: Uuid) -> Result<Vec<DeviceToken>, DomainError> {
            unimplemented!()
        }
        async fn get_active_by_user(&self, _: Uuid) -> Result<Vec<DeviceToken>, DomainError> {
            Ok(self.devices.clone())
        }
        async fn get_active_by_user_and_platform(
            &self,
            _: Uuid,
            _: NotificationPlatform,
        ) -> Result<Vec<DeviceToken>, DomainError> {
            unimplemented!()
        }
        async fn save(&self, token: DeviceToken) -> Result<DeviceToken, DomainError> {
            Ok(token)
        }
        async fn deactivate(&self, id: Uuid) -> Result<(), DomainError> {
            self.deactivated.lock().unwrap().push(id);
            Ok(())
        }
        async fn deactivate_token_for_other_users(
            &self,
            _: NotificationPlatform,
            _: &str,
            _: Uuid,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn deactivate_all_for_user(&self, _: Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }
    }

    fn device(platform: NotificationPlatform, token: String) -> DeviceToken {
        let now = Utc::now().naive_utc();
        DeviceToken::try_new(Uuid::new_v4(), Uuid::new_v4(), platform, token, now, now).unwrap()
    }

    fn subscription_token(endpoint: &str) -> String {
        format!(r#"{{"endpoint":"{endpoint}","keys":{{"p256dh":"{P256DH}","auth":"{AUTH}"}}}}"#)
    }

    fn sender(devices: Arc<Devices>) -> WebPushSender {
        WebPushSender::new(
            devices,
            &WebPushSettings {
                private_key: TEST_PRIVATE_KEY.to_string(),
                subject: "mailto:test@179.ru".to_string(),
            },
        )
        .unwrap()
    }

    fn payload() -> NotificationPayload {
        NotificationPayload {
            user_id: Uuid::new_v4(),
            title: "Новое ДЗ · Алгебра".to_string(),
            body: "№ 12–15".to_string(),
            data: HashMap::from([("url".to_string(), "/main".to_string())]),
        }
    }

    /// A fake push service: answers every request with `status`, counts requests.
    async fn push_service(status: &'static str) -> (String, Arc<Mutex<usize>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let hits = Arc::new(Mutex::new(0));
        let counter = hits.clone();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                *counter.lock().unwrap() += 1;
                let mut buffer = vec![0u8; 16 * 1024];
                let _ = socket.read(&mut buffer).await;
                let response = format!("HTTP/1.1 {status}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });
        (format!("http://{address}/push/abc"), hits)
    }

    #[test]
    fn public_key_is_derived_from_private_key() {
        let sender = sender(Arc::new(Devices {
            devices: vec![],
            deactivated: Mutex::new(vec![]),
        }));
        // Uncompressed P-256 point: 65 bytes -> 87 base64url chars, starts with 0x04 ("B").
        assert_eq!(sender.public_key().len(), 87);
        assert!(sender.public_key().starts_with('B'));
    }

    #[test]
    fn invalid_private_key_fails_at_startup() {
        let result = WebPushSender::new(
            Arc::new(Devices {
                devices: vec![],
                deactivated: Mutex::new(vec![]),
            }),
            &WebPushSettings {
                private_key: "not-a-key".to_string(),
                subject: "mailto:test@179.ru".to_string(),
            },
        );
        assert!(matches!(result, Err(DomainError::InternalError)));
    }

    #[test]
    fn message_json_is_short_and_carries_url() {
        let mut long = payload();
        long.body = "я".repeat(1000);
        let json: serde_json::Value = serde_json::from_str(&WebPushSender::message_json(&long)).unwrap();
        assert_eq!(json["title"], "Новое ДЗ · Алгебра");
        assert_eq!(json["url"], "/main");
        assert_eq!(json["body"].as_str().unwrap().chars().count(), MAX_BODY_CHARS);
    }

    #[tokio::test]
    async fn delivered_message_reaches_push_service() {
        let (endpoint, hits) = push_service("201 Created").await;
        let devices = Arc::new(Devices {
            devices: vec![device(NotificationPlatform::Web, subscription_token(&endpoint))],
            deactivated: Mutex::new(vec![]),
        });

        sender(devices.clone()).send_to_user(Uuid::new_v4(), &payload()).await.unwrap();

        assert_eq!(*hits.lock().unwrap(), 1);
        assert!(devices.deactivated.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn gone_subscription_is_deactivated_and_others_still_get_it() {
        let (gone_endpoint, _) = push_service("410 Gone").await;
        let (ok_endpoint, ok_hits) = push_service("201 Created").await;
        let gone = device(NotificationPlatform::Web, subscription_token(&gone_endpoint));
        let broken = device(NotificationPlatform::Web, "not json".to_string());
        let mobile = device(NotificationPlatform::Android, "fcm-token".to_string());
        let ok = device(NotificationPlatform::Web, subscription_token(&ok_endpoint));
        let devices = Arc::new(Devices {
            devices: vec![gone.clone(), broken.clone(), mobile, ok],
            deactivated: Mutex::new(vec![]),
        });

        sender(devices.clone()).send_to_user(Uuid::new_v4(), &payload()).await.unwrap();

        assert_eq!(*devices.deactivated.lock().unwrap(), vec![gone.id, broken.id]);
        assert_eq!(*ok_hits.lock().unwrap(), 1, "a gone device must not stop the others");
    }
}
