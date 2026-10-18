//! FILE: agent_forward.rs
//! PURPOSE: Forwards Telegram updates the bot does not handle itself to the chat agent (Python service
//!          in `agent/`): messages from class chats, homework a logged-in user sends in DM, and presses
//!          on the agent's buttons. Users talk to one bot; only this process calls getUpdates (no 409),
//!          the agent answers with the same bot token through the Bot API.
//! DEPENDENCIES: `reqwest`, `serde_json`, `teloxide`.

use std::time::Duration;
use teloxide::types::Update;

/// HTTP client for the agent endpoint (`AGENT_URL`, secret in `X-Agent-Key`).
pub struct AgentForwarder {
    client: reqwest::Client,
    url: String,
    key: String,
}

impl AgentForwarder {
    pub fn new(url: String, key: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build HTTP client");
        Self { client, url, key: key.unwrap_or_default() }
    }

    /// Sends the raw update as Bot API JSON. Fail-safe: returns false if the agent is down,
    /// the caller decides what to tell the user.
    pub async fn forward(&self, update: &Update) -> bool {
        let body = match serde_json::to_vec(update) {
            Ok(body) => body,
            Err(e) => {
                tracing::warn!("agent forward: cannot serialize update {}: {e}", update.id);
                return false;
            }
        };
        let result = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("X-Agent-Key", &self.key)
            .body(body)
            .send()
            .await;
        match result {
            Ok(resp) if resp.status().is_success() => true,
            Ok(resp) => {
                tracing::warn!("agent forward: agent answered {}", resp.status());
                false
            }
            Err(e) => {
                tracing::warn!("agent forward: agent unreachable: {e}");
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    /// The agent reads forwarded updates exactly like getUpdates results, so teloxide must write back
    /// the Bot API field names it parses (chat.type, from.id, text, callback_query.data, ...).
    fn roundtrip(raw: Value) -> Value {
        let update: Update = serde_json::from_str(&raw.to_string()).expect("valid Bot API update");
        serde_json::to_value(&update).unwrap()
    }

    #[test]
    fn group_message_keeps_bot_api_fields() {
        let out = roundtrip(json!({"update_id": 10, "message": {
            "message_id": 5, "date": 1792400000,
            "chat": {"id": -1001, "type": "supergroup", "title": "Физика 11Б"},
            "from": {"id": 42, "is_bot": false, "first_name": "Тест"},
            "text": "дз к пятнице §12"}}));
        let m = &out["message"];
        assert_eq!(out["update_id"], 10);
        assert_eq!(m["chat"]["type"], "supergroup");
        assert_eq!(m["chat"]["title"], "Физика 11Б");
        assert_eq!(m["from"]["id"], 42);
        assert_eq!(m["message_id"], 5);
        assert_eq!(m["date"], 1792400000);
        assert_eq!(m["text"], "дз к пятнице §12");
    }

    #[test]
    fn callback_keeps_data_and_sender() {
        let out = roundtrip(json!({"update_id": 11, "callback_query": {
            "id": "77", "chat_instance": "1", "data": "dm:ab12cd34:ok",
            "from": {"id": 42, "is_bot": false, "first_name": "Тест"}}}));
        assert_eq!(out["callback_query"]["data"], "dm:ab12cd34:ok");
        assert_eq!(out["callback_query"]["from"]["id"], 42);
        assert_eq!(out["callback_query"]["id"], "77");
    }

    /// Against a running agent: AGENT_TEST_URL=http://127.0.0.1:8091/telegram/update AGENT_KEY=...
    /// cargo test -p telegram-adapter -- --ignored
    #[tokio::test]
    #[ignore]
    async fn forwards_to_running_agent() {
        let url = std::env::var("AGENT_TEST_URL").expect("AGENT_TEST_URL");
        let update: Update = serde_json::from_str(&json!({"update_id": 12, "message": {
            "message_id": 6, "date": 1792400000, "chat": {"id": 42, "type": "private", "first_name": "Тест"},
            "from": {"id": 42, "is_bot": false, "first_name": "Тест"}, "text": "физика к пятнице №5"}}).to_string())
            .unwrap();
        let agent = AgentForwarder::new(url.clone(), std::env::var("AGENT_KEY").ok());
        assert!(agent.forward(&update).await);
        assert!(!AgentForwarder::new(url, Some("wrong".into())).forward(&update).await);
    }
}
