//! Browser push subscription (Web Push, RFC 8030 / RFC 8291).
//!
//! The browser gives `{endpoint, keys: {p256dh, auth}}` from `pushManager.subscribe()`:
//! the push-service URL (Google / Mozilla / Apple) and the keys to encrypt messages for
//! this browser. It is stored as a `device_tokens` row with platform `web`; `to_token`
//! is that stored form.
//!
//! Guarantees: only `https://` endpoints and base64url keys pass `try_new`, so the token
//! JSON is built without escaping and can never smuggle quotes or another URL scheme.

use crate::errors::DomainError;

/// `device_tokens.token` is VARCHAR(1024); the JSON wrapper and keys take ~150 chars.
const MAX_ENDPOINT_CHARS: usize = 800;
const MAX_KEY_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebPushSubscription {
    endpoint: String,
    p256dh: String,
    auth: String,
}

fn is_base64url(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_KEY_CHARS
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '='))
}

impl WebPushSubscription {
    /// # Fail-safe поведение
    /// `InvalidDeviceTokenFormat` unless the endpoint is an `https://` URL without
    /// whitespace, quotes or backslashes (≤ 800 chars) and both keys are base64url.
    pub fn try_new(endpoint: &str, p256dh: &str, auth: &str) -> Result<Self, DomainError> {
        let endpoint = endpoint.trim();
        let endpoint_ok = endpoint.starts_with("https://")
            && endpoint.len() > "https://".len()
            && endpoint.len() <= MAX_ENDPOINT_CHARS
            && !endpoint
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '"' | '\\'));
        let (p256dh, auth) = (p256dh.trim(), auth.trim());
        if !endpoint_ok || !is_base64url(p256dh) || !is_base64url(auth) {
            return Err(DomainError::InvalidDeviceTokenFormat);
        }
        Ok(Self {
            endpoint: endpoint.to_string(),
            p256dh: p256dh.to_string(),
            auth: auth.to_string(),
        })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// The form stored in `device_tokens.token`: the browser's own JSON shape.
    pub fn to_token(&self) -> String {
        format!(
            r#"{{"endpoint":"{}","keys":{{"p256dh":"{}","auth":"{}"}}}}"#,
            self.endpoint, self.p256dh, self.auth
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_subscription_becomes_browser_json() {
        let sub = WebPushSubscription::try_new(
            " https://fcm.googleapis.com/fcm/send/abc:DEF-1 ",
            "BNcRdreALRFXTkOOUHK1EtK2wtaz5Ry4YfYCA_0QTpQtUbVlUls0VJXg7A8u-Ts1XbjhazAkj7I99e8QcYP7DkM",
            "tBHItJI5svbpez7KI4CCXg",
        )
        .unwrap();

        assert_eq!(sub.endpoint(), "https://fcm.googleapis.com/fcm/send/abc:DEF-1");
        assert_eq!(
            sub.to_token(),
            r#"{"endpoint":"https://fcm.googleapis.com/fcm/send/abc:DEF-1","keys":{"p256dh":"BNcRdreALRFXTkOOUHK1EtK2wtaz5Ry4YfYCA_0QTpQtUbVlUls0VJXg7A8u-Ts1XbjhazAkj7I99e8QcYP7DkM","auth":"tBHItJI5svbpez7KI4CCXg"}}"#
        );
        assert!(sub.to_token().len() < 1024);
    }

    #[test]
    fn rejects_unsafe_or_malformed_input() {
        let key = "tBHItJI5svbpez7KI4CCXg";
        for endpoint in [
            "http://push.example.com/x",
            "https://",
            "https://push.example.com/a\"b",
            "https://push.example.com/a b",
            "javascript:alert(1)",
        ] {
            assert_eq!(
                WebPushSubscription::try_new(endpoint, key, key),
                Err(DomainError::InvalidDeviceTokenFormat),
                "{endpoint}"
            );
        }
        let endpoint = "https://push.example.com/x";
        assert!(WebPushSubscription::try_new(endpoint, "a\"b", key).is_err());
        assert!(WebPushSubscription::try_new(endpoint, key, "").is_err());
        assert!(WebPushSubscription::try_new(&format!("https://e.com/{}", "a".repeat(800)), key, key).is_err());
    }
}
