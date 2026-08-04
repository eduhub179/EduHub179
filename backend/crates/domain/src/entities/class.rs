//! Class entity.
//!
//! Invariants:
//! - `letter` must be a valid class letter (enforced by the `ClassLetter` enum).
//! - `graduation_year` is integer more than 1900 and less than 2200;
//!   via `CHECK` constraints, keeping the domain model lightweight.
use crate::errors::DomainError;
use crate::value_objects::class_letter::ClassLetter;
use uuid::Uuid;



/// Representation of a school class.
///
/// A class is defined by its graduation year and letter (e.g., "2027б").
/// The `is_active` flag allows soft-deleting classes without breaking
/// historical data (e.g., past homework or grades).
#[derive(Debug, Clone, PartialEq)]
pub struct Class {
    /// Unique class identifier (UUID v4).
    pub id: Uuid,
    /// Graduation year (e.g., 2025).
    pub graduation_year: i32,
    /// Class letter (Value Object).
    pub letter: ClassLetter,
    /// Activity flag. Inactive classes are hidden from active schedules.
    pub is_active: bool,
}

impl Class {
    /// Constructor.
    ///
    /// Guarantees: 1901 <= graduation_year < 2200
    pub fn try_new(
        id: Uuid,
        graduation_year: i32,
        letter: ClassLetter,
        is_active: bool,
    ) -> Result<Self, DomainError> {
        if !(1901..=2200).contains(&graduation_year) {
            return Err(DomainError::InvalidGraduationYear);
        }
        Ok(Self { id, graduation_year, letter, is_active })
    }

    /// Returns the full class name (e.g., "2027б").
    /// Useful for UI display and logging.
    pub fn full_name(&self) -> String {
        format!("{}{}", self.graduation_year, self.letter)
    }
}