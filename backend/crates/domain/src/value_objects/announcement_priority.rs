//! Value Object for announcement priority.
//!
//! Corresponds to the `announcement_priority` ENUM in PostgreSQL
//! ('low', 'normal', 'high', 'urgent').
use crate::errors::DomainError;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AnnouncementPriority {
    Low,
    Normal,
    High,
    Urgent,
}

impl FromStr for AnnouncementPriority {
    type Err = DomainError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "low" => Ok(AnnouncementPriority::Low),
            "normal" => Ok(AnnouncementPriority::Normal),
            "high" => Ok(AnnouncementPriority::High),
            "urgent" => Ok(AnnouncementPriority::Urgent),
            _ => Err(DomainError::InvalidAnnouncementPriority),
        }
    }
}

impl std::fmt::Display for AnnouncementPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            AnnouncementPriority::Low => "low",
            AnnouncementPriority::Normal => "normal",
            AnnouncementPriority::High => "high",
            AnnouncementPriority::Urgent => "urgent",
        };
        write!(f, "{}", s)
    }
}
