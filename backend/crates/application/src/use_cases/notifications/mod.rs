//! Notifications-related use cases.
//!
//! For now the project only needs a bridge between domain events (for example,
//! “new homework was created/updated”) and the push-delivery port.

pub mod notify_new_homework;
pub mod resolve_homework_recipients;
