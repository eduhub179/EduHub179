//! Read-only queries for the admin panel: lists with joined display data that
//! the per-entity repositories don't offer (users with class names and Telegram
//! status, every announcement with its targets).
//!
//! Implementation of this trait is located in the `infrastructure` crate.

use uuid::Uuid;

use crate::entities::announcement::Announcement;
use crate::entities::user::User;
use crate::errors::DomainError;
use crate::value_objects::role::UserRole;

/// Filter for `AdminRepository::list_users`; empty fields don't filter.
#[derive(Debug, Clone, Default)]
pub struct UserFilter {
    /// Substring of the login or the full name, case-insensitive.
    pub query: Option<String>,
    pub role: Option<UserRole>,
    pub class_id: Option<Uuid>,
    /// Deactivated users are hidden unless this is set.
    pub include_inactive: bool,
}

/// A user row of the admin user list.
#[derive(Debug, Clone, PartialEq)]
pub struct AdminUserRow {
    pub user: User,
    /// "2027б" for a student with a class.
    pub class_name: Option<String>,
    /// The user has an active Telegram binding (logged in to the bot).
    pub telegram_bound: bool,
}

/// An announcement with its targets; all target lists empty = everyone.
#[derive(Debug, Clone, PartialEq)]
pub struct AdminAnnouncementRow {
    pub announcement: Announcement,
    pub class_ids: Vec<Uuid>,
    pub group_ids: Vec<Uuid>,
    pub user_ids: Vec<Uuid>,
    /// "Фамилия Имя" of the author.
    pub author_name: String,
}

#[async_trait::async_trait]
pub trait AdminRepository: Send + Sync {
    /// Users sorted by role, then last and first name; at most `limit` rows.
    async fn list_users(&self, filter: &UserFilter, limit: i64) -> Result<Vec<AdminUserRow>, DomainError>;

    /// All announcements of every author and status, newest first.
    async fn list_announcements(&self) -> Result<Vec<AdminAnnouncementRow>, DomainError>;
}
