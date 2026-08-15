//! Repository trait for cabinet persistence.
//!
//! Dependencies: Only types from `crate::entities` and `crate::errors`.
//! Guarantees: All methods return `Result`. No panics are allowed.
//! Implementation of this trait is located in the `infrastructure` crate.

use crate::entities::cabinet::Cabinet;
use crate::errors::DomainError;
use uuid::Uuid;

/// Interface for interacting with the cabinet (classroom) storage.
///
/// Cabinets are a catalog: they are referenced by `lesson_templates` and
/// `events`, but never owned by them. Using a trait allows mocking the
/// database in use-case unit tests without spinning up a real PostgreSQL
/// instance.
#[async_trait::async_trait]
pub trait CabinetRepository: Send + Sync {
    /// Fetches a cabinet by its unique identifier.
    /// Fail-safe: Returns `CabinetNotFound` if the record doesn't exist,
    /// rather than `None` (forcing the caller to handle this case).
    async fn get_by_id(&self, cabinet_id: Uuid) -> Result<Cabinet, DomainError>;

    /// Fetches a cabinet by its unique 3-digit number.
    /// Performance: relies on the unique index on `cabinets.number`.
    /// Fail-safe: Returns `CabinetNotFound` if the record doesn't exist.
    async fn get_by_number(&self, number: i32) -> Result<Cabinet, DomainError>;

    /// Fetches all cabinets, sorted by number.
    /// Small catalog (hundreds of rows at most), so a full ordered scan is fine.
    async fn get_all(&self) -> Result<Vec<Cabinet>, DomainError>;

    /// Fetches all cabinets on a given floor, sorted by number.
    /// Performance: relies on `idx_cabinets_floor` (floor is a stored
    /// generated column, so the index is usable directly).
    async fn get_by_floor(&self, floor: i32) -> Result<Vec<Cabinet>, DomainError>;

    /// Saves or updates a cabinet.
    ///
    /// Uses PostgreSQL `INSERT ... ON CONFLICT` for atomic upsert.
    /// If a cabinet with the same `cabinet_id` exists, it updates mutable fields.
    /// If a cabinet with the same `number` exists (but different `cabinet_id`),
    /// it raises a unique violation, mapped to `DomainError::CabinetAlreadyExists`.
    async fn save(&self, cabinet: Cabinet) -> Result<Cabinet, DomainError>;
}
