//! Value Object for plusnik sheet status.
//!
//! Corresponds to the `sheet_status` ENUM in PostgreSQL ('draft', 'published', 'archived').
//! Guarantees: An instance can only be created from the predefined set.
//! Any attempt to parse an unknown string from the DB will return
//! `Err(DomainError::InvalidSheetStatus)`, preventing "garbage" statuses
//! from entering the system.

use crate::errors::DomainError;
use std::str::FromStr;

/// Lifecycle status of a plusnik sheet (problem worksheet).
/// Corresponds to the `sheet_status` ENUM in PostgreSQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SheetStatus {
    /// Draft: visible only to the creator (teacher), not to students.
    Draft,
    /// Published: visible to students, pluses can be awarded.
    Published,
    /// Archived: hidden from the active list, history is preserved.
    Archived,
}

impl SheetStatus {
    /// Fail-safe: explicit check for Draft status.
    pub fn is_draft(&self) -> bool {
        matches!(self, SheetStatus::Draft)
    }

    /// Fail-safe: explicit check for Published status.
    pub fn is_published(&self) -> bool {
        matches!(self, SheetStatus::Published)
    }

    /// Fail-safe: explicit check for Archived status.
    pub fn is_archived(&self) -> bool {
        matches!(self, SheetStatus::Archived)
    }

    /// Checks if the sheet is visible to students.
    /// Only Published sheets are visible to students.
    pub fn is_visible_to_students(&self) -> bool {
        matches!(self, SheetStatus::Published)
    }
}

impl FromStr for SheetStatus {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "draft" => Ok(SheetStatus::Draft),
            "published" => Ok(SheetStatus::Published),
            "archived" => Ok(SheetStatus::Archived),
            _ => Err(DomainError::InvalidSheetStatus),
        }
    }
}

impl std::fmt::Display for SheetStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            SheetStatus::Draft => "draft",
            SheetStatus::Published => "published",
            SheetStatus::Archived => "archived",
        };
        write!(f, "{}", s)
    }
}

// ============================================================================
// UNIT TESTS
// Запуск: `cargo test -p domain sheet_status`
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_values_parse() {
        assert_eq!("draft".parse::<SheetStatus>().unwrap(), SheetStatus::Draft);
        assert_eq!(
            "published".parse::<SheetStatus>().unwrap(),
            SheetStatus::Published
        );
        assert_eq!(
            "archived".parse::<SheetStatus>().unwrap(),
            SheetStatus::Archived
        );
    }

    #[test]
    fn parsing_is_case_insensitive() {
        assert_eq!(
            "PUBLISHED".parse::<SheetStatus>().unwrap(),
            SheetStatus::Published
        );
        assert_eq!(
            "Archived".parse::<SheetStatus>().unwrap(),
            SheetStatus::Archived
        );
    }

    #[test]
    fn unknown_value_is_rejected() {
        assert_eq!(
            "deleted".parse::<SheetStatus>(),
            Err(DomainError::InvalidSheetStatus)
        );
        assert_eq!(
            "".parse::<SheetStatus>(),
            Err(DomainError::InvalidSheetStatus)
        );
    }

    #[test]
    fn helpers_match_variants() {
        assert!(SheetStatus::Draft.is_draft());
        assert!(!SheetStatus::Draft.is_published());
        assert!(SheetStatus::Published.is_published());
        assert!(!SheetStatus::Published.is_draft());
        assert!(SheetStatus::Archived.is_archived());
        assert!(!SheetStatus::Archived.is_published());
    }

    #[test]
    fn only_published_is_visible_to_students() {
        assert!(!SheetStatus::Draft.is_visible_to_students());
        assert!(SheetStatus::Published.is_visible_to_students());
        assert!(!SheetStatus::Archived.is_visible_to_students());
    }

    #[test]
    fn display_matches_db_values() {
        assert_eq!(SheetStatus::Draft.to_string(), "draft");
        assert_eq!(SheetStatus::Published.to_string(), "published");
        assert_eq!(SheetStatus::Archived.to_string(), "archived");
    }
}
