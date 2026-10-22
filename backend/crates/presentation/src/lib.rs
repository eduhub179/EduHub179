//! Presentation crate: HTTP API layer.
//!
//! This crate is responsible for HTTP transport, request/response DTOs,
//! auth middleware, and the Axum router that wires the application use cases.

pub mod router;
pub mod routes;
pub use routes::notifications::PushNotifications;
pub use router::{
    create_app, create_app_with_auth, create_app_with_auth_and_schedule, create_app_with_schedule,
    AgentAccess, FileUseCases, TelegramLookup,
};
