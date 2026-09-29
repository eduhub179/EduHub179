//! Notification adapters.
//!
//! The real provider implementation lives here (FCM / APNs / OneSignal), while
//! unit tests use a mock implementation of the same interface.

pub mod push_sender;
