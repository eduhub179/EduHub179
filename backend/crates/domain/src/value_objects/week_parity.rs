//! Value Object for lesson periodicity (week parity).
//!
//! Corresponds to the `week_parity` ENUM in PostgreSQL ('every', 'odd', 'even').
//! Guarantees: An instance can only be created from the predefined set.
//! Any attempt to parse an unknown string from the DB will return
//! `Err(DomainError::InvalidWeekParity)`.
//!
//! NOTE (2026-08-16): parity is stored but NOT read anywhere yet — availability
//! checks and week generation treat every template as 'every'. The column is kept
//! "just in case" (future every-other-week clubs/lessons); the dedup index already
//! includes parity, so odd/even twin templates coexist. See docs/SCHEDULE.en.md.

use crate::errors::DomainError;
use std::str::FromStr;

/// Periodicity of a lesson template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WeekParity {
    /// Every week (default).
    Every,
    /// Only on odd weeks.
    Odd,
    /// Only on even weeks.
    Even,
}

impl FromStr for WeekParity {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "every" => Ok(WeekParity::Every),
            "odd" => Ok(WeekParity::Odd),
            "even" => Ok(WeekParity::Even),
            _ => Err(DomainError::InvalidWeekParity),
        }
    }
}

impl std::fmt::Display for WeekParity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            WeekParity::Every => "every",
            WeekParity::Odd => "odd",
            WeekParity::Even => "even",
        };
        write!(f, "{}", s)
    }
}

// ============================================================================
// UNIT TESTS
// Запуск: `cargo test -p domain week_parity`
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_values_parse() {
        for (s, expected) in [
            ("every", WeekParity::Every),
            ("odd", WeekParity::Odd),
            ("even", WeekParity::Even),
        ] {
            assert_eq!(s.parse::<WeekParity>().unwrap(), expected, "parsing {s}");
        }
    }

    #[test]
    fn parsing_is_case_insensitive() {
        assert_eq!("EVERY".parse::<WeekParity>().unwrap(), WeekParity::Every);
    }

    #[test]
    fn unknown_value_is_rejected() {
        assert_eq!(
            "weekly".parse::<WeekParity>(),
            Err(DomainError::InvalidWeekParity)
        );
        assert_eq!("".parse::<WeekParity>(), Err(DomainError::InvalidWeekParity));
    }

    #[test]
    fn display_matches_db_enum_values() {
        assert_eq!(WeekParity::Every.to_string(), "every");
        assert_eq!(WeekParity::Odd.to_string(), "odd");
        assert_eq!(WeekParity::Even.to_string(), "even");
    }
}
