//! Business-logic layer of the backend.
//!
//! The crate is deliberately placed directly under `backend`: it orchestrates
//! existing domain entities and repository traits without knowing anything
//! about PostgreSQL, HTTP, Redis, or S3.
//!
//! Guarantees: public operations return `Result`; infrastructure failures are
//! propagated and no business operation hides a repository error.

pub mod services;
