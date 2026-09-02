//! PostgreSQL connection pool factory.
//!
//! Dependencies: `sqlx` (PostgreSQL driver), `domain` crate (errors).
//! Guarantees: returns `Result`; connection problems are reported, not panicked.

use domain::errors::DomainError;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Creates the shared connection pool for all repositories.
///
/// # Fail-safe behavior
/// Returns `Err(DomainError::InternalError)` if the connection cannot be
/// established (wrong URL, database down); never panics.
pub async fn create_pool(database_url: &str) -> Result<PgPool, DomainError> {
    PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
        .map_err(|_| DomainError::InternalError)
}
