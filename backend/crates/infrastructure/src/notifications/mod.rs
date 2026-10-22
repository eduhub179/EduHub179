//! Notification adapters.
//!
//! The real provider implementation lives here (FCM / APNs / OneSignal), while
//! unit tests use a mock implementation of the same interface.

pub mod push_sender;
pub mod web_push_sender;

pub use web_push_sender::{WebPushSender, WebPushSettings};
