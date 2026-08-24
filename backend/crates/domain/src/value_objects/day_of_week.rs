//! Value Object for the day of the week.
//!
//! Corresponds to the `day_of_week` ENUM in PostgreSQL
//! ('mon', 'tue', 'wed', 'thu', 'fri', 'sat').
//! Guarantees: An instance can only be created from the predefined set.
//! Any attempt to parse an unknown string from the DB will return
//! `Err(DomainError::InvalidDayOfWeek)`, preventing garbage values from entering the system.

use crate::errors::DomainError;
use std::str::FromStr;

/// Day of the week in the school schedule (Monday — Saturday).
/// No Sunday: only events can take place on Sunday (see `LessonInstance`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DayOfWeek {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
}

impl DayOfWeek {
    /// Offset in days from Monday (Mon = 0, Sat = 5).
    /// Used to derive a concrete lesson date from a template:
    /// `lesson_date = week_start_date + num_days_from_monday()`.
    pub fn num_days_from_monday(self) -> u32 {
        self as u32
    }
}

impl FromStr for DayOfWeek {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "mon" => Ok(DayOfWeek::Mon),
            "tue" => Ok(DayOfWeek::Tue),
            "wed" => Ok(DayOfWeek::Wed),
            "thu" => Ok(DayOfWeek::Thu),
            "fri" => Ok(DayOfWeek::Fri),
            "sat" => Ok(DayOfWeek::Sat),
            _ => Err(DomainError::InvalidDayOfWeek),
        }
    }
}

impl std::fmt::Display for DayOfWeek {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            DayOfWeek::Mon => "mon",
            DayOfWeek::Tue => "tue",
            DayOfWeek::Wed => "wed",
            DayOfWeek::Thu => "thu",
            DayOfWeek::Fri => "fri",
            DayOfWeek::Sat => "sat",
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
    fn all_days_parse_from_english_abbreviations() {
        for (s, expected) in [
            ("mon", DayOfWeek::Mon),
            ("tue", DayOfWeek::Tue),
            ("wed", DayOfWeek::Wed),
            ("thu", DayOfWeek::Thu),
            ("fri", DayOfWeek::Fri),
            ("sat", DayOfWeek::Sat),
        ] {
            assert_eq!(s.parse::<DayOfWeek>().unwrap(), expected, "parsing {s}");
        }
    }

    #[test]
    fn parsing_is_case_insensitive() {
        assert_eq!("MON".parse::<DayOfWeek>().unwrap(), DayOfWeek::Mon);
        assert_eq!("Sat".parse::<DayOfWeek>().unwrap(), DayOfWeek::Sat);
    }

    #[test]
    fn unknown_day_is_rejected() {
        // The old Russian values are gone, and full English names are not accepted.
        assert_eq!(
            "вс".parse::<DayOfWeek>(),
            Err(DomainError::InvalidDayOfWeek)
        );
        assert_eq!(
            "пн".parse::<DayOfWeek>(),
            Err(DomainError::InvalidDayOfWeek)
        );
        assert_eq!(
            "sun".parse::<DayOfWeek>(),
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
        assert_eq!(DayOfWeek::Mon.to_string(), "mon");
        assert_eq!(DayOfWeek::Sat.to_string(), "sat");
    }

    #[test]
    fn offsets_start_at_monday() {
        assert_eq!(DayOfWeek::Mon.num_days_from_monday(), 0);
        assert_eq!(DayOfWeek::Tue.num_days_from_monday(), 1);
        assert_eq!(DayOfWeek::Wed.num_days_from_monday(), 2);
        assert_eq!(DayOfWeek::Sat.num_days_from_monday(), 5);
    }
}
