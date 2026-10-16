//! Presentation crate: HTTP API layer.
//!
//! This crate is responsible for HTTP transport, request/response DTOs,
//! auth middleware, and the Axum router that wires the application use cases.

pub mod router;

pub use router::create_app;
