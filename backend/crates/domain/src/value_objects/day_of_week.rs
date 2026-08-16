//! Value Object for the day of the week.
//!
//! Corresponds to the `day_of_week` ENUM in PostgreSQL ('пн', 'вт', 'ср', 'чт', 'пт', 'сб').
//! Guarantees: An instance can only be created from the predefined set.
//! Any attempt to parse an unknown string from the DB will return
//! `Err(DomainError::InvalidDayOfWeek)`, preventing garbage values from entering the system.

use crate::errors::DomainError;
use std::str::FromStr;

/// Day of the week in the school schedule (Monday — Saturday).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DayOfWeek {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
}

impl FromStr for DayOfWeek {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "пн" => Ok(DayOfWeek::Mon),
            "вт" => Ok(DayOfWeek::Tue),
            "ср" => Ok(DayOfWeek::Wed),
            "чт" => Ok(DayOfWeek::Thu),
            "пт" => Ok(DayOfWeek::Fri),
            "сб" => Ok(DayOfWeek::Sat),
            _ => Err(DomainError::InvalidDayOfWeek),
        }
    }
}

impl std::fmt::Display for DayOfWeek {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            DayOfWeek::Mon => "пн",
            DayOfWeek::Tue => "вт",
            DayOfWeek::Wed => "ср",
            DayOfWeek::Thu => "чт",
            DayOfWeek::Fri => "пт",
            DayOfWeek::Sat => "сб",
        };
        write!(f, "{}", s)
    }
}

// ============================================================================
// UNIT TESTS
// Запуск: `cargo test -p domain day_of_week`
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_days_parse_from_russian_abbreviations() {
        for (s, expected) in [
            ("пн", DayOfWeek::Mon),
            ("вт", DayOfWeek::Tue),
            ("ср", DayOfWeek::Wed),
            ("чт", DayOfWeek::Thu),
            ("пт", DayOfWeek::Fri),
            ("сб", DayOfWeek::Sat),
        ] {
            assert_eq!(s.parse::<DayOfWeek>().unwrap(), expected, "parsing {s}");
        }
    }

    #[test]
    fn parsing_is_case_insensitive() {
        assert_eq!("ПН".parse::<DayOfWeek>().unwrap(), DayOfWeek::Mon);
        assert_eq!("Сб".parse::<DayOfWeek>().unwrap(), DayOfWeek::Sat);
    }

    #[test]
    fn unknown_day_is_rejected() {
        assert_eq!(
            "вс".parse::<DayOfWeek>(),
            Err(DomainError::InvalidDayOfWeek)
        );
        assert_eq!(
            "monday".parse::<DayOfWeek>(),
            Err(DomainError::InvalidDayOfWeek)
        );
        assert_eq!(
            "".parse::<DayOfWeek>(),
            Err(DomainError::InvalidDayOfWeek)
        );
    }

    #[test]
    fn display_matches_db_enum_values() {
        assert_eq!(DayOfWeek::Mon.to_string(), "пн");
        assert_eq!(DayOfWeek::Sat.to_string(), "сб");
    }
}
