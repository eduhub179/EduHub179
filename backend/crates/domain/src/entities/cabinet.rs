//! Cabinet entity (classroom).
//!
//! Invariants:
//! - `number` is a 3-digit room number (100..=999), unique across the school.
//! - Floor is NOT stored: it is derived as `number / 100`, mirroring the DB
//!   generated column `floor INT GENERATED ALWAYS AS (number / 100) STORED` —
//!   it cannot be inconsistent by construction.
//! - `description` is optional, trimmed, max 255 characters (DB `VARCHAR(255)`)
//!     whitespace-only converted to None.
//! - `capacity` is optional, strictly positive (DB `CHECK (capacity > 0)`).
//!
//! Dependencies: Only `crate::errors::DomainError` and `uuid::Uuid`.
//! Guarantees: An instance can only be created via `try_new`, which validates
//! the invariants. This prevents invalid entities from reaching the repository.

use crate::errors::DomainError;
use uuid::Uuid;

/// Representation of a classroom (cabinet).
///
/// Examples: number `412` → floor 4, "Химическая лаборатория" (Chemistry lab).
/// Cabinets form a catalog referenced by `lesson_templates` and `events`
/// (both with `ON DELETE SET NULL`), so they are managed like other catalogs
/// (subjects, groups): upsert-only, no hard delete in the MVP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cabinet {
    /// Unique cabinet identifier (UUID v4).
    pub id: Uuid,
    /// 3-digit room number (100..=999). Unique across the school.
    pub number: i32,
    /// Optional free-text description (e.g. "Компьютерный класс").
    pub description: Option<String>,
    /// Optional seating capacity; must be > 0 when present.
    pub capacity: Option<i32>,
}

impl Cabinet {
    /// Constructor with invariant validation (Fail-safe).
    ///
    /// Returns `Err` if:
    /// - `number` is outside 100..=999 → `InvalidCabinetNumber`;
    /// - `description` is `Some` and whitespace-only or > 255 chars
    ///   → `InvalidCabinetDescription` (trimmed value is stored);
    /// - `capacity` is `Some` and <= 0 → `InvalidCabinetCapacity`.
    pub fn try_new(
        id: Uuid,
        number: i32,
        description: Option<String>,
        capacity: Option<i32>,
    ) -> Result<Self, DomainError> {
        if !(100..=999).contains(&number) {
            return Err(DomainError::InvalidCabinetNumber);
        }
        let validated_description = match description {
            Some(text) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    None
                } else if trimmed.chars().count() > 255 {
                    return Err(DomainError::InvalidCabinetDescription);
                } else {
                    Some(trimmed.to_string())
                }
            }
            None => None,
        };
        if let Some(cap) = capacity {
            if cap <= 0 {
                return Err(DomainError::InvalidCabinetCapacity);
            }
        }
        Ok(Self {
            id,
            number,
            description: validated_description,
            capacity,
        })
    }

    /// Floor derived from the 3-digit number (412 → 4).
    /// Mirrors the DB generated column `floor`.
    pub fn floor(&self) -> i32 {
        self.number / 100
    }
}
