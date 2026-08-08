use std::str::FromStr;
use crate::errors::DomainError;

/// Value Object: Class letter.
///
/// Corresponds to the `class_letter` ENUM in PostgreSQL.
/// Guarantees: An instance can only be created from a predefined set ('б', 'в', 'и').
/// Any attempt to parse an unknown string from the DB will return `Err`,
/// preventing "garbage" letters from entering the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClassLetter {
    /// 'б'
    B,
    /// 'в'
    V,
    /// 'и'
    I,
}

impl FromStr for ClassLetter {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "б" => Ok(ClassLetter::B),
            "в" => Ok(ClassLetter::V),
            "и" => Ok(ClassLetter::I),
            _ => Err(DomainError::InvalidClassLetter),
        }
    }
}

impl std::fmt::Display for ClassLetter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ClassLetter::B => "б",
            ClassLetter::V => "в",
            ClassLetter::I => "и",
        };
        write!(f, "{}", s)
    }
}