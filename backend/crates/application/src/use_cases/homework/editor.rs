//! Who may change homework (text, files, deletion).
//!
//! Rule: teachers and admins; through the agent — anyone. The agent records homework a
//! student sent to the bot in DM on behalf of that student, after the bot's moderator
//! confirmed it, so the student's role alone does not decide there.
//!
//! The check lives here, not in HTTP handlers: every homework use case that changes
//! data takes a `HomeworkEditor`, and the only way to get one is `HomeworkEditor::authorize`.
//! So no entry point (HTTP, Telegram bot, a future worker) can skip the rule.
//!
//! Dependencies: `domain` (User, UserRole, DomainError).
//! Guarantees: no panics; `authorize` is the single place of the rule.

use domain::{entities::user::User, errors::DomainError, value_objects::role::UserRole};
use uuid::Uuid;

/// Proof that a user passed the homework editing rule.
///
/// Fields are private: a value exists only after `authorize` succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HomeworkEditor {
    user_id: Uuid,
    role: UserRole,
    via_agent: bool,
}

impl HomeworkEditor {
    /// Checks the rule for `user`.
    ///
    /// `via_agent` must be true only for a request authenticated with the agent's key —
    /// the caller (presentation) is the one that knows the channel.
    ///
    /// # Fail-safe поведение
    /// `UserIsInactive` for a deactivated account; `InsufficientPermissions` for a
    /// student outside the agent channel.
    pub fn authorize(user: &User, via_agent: bool) -> Result<Self, DomainError> {
        if !user.is_active {
            return Err(DomainError::UserIsInactive);
        }
        if !(via_agent || user.is_teacher() || user.is_admin()) {
            return Err(DomainError::InsufficientPermissions);
        }
        Ok(Self {
            user_id: user.id,
            role: user.role,
            via_agent,
        })
    }

    /// The user the change is recorded for (`created_by`, `uploaded_by`).
    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    /// Role at the moment of the change (`created_by_role`).
    pub fn role(&self) -> UserRole {
        self.role
    }

    /// True if the change came through the agent.
    pub fn via_agent(&self) -> bool {
        self.via_agent
    }

    /// An editor without a `User`, for unit tests of this crate only.
    #[cfg(test)]
    pub(crate) fn for_tests(user_id: Uuid, role: UserRole) -> Self {
        Self { user_id, role, via_agent: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(role: UserRole, is_active: bool) -> User {
        let mut user = User::try_new(
            Uuid::new_v4(),
            "user@179.ru".to_string(),
            role,
            "Фамилия".to_string(),
            "Имя".to_string(),
            None,
            None,
        )
        .unwrap();
        user.is_active = is_active;
        user
    }

    #[test]
    fn teachers_and_admins_edit() {
        assert!(HomeworkEditor::authorize(&user(UserRole::Teacher, true), false).is_ok());
        assert!(HomeworkEditor::authorize(&user(UserRole::Admin, true), false).is_ok());
    }

    #[test]
    fn student_edits_only_through_agent() {
        let student = user(UserRole::Student, true);
        assert_eq!(
            HomeworkEditor::authorize(&student, false).unwrap_err(),
            DomainError::InsufficientPermissions
        );
        let editor = HomeworkEditor::authorize(&student, true).unwrap();
        assert_eq!(editor.role(), UserRole::Student);
        assert!(editor.via_agent());
    }

    #[test]
    fn inactive_users_never_edit() {
        let teacher = user(UserRole::Teacher, false);
        assert_eq!(HomeworkEditor::authorize(&teacher, true).unwrap_err(), DomainError::UserIsInactive);
    }
}
