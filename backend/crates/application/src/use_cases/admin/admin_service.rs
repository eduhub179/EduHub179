//! Admin panel use cases.
//!
//! Access: only users with role `admin`. The check is repeated here (not only in
//! the HTTP layer), so a new caller of the service can't skip it.
//!
//! Self-protection: an admin can't change their own role or deactivate themselves,
//! so the team can't lock itself out of the panel by a misclick.
//!
//! Dependencies: `domain` (repositories), `ScheduleService` (lesson display data, weeks).
//! Guarantees: all methods return `Result`, no panics.

use std::sync::Arc;

use chrono::{Datelike, NaiveDate, Utc, Weekday};
use domain::{
    entities::{
        announcement::Announcement, cabinet::Cabinet, lesson_substitution::LessonSubstitution,
        schedule_week::ScheduleWeek, user::User,
    },
    errors::DomainError,
    ports::auth::CredentialsStore,
    repositories::{
        admin_repository::{AdminAnnouncementRow, AdminRepository, AdminUserRow, UserFilter},
        announcement_repository::AnnouncementRepository,
        cabinet_repository::CabinetRepository,
        class_repository::ClassRepository,
        lesson_instance_repository::LessonInstanceRepository,
        lesson_substitution_repository::LessonSubstitutionRepository,
        user_repository::UserRepository,
    },
    value_objects::{
        announcement_priority::AnnouncementPriority, announcement_status::AnnouncementStatus,
        lesson_instance_status::LessonInstanceStatus, login::Login, role::UserRole,
    },
};
use uuid::Uuid;

use crate::use_cases::schedule::schedule::{ScheduleDay, ScheduleLesson, ScheduleService};

/// Most users returned by one search; the panel asks to narrow the search beyond it.
pub const USER_LIST_LIMIT: i64 = 500;

/// A new user. The login is the part of the school email before `@`.
#[derive(Debug, Clone)]
pub struct NewUser {
    pub login: String,
    pub role: UserRole,
    pub last_name: String,
    pub first_name: String,
    pub middle_name: Option<String>,
    /// Only for students; ignored for staff.
    pub class_id: Option<Uuid>,
}

/// Fields to change; `None` keeps the current value. For `Option<Option<_>>`
/// fields `Some(None)` clears the value.
#[derive(Debug, Clone, Default)]
pub struct UserChange {
    pub role: Option<UserRole>,
    pub class_id: Option<Option<Uuid>>,
    pub is_active: Option<bool>,
    pub last_name: Option<String>,
    pub first_name: Option<String>,
    pub middle_name: Option<Option<String>>,
}

/// Change of one lesson instance; same `Option` rules as `UserChange`.
#[derive(Debug, Clone, Default)]
pub struct LessonChange {
    pub status: Option<LessonInstanceStatus>,
    /// `Some(None)` returns the lesson to the cabinet of its template.
    pub cabinet_id: Option<Option<Uuid>>,
    pub substitute_teacher_id: Option<Option<Uuid>>,
    pub note: Option<Option<String>>,
}

/// How to fill a new schedule week.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeekSource {
    /// No lessons yet.
    Empty,
    /// One lesson per active template (the regular timetable).
    Templates,
    /// Scheduled lessons of another week, without its cancellations and substitutions.
    CopyFrom(NaiveDate),
}

/// Content and targets of an announcement. Empty target lists = everyone.
#[derive(Debug, Clone)]
pub struct AnnouncementInput {
    pub title: Option<String>,
    pub content: Option<String>,
    pub status: AnnouncementStatus,
    pub priority: AnnouncementPriority,
    pub expires_at: Option<chrono::DateTime<Utc>>,
    pub class_ids: Vec<Uuid>,
    pub group_ids: Vec<Uuid>,
    pub user_ids: Vec<Uuid>,
}

pub struct AdminService {
    admin: Arc<dyn AdminRepository>,
    users: Arc<dyn UserRepository>,
    classes: Arc<dyn ClassRepository>,
    cabinets: Arc<dyn CabinetRepository>,
    instances: Arc<dyn LessonInstanceRepository>,
    substitutions: Arc<dyn LessonSubstitutionRepository>,
    announcements: Arc<dyn AnnouncementRepository>,
    schedule: Arc<ScheduleService>,
    /// Passwords for website login; `None` = setting passwords is unavailable.
    credentials: Option<Arc<dyn CredentialsStore>>,
}

/// Length limits of a password set by an admin (as in `make-admin.sh`).
const PASSWORD_CHARS: std::ops::RangeInclusive<usize> = 8..=128;

fn require_admin(actor: &User) -> Result<(), DomainError> {
    if actor.is_admin() && actor.is_active {
        Ok(())
    } else {
        Err(DomainError::InsufficientPermissions)
    }
}

/// Empty or whitespace-only text is treated as absent.
fn non_empty(text: Option<String>) -> Option<String> {
    text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

impl AdminService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        admin: Arc<dyn AdminRepository>,
        users: Arc<dyn UserRepository>,
        classes: Arc<dyn ClassRepository>,
        cabinets: Arc<dyn CabinetRepository>,
        instances: Arc<dyn LessonInstanceRepository>,
        substitutions: Arc<dyn LessonSubstitutionRepository>,
        announcements: Arc<dyn AnnouncementRepository>,
        schedule: Arc<ScheduleService>,
    ) -> Self {
        Self {
            admin,
            users,
            classes,
            cabinets,
            instances,
            substitutions,
            announcements,
            schedule,
            credentials: None,
        }
    }

    /// Lets admins set website passwords for users.
    pub fn with_credentials(mut self, credentials: Arc<dyn CredentialsStore>) -> Self {
        self.credentials = Some(credentials);
        self
    }

    // ------------------------------------------------------------------
    // Users
    // ------------------------------------------------------------------

    pub async fn list_users(&self, actor: &User, filter: UserFilter) -> Result<Vec<AdminUserRow>, DomainError> {
        require_admin(actor)?;
        self.admin.list_users(&filter, USER_LIST_LIMIT).await
    }

    /// Creates an active user without a password: they log in by a code sent to the school email.
    ///
    /// # Fail-safe поведение
    /// `InvalidLoginFormat` / `InvalidNameFormat` for bad input, `LoginAlreadyExists`
    /// for a taken login, `ClassNotFound` for a missing class.
    pub async fn create_user(&self, actor: &User, new_user: NewUser) -> Result<User, DomainError> {
        require_admin(actor)?;
        let login = Login::try_new(&new_user.login)?;
        let class_id = match new_user.role {
            UserRole::Student => new_user.class_id,
            _ => None,
        };
        if let Some(class_id) = class_id {
            self.classes.get_by_id(class_id).await?;
        }
        let user = User::try_new(
            Uuid::new_v4(),
            login.as_str().to_string(),
            new_user.role,
            new_user.last_name.trim().to_string(),
            new_user.first_name.trim().to_string(),
            non_empty(new_user.middle_name),
            class_id,
        )?;
        self.users.save(user).await
    }

    /// Changes role, class, names or the active flag of a user.
    ///
    /// # Fail-safe поведение
    /// `InsufficientPermissions` when an admin changes their own role or deactivates
    /// themselves. A user who stops being a student loses the class.
    pub async fn update_user(&self, actor: &User, user_id: Uuid, change: UserChange) -> Result<User, DomainError> {
        require_admin(actor)?;
        let mut user = self.users.get_by_id(user_id).await?;
        if user.id == actor.id
            && (change.role.is_some_and(|role| role != user.role) || change.is_active == Some(false))
        {
            return Err(DomainError::InsufficientPermissions);
        }

        if let Some(role) = change.role {
            user.role = role;
        }
        if let Some(class_id) = change.class_id {
            if let Some(id) = class_id {
                self.classes.get_by_id(id).await?;
            }
            user.class_id = class_id;
        }
        if !user.role.is_student() {
            user.class_id = None;
        }
        if let Some(is_active) = change.is_active {
            user.is_active = is_active;
        }
        if let Some(last_name) = change.last_name {
            user.last_name = last_name.trim().to_string();
        }
        if let Some(first_name) = change.first_name {
            user.first_name = first_name.trim().to_string();
        }
        if let Some(middle_name) = change.middle_name {
            user.middle_name = non_empty(middle_name);
        }
        if user.last_name.is_empty() || user.first_name.is_empty() {
            return Err(DomainError::InvalidNameFormat);
        }
        self.users.save(user).await
    }

    /// Sets the user's website password (replacing the old one), so that a user created
    /// in the panel can log in.
    ///
    /// # Fail-safe поведение
    /// `InvalidPasswordFormat` outside 8..=128 chars, `UserNotFound` for a missing user,
    /// `InternalError` when the service was built without a credentials store.
    pub async fn set_password(&self, actor: &User, user_id: Uuid, password: &str) -> Result<(), DomainError> {
        require_admin(actor)?;
        if !PASSWORD_CHARS.contains(&password.chars().count()) {
            return Err(DomainError::InvalidPasswordFormat);
        }
        let credentials = self.credentials.as_ref().ok_or(DomainError::InternalError)?;
        self.users.get_by_id(user_id).await?;
        credentials.set_password(user_id, password).await
    }

    // ------------------------------------------------------------------
    // Lessons of a day
    // ------------------------------------------------------------------

    pub async fn cabinets(&self, actor: &User) -> Result<Vec<Cabinet>, DomainError> {
        require_admin(actor)?;
        self.cabinets.get_all().await
    }

    /// Adds a cabinet (100..=999; the floor is the first digit).
    ///
    /// # Fail-safe поведение
    /// `InvalidCabinetNumber` out of range, `CabinetAlreadyExists` for a taken number.
    pub async fn create_cabinet(&self, actor: &User, number: i32, description: Option<String>) -> Result<Cabinet, DomainError> {
        require_admin(actor)?;
        let cabinet = Cabinet::try_new(Uuid::new_v4(), number, non_empty(description), None)?;
        self.cabinets.save(cabinet).await
    }

    /// Every lesson of a date, also in a draft week, with substitutions.
    pub async fn lessons_for_date(&self, actor: &User, date: NaiveDate) -> Result<ScheduleDay, DomainError> {
        require_admin(actor)?;
        self.schedule.all_lessons_for_date(date).await
    }

    /// Cancels or restores a lesson, moves it to another cabinet, sets a substitute
    /// teacher or a note. Returns the lesson as the schedule shows it now.
    ///
    /// # Fail-safe поведение
    /// `LessonInstanceNotFound`, `CabinetNotFound`; `UserNotFound` for a substitute
    /// who doesn't exist or is a student; `UserIsInactive` for a deactivated one.
    pub async fn update_lesson(
        &self,
        actor: &User,
        instance_id: Uuid,
        change: LessonChange,
    ) -> Result<ScheduleLesson, DomainError> {
        require_admin(actor)?;
        let mut instance = self.instances.get_by_id(instance_id).await?;

        if let Some(Some(cabinet_id)) = change.cabinet_id {
            self.cabinets.get_by_id(cabinet_id).await?;
        }
        if let Some(Some(teacher_id)) = change.substitute_teacher_id {
            let teacher = self.users.get_by_id(teacher_id).await?;
            if teacher.role.is_student() {
                return Err(DomainError::UserNotFound);
            }
            if !teacher.is_active {
                return Err(DomainError::UserIsInactive);
            }
        }

        if change.status.is_some() || change.cabinet_id.is_some() {
            if let Some(status) = change.status {
                instance.status = status;
            }
            if let Some(cabinet_id) = change.cabinet_id {
                instance.cabinet_id = cabinet_id;
            }
            self.instances.save(instance).await?;
        }

        if change.substitute_teacher_id.is_some() || change.note.is_some() {
            let current = self.substitutions.get(instance_id).await?;
            let teacher_id = match change.substitute_teacher_id {
                Some(value) => value,
                None => current.as_ref().and_then(|s| s.substitute_teacher_id),
            };
            let note = match change.note {
                Some(value) => value,
                None => current.and_then(|s| s.note),
            };
            match LessonSubstitution::try_new(instance_id, teacher_id, note, actor.id)? {
                Some(substitution) => {
                    self.substitutions.save(substitution).await?;
                }
                None => self.substitutions.delete(instance_id).await?,
            }
        }

        self.schedule.schedule_lesson(instance_id).await
    }

    // ------------------------------------------------------------------
    // Schedule weeks
    // ------------------------------------------------------------------

    pub async fn weeks(&self, actor: &User) -> Result<Vec<ScheduleWeek>, DomainError> {
        require_admin(actor)?;
        self.schedule.list().await
    }

    /// Creates a draft week starting on `monday` and fills it from `source`.
    ///
    /// # Fail-safe поведение
    /// `InvalidLessonInstanceDate` if `monday` is not a Monday, `ScheduleWeekAlreadyExists`
    /// if the week exists (it is never reset to a draft), `ScheduleWeekNotFound` for a
    /// missing source week.
    pub async fn create_week(&self, actor: &User, monday: NaiveDate, source: WeekSource) -> Result<ScheduleWeek, DomainError> {
        require_admin(actor)?;
        if monday.weekday() != Weekday::Mon {
            return Err(DomainError::InvalidLessonInstanceDate);
        }
        match self.find_week(monday).await {
            Ok(_) => return Err(DomainError::ScheduleWeekAlreadyExists),
            Err(DomainError::ScheduleWeekNotFound) => {}
            Err(error) => return Err(error),
        }
        match source {
            WeekSource::Empty => {
                self.schedule.create_draft(monday, None).await?;
            }
            WeekSource::Templates => {
                self.schedule.create_draft(monday, None).await?;
                self.schedule.generate_from_templates(monday).await?;
            }
            WeekSource::CopyFrom(source_monday) => {
                self.schedule.copy_week(source_monday, monday).await?;
            }
        }
        self.find_week(monday).await
    }

    /// Publishes a week (students see it) or returns it to a draft.
    pub async fn set_week_published(&self, actor: &User, monday: NaiveDate, published: bool) -> Result<ScheduleWeek, DomainError> {
        require_admin(actor)?;
        if published {
            return self.schedule.publish(monday).await;
        }
        let week = self.find_week(monday).await?;
        self.schedule.create_draft(week.week_start_date, week.copied_from).await
    }

    async fn find_week(&self, monday: NaiveDate) -> Result<ScheduleWeek, DomainError> {
        self.schedule
            .list()
            .await?
            .into_iter()
            .find(|w| w.week_start_date == monday)
            .ok_or(DomainError::ScheduleWeekNotFound)
    }

    // ------------------------------------------------------------------
    // Announcements
    // ------------------------------------------------------------------

    pub async fn list_announcements(&self, actor: &User) -> Result<Vec<AdminAnnouncementRow>, DomainError> {
        require_admin(actor)?;
        self.admin.list_announcements().await
    }

    /// # Fail-safe поведение
    /// `InvalidAnnouncementContent` when both title and text are empty.
    pub async fn create_announcement(&self, actor: &User, input: AnnouncementInput) -> Result<Announcement, DomainError> {
        require_admin(actor)?;
        let announcement = Self::build_announcement(Uuid::new_v4(), actor.id, Utc::now(), &input)?;
        let saved = self.announcements.save(announcement).await?;
        self.announcements
            .set_targets(saved.id, &input.class_ids, &input.group_ids, &input.user_ids)
            .await?;
        Ok(saved)
    }

    /// Replaces content, status and targets; the author and creation time stay.
    pub async fn update_announcement(
        &self,
        actor: &User,
        announcement_id: Uuid,
        input: AnnouncementInput,
    ) -> Result<Announcement, DomainError> {
        require_admin(actor)?;
        let current = self.announcements.get_by_id(announcement_id).await?;
        let announcement =
            Self::build_announcement(current.id, current.created_by, current.created_at, &input)?;
        let saved = self.announcements.save(announcement).await?;
        self.announcements
            .set_targets(saved.id, &input.class_ids, &input.group_ids, &input.user_ids)
            .await?;
        Ok(saved)
    }

    pub async fn delete_announcement(&self, actor: &User, announcement_id: Uuid) -> Result<(), DomainError> {
        require_admin(actor)?;
        self.announcements.delete(announcement_id).await
    }

    /// Published, unexpired announcements targeted at `user` (for any logged-in user,
    /// not only admins: the site and the bot show them).
    pub async fn visible_announcements(&self, user: &User) -> Result<Vec<Announcement>, DomainError> {
        self.announcements.get_visible_for_user(user.id).await
    }

    fn build_announcement(
        id: Uuid,
        created_by: Uuid,
        created_at: chrono::DateTime<Utc>,
        input: &AnnouncementInput,
    ) -> Result<Announcement, DomainError> {
        let title = non_empty(input.title.clone());
        let content = non_empty(input.content.clone());
        if title.is_none() && content.is_none() {
            return Err(DomainError::InvalidAnnouncementContent);
        }
        Announcement::try_new(
            id,
            title,
            content,
            created_by,
            input.status,
            input.priority,
            input.expires_at,
            created_at,
        )
    }
}
