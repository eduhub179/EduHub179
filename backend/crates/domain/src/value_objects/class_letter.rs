/// Value Object for class letter.
///
/// Guarantees: An instance can only be created from a predefined set.
/// Any attempt to parse an unknown string from the DB will return `Err`,
/// preventing "garbage" letters from entering the system.

use std::str::FromStr;
use crate::errors::DomainError;

/// Class letter in the system
/// Corresponds to the `class_letter` ENUM in PostgreSQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClassLetter {
    /// 'б'
    B,
    /// 'в'
    V,
    /// 'г'
    G,
    /// 'д'
    D,
    /// 'и'
    I,
    /// 'к'
    K,
    /// 'л'
    L,
}

impl FromStr for ClassLetter {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "б" => Ok(ClassLetter::B),
            "в" => Ok(ClassLetter::V),
            "г" => Ok(ClassLetter::G),
            "д" => Ok(ClassLetter::D),
            "и" => Ok(ClassLetter::I),
            "к" => Ok(ClassLetter::K),
            "л" => Ok(ClassLetter::L),
            _ => Err(DomainError::InvalidClassLetter),
        }
    }
}

impl std::fmt::Display for ClassLetter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ClassLetter::B => "б",
            ClassLetter::V => "в",
            ClassLetter::G => "г",
            ClassLetter::D => "д",
            ClassLetter::I => "и",
            ClassLetter::K => "к",
            ClassLetter::L => "л",
        };
        write!(f, "{}", s)
    }
}
