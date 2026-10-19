//! Value Object for announcement lifecycle status.
//!
//! Corresponds to the `announcement_status` ENUM in PostgreSQL
//! ('draft', 'published', 'archived').
use crate::errors::DomainError;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnnouncementStatus {
    Draft,
    Published,
    Archived,
}

impl AnnouncementStatus {
    pub fn is_published(&self) -> bool {
        matches!(self, AnnouncementStatus::Published)
    }
    pub fn is_draft(&self) -> bool {
        matches!(self, AnnouncementStatus::Draft)
    }
}

impl FromStr for AnnouncementStatus {
    type Err = DomainError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "draft" => Ok(AnnouncementStatus::Draft),
            "published" => Ok(AnnouncementStatus::Published),
            "archived" => Ok(AnnouncementStatus::Archived),
            _ => Err(DomainError::InvalidAnnouncementStatus),
        }
    }
}

impl std::fmt::Display for AnnouncementStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            AnnouncementStatus::Draft => "draft",
            AnnouncementStatus::Published => "published",
            AnnouncementStatus::Archived => "archived",
        };
        write!(f, "{}", s)
    }
}
