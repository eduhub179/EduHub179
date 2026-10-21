//! Plusnik use cases: problem sheets, their tasks, and pluses for solved tasks.
//!
//! Teacher workflow (master doc §3, §6.4): lesson → sheet → students × tasks matrix →
//! tap a cell → plus. Student workflow: dashboard "how many tasks are solved".
//!
//! Access rules:
//! - a teacher manages sheets and pluses only of lessons they teach (`lesson_teachers`);
//!   an admin manages any lesson;
//! - a student sees published sheets of their lessons (their class or their groups);
//! - pluses are granted only on published sheets and only to students of the lesson.
//!
//! Revoking keeps the record (`revoked_at` / `revoked_by` / comment) — the audit trail
//! that settles "who put / removed this plus" disputes (master doc §11.4).
//!
//! Dependencies: `domain` (repositories, entities).
//! Guarantees: all methods return `Result`, no panics.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};
use domain::{
    entities::{
        lesson::Lesson,
        plusnik::{PlusnikRecord, PlusnikSheet, PlusnikTask},
        user::User,
    },
    errors::DomainError,
    repositories::{
        class_repository::ClassRepository, lesson_repository::LessonRepository,
        plusnik_repository::PlusnikRepository,
        student_group_repository::StudentGroupRepository,
        subject_repository::SubjectRepository, user_repository::UserRepository,
    },
    value_objects::{lesson_target::LessonTarget, role::UserRole, sheet_status::SheetStatus},
};
use uuid::Uuid;

/// A lesson as shown in a picker: "Алгебра — 2027б".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LessonSummary {
    pub lesson_id: Uuid,
    pub subject_name: String,
    /// Class ("2027б") or group name.
    pub target_name: String,
}

/// A sheet with its tasks.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetWithTasks {
    pub sheet: PlusnikSheet,
    pub tasks: Vec<PlusnikTask>,
}

/// A student row of the matrix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatrixStudent {
    pub id: Uuid,
    /// "Фамилия Имя".
    pub name: String,
}

/// The teacher's screen: tasks × students with active pluses.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetMatrix {
    pub sheet: PlusnikSheet,
    pub tasks: Vec<PlusnikTask>,
    /// Students of the lesson, sorted by name.
    pub students: Vec<MatrixStudent>,
    /// Active (not revoked) pluses of this sheet.
    pub pluses: Vec<PlusnikRecord>,
}

/// One sheet on the student dashboard.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetProgress {
    pub sheet: PlusnikSheet,
    pub subject_name: String,
    pub total_tasks: usize,
    /// Tasks of this sheet the student has an active plus for.
    pub solved_task_ids: Vec<Uuid>,
}

/// Input for creating a sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct CreateSheetCommand {
    pub lesson_id: Uuid,
    pub name: String,
    pub issue_date: NaiveDate,
    pub deadline: Option<DateTime<Utc>>,
    pub status: SheetStatus,
    /// Task numbers in display order: ["1а", "1б", "2"]. May be empty.
    pub task_numbers: Vec<String>,
}

/// Partial update of a sheet: `None` keeps the current value.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UpdateSheetCommand {
    pub name: Option<String>,
    pub issue_date: Option<NaiveDate>,
    /// `Some(None)` clears the deadline.
    pub deadline: Option<Option<DateTime<Utc>>>,
    pub status: Option<SheetStatus>,
}

/// Result of granting a plus.
#[derive(Debug, Clone, PartialEq)]
pub struct GrantedPlus {
    pub record: PlusnikRecord,
    /// False if the student already had this plus (a repeated tap is not an error).
    pub created: bool,
}

pub struct PlusnikService {
    plusnik: Arc<dyn PlusnikRepository>,
    lessons: Arc<dyn LessonRepository>,
    users: Arc<dyn UserRepository>,
    groups: Arc<dyn StudentGroupRepository>,
    subjects: Arc<dyn SubjectRepository>,
    classes: Arc<dyn ClassRepository>,
}

impl PlusnikService {
    pub fn new(
        plusnik: Arc<dyn PlusnikRepository>,
        lessons: Arc<dyn LessonRepository>,
        users: Arc<dyn UserRepository>,
        groups: Arc<dyn StudentGroupRepository>,
        subjects: Arc<dyn SubjectRepository>,
        classes: Arc<dyn ClassRepository>,
    ) -> Self {
        Self {
            plusnik,
            lessons,
            users,
            groups,
            subjects,
            classes,
        }
    }

    // ========================================================================
    // Lessons and sheets
    // ========================================================================

    /// Lessons the user can manage sheets for, sorted by subject and class/group:
    /// a teacher gets the lessons they teach, an admin every active lesson of the school.
    ///
    /// # Fail-safe поведение
    /// `InsufficientPermissions` for students.
    pub async fn teacher_lessons(&self, user: &User) -> Result<Vec<LessonSummary>, DomainError> {
        let candidates = match user.role {
            UserRole::Teacher => self.lessons.get_by_teacher(user.id).await?,
            UserRole::Admin => self.all_lessons().await?,
            UserRole::Student => return Err(DomainError::InsufficientPermissions),
        };
        let mut lessons = Vec::new();
        for lesson in candidates {
            if lesson.is_active {
                lessons.push(self.lesson_summary(&lesson).await?);
            }
        }
        lessons.sort_by(|a, b| {
            (&a.subject_name, &a.target_name).cmp(&(&b.subject_name, &b.target_name))
        });
        Ok(lessons)
    }

    /// Sheets of a lesson with their tasks, newest first.
    /// A teacher of the lesson (or admin) sees all statuses, a student of the lesson
    /// only published sheets.
    ///
    /// # Fail-safe поведение
    /// `InsufficientPermissions` for anyone else; `LessonNotFound` for an unknown lesson.
    pub async fn lesson_sheets(
        &self,
        user: &User,
        lesson_id: Uuid,
    ) -> Result<Vec<SheetWithTasks>, DomainError> {
        let lesson = self.lessons.get_by_id(lesson_id).await?;
        let published_only = if self.can_manage(user, lesson_id).await? {
            false
        } else if user.role.is_student() && self.is_in_audience(user.id, &lesson).await? {
            true
        } else {
            return Err(DomainError::InsufficientPermissions);
        };

        let mut result = Vec::new();
        for sheet in self.plusnik.get_sheets_by_lesson(lesson_id).await? {
            if published_only && sheet.status != SheetStatus::Published {
                continue;
            }
            let tasks = self.plusnik.get_tasks(sheet.id).await?;
            result.push(SheetWithTasks { sheet, tasks });
        }
        Ok(result)
    }

    /// Creates a sheet together with its tasks.
    ///
    /// # Fail-safe поведение
    /// All input (name, task numbers, duplicates) is validated before anything is saved;
    /// `InvalidPlusnikSheetName` / `InvalidTaskNumber` / `PlusnikTaskAlreadyExists` then.
    pub async fn create_sheet(
        &self,
        user: &User,
        cmd: CreateSheetCommand,
    ) -> Result<SheetWithTasks, DomainError> {
        self.require_manager(user, cmd.lesson_id).await?;

        let now = Utc::now();
        let sheet = PlusnikSheet::try_new(
            Uuid::new_v4(),
            cmd.lesson_id,
            user.id,
            cmd.name,
            cmd.issue_date,
            cmd.deadline,
            cmd.status,
            now,
        )?;
        let mut seen = HashSet::new();
        let mut tasks = Vec::with_capacity(cmd.task_numbers.len());
        for (index, number) in cmd.task_numbers.into_iter().enumerate() {
            let sort_order = i32::try_from(index).map_err(|_| DomainError::InvalidTaskNumber)?;
            let task = PlusnikTask::try_new(Uuid::new_v4(), sheet.id, number, sort_order, now)?;
            if !seen.insert(task.task_number.clone()) {
                return Err(DomainError::PlusnikTaskAlreadyExists);
            }
            tasks.push(task);
        }

        let sheet = self.plusnik.save_sheet(sheet).await?;
        let mut saved_tasks = Vec::with_capacity(tasks.len());
        for task in tasks {
            saved_tasks.push(self.plusnik.save_task(task).await?);
        }
        Ok(SheetWithTasks {
            sheet,
            tasks: saved_tasks,
        })
    }

    /// Changes name, dates or status (publish / archive) of a sheet.
    pub async fn update_sheet(
        &self,
        user: &User,
        sheet_id: Uuid,
        cmd: UpdateSheetCommand,
    ) -> Result<SheetWithTasks, DomainError> {
        let sheet = self.plusnik.get_sheet_by_id(sheet_id).await?;
        self.require_manager(user, sheet.lesson_id).await?;

        let updated = PlusnikSheet::try_new(
            sheet.id,
            sheet.lesson_id,
            sheet.created_by,
            cmd.name.unwrap_or(sheet.name),
            cmd.issue_date.unwrap_or(sheet.issue_date),
            cmd.deadline.unwrap_or(sheet.deadline),
            cmd.status.unwrap_or(sheet.status),
            sheet.created_at,
        )?;
        let sheet = self.plusnik.save_sheet(updated).await?;
        let tasks = self.plusnik.get_tasks(sheet_id).await?;
        Ok(SheetWithTasks { sheet, tasks })
    }

    /// Deletes a sheet with its tasks.
    ///
    /// # Fail-safe поведение
    /// `PlusnikSheetHasRecords` if any plus (even revoked) exists: history is not
    /// deleted, archive the sheet instead.
    pub async fn delete_sheet(&self, user: &User, sheet_id: Uuid) -> Result<(), DomainError> {
        let sheet = self.plusnik.get_sheet_by_id(sheet_id).await?;
        self.require_manager(user, sheet.lesson_id).await?;
        self.plusnik.delete_sheet(sheet_id).await
    }

    // ========================================================================
    // Tasks
    // ========================================================================

    /// Appends a task to the end of the sheet.
    ///
    /// # Fail-safe поведение
    /// `PlusnikTaskAlreadyExists` if the sheet already has a task with this number.
    pub async fn add_task(
        &self,
        user: &User,
        sheet_id: Uuid,
        task_number: String,
    ) -> Result<PlusnikTask, DomainError> {
        let sheet = self.plusnik.get_sheet_by_id(sheet_id).await?;
        self.require_manager(user, sheet.lesson_id).await?;

        let sort_order = self
            .plusnik
            .get_tasks(sheet_id)
            .await?
            .iter()
            .map(|task| task.sort_order)
            .max()
            .map_or(0, |last| last.saturating_add(1));
        let task = PlusnikTask::try_new(Uuid::new_v4(), sheet_id, task_number, sort_order, Utc::now())?;
        self.plusnik.save_task(task).await
    }

    /// Deletes a task.
    ///
    /// # Fail-safe поведение
    /// `PlusnikTaskHasRecords` if any plus (even revoked) was given for it.
    pub async fn delete_task(&self, user: &User, task_id: Uuid) -> Result<(), DomainError> {
        let task = self.plusnik.get_task_by_id(task_id).await?;
        let sheet = self.plusnik.get_sheet_by_id(task.sheet_id).await?;
        self.require_manager(user, sheet.lesson_id).await?;
        self.plusnik.delete_task(task_id).await
    }

    // ========================================================================
    // Matrix and pluses
    // ========================================================================

    /// The teacher's matrix: tasks × students of the lesson with active pluses.
    pub async fn sheet_matrix(&self, user: &User, sheet_id: Uuid) -> Result<SheetMatrix, DomainError> {
        let sheet = self.plusnik.get_sheet_by_id(sheet_id).await?;
        self.require_manager(user, sheet.lesson_id).await?;
        let lesson = self.lessons.get_by_id(sheet.lesson_id).await?;

        let mut students: Vec<MatrixStudent> = self
            .lesson_students(&lesson)
            .await?
            .into_iter()
            .map(|student| MatrixStudent {
                id: student.id,
                name: format!("{} {}", student.last_name, student.first_name),
            })
            .collect();
        students.sort_by(|a, b| a.name.cmp(&b.name));

        let tasks = self.plusnik.get_tasks(sheet_id).await?;
        let pluses = self
            .plusnik
            .get_records_by_sheet(sheet_id)
            .await?
            .into_iter()
            .filter(PlusnikRecord::is_active)
            .collect();

        Ok(SheetMatrix {
            sheet,
            tasks,
            students,
            pluses,
        })
    }

    /// Puts a plus for `task_id` to `student_id`.
    ///
    /// # Fail-safe поведение
    /// Idempotent: if the plus is already there, returns it with `created: false`
    /// (double taps and two teachers at once are not errors).
    /// `PlusnikSheetNotPublished` for draft/archived sheets,
    /// `StudentNotInLesson` if the student is not in the lesson's class/group.
    pub async fn grant_plus(
        &self,
        user: &User,
        task_id: Uuid,
        student_id: Uuid,
    ) -> Result<GrantedPlus, DomainError> {
        let task = self.plusnik.get_task_by_id(task_id).await?;
        let sheet = self.plusnik.get_sheet_by_id(task.sheet_id).await?;
        self.require_manager(user, sheet.lesson_id).await?;
        if sheet.status != SheetStatus::Published {
            return Err(DomainError::PlusnikSheetNotPublished);
        }
        let lesson = self.lessons.get_by_id(sheet.lesson_id).await?;
        if !self.is_in_audience(student_id, &lesson).await? {
            return Err(DomainError::StudentNotInLesson);
        }

        if let Some(record) = self.active_plus(sheet.id, task_id, student_id).await? {
            return Ok(GrantedPlus {
                record,
                created: false,
            });
        }
        let record = PlusnikRecord::try_new_active(
            Uuid::new_v4(),
            student_id,
            sheet.id,
            task_id,
            user.id,
            Utc::now(),
        )?;
        match self.plusnik.save_record(record).await {
            Ok(record) => Ok(GrantedPlus {
                record,
                created: true,
            }),
            // Another teacher put the same plus between our check and insert.
            Err(DomainError::PlusnikRecordAlreadyExists) => self
                .active_plus(sheet.id, task_id, student_id)
                .await?
                .map(|record| GrantedPlus {
                    record,
                    created: false,
                })
                .ok_or(DomainError::PlusnikRecordAlreadyExists),
            Err(error) => Err(error),
        }
    }

    /// Removes the plus for `task_id` from `student_id`. The record stays in history
    /// with who/when/why.
    ///
    /// # Fail-safe поведение
    /// Idempotent: returns `Ok(false)` if there was no active plus.
    /// Unlike granting, works on any sheet status (a mistake can be fixed after archiving).
    pub async fn revoke_plus(
        &self,
        user: &User,
        task_id: Uuid,
        student_id: Uuid,
        comment: Option<String>,
    ) -> Result<bool, DomainError> {
        let task = self.plusnik.get_task_by_id(task_id).await?;
        let sheet = self.plusnik.get_sheet_by_id(task.sheet_id).await?;
        self.require_manager(user, sheet.lesson_id).await?;

        let Some(record) = self.active_plus(sheet.id, task_id, student_id).await? else {
            return Ok(false);
        };
        let comment = comment
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty());
        match self.plusnik.revoke_plus(record.id, user.id, comment).await {
            Ok(()) => Ok(true),
            // Revoked concurrently by someone else.
            Err(DomainError::PlusnikRecordNotFound) => Ok(false),
            Err(error) => Err(error),
        }
    }

    // ========================================================================
    // Student dashboard
    // ========================================================================

    /// Published sheets of the student's lessons with solved/total counts,
    /// newest first.
    ///
    /// # Fail-safe поведение
    /// `InsufficientPermissions` for non-students.
    pub async fn student_progress(&self, user: &User) -> Result<Vec<SheetProgress>, DomainError> {
        if !user.role.is_student() {
            return Err(DomainError::InsufficientPermissions);
        }

        let mut lessons = Vec::new();
        if let Some(class_id) = user.class_id {
            lessons.extend(self.lessons.get_by_class(class_id).await?);
        }
        for group in self.groups.get_groups_by_student(user.id).await? {
            lessons.extend(self.lessons.get_by_group(group.id).await?);
        }

        let solved: HashSet<Uuid> = self
            .plusnik
            .get_active_records_by_student(user.id)
            .await?
            .into_iter()
            .map(|record| record.task_id)
            .collect();
        let mut subject_names: HashMap<Uuid, String> = HashMap::new();
        let mut progress = Vec::new();

        for lesson in lessons.into_iter().filter(|lesson| lesson.is_active) {
            let sheets: Vec<PlusnikSheet> = self
                .plusnik
                .get_sheets_by_lesson(lesson.id)
                .await?
                .into_iter()
                .filter(|sheet| sheet.status == SheetStatus::Published)
                .collect();
            if sheets.is_empty() {
                continue;
            }
            let subject_name = match subject_names.get(&lesson.subject_id) {
                Some(name) => name.clone(),
                None => {
                    let name = self.subjects.get_by_id(lesson.subject_id).await?.name;
                    subject_names.insert(lesson.subject_id, name.clone());
                    name
                }
            };
            for sheet in sheets {
                let tasks = self.plusnik.get_tasks(sheet.id).await?;
                progress.push(SheetProgress {
                    total_tasks: tasks.len(),
                    solved_task_ids: tasks
                        .iter()
                        .filter(|task| solved.contains(&task.id))
                        .map(|task| task.id)
                        .collect(),
                    subject_name: subject_name.clone(),
                    sheet,
                });
            }
        }
        progress.sort_by_key(|item| std::cmp::Reverse(item.sheet.issue_date));
        Ok(progress)
    }

    // ========================================================================
    // Helpers
    // ========================================================================

    /// Lessons of all active classes and all groups (the repository has no "get all";
    /// every lesson belongs to exactly one class or group). Deduplicated by id.
    async fn all_lessons(&self) -> Result<Vec<Lesson>, DomainError> {
        let mut seen = HashSet::new();
        let mut lessons = Vec::new();
        for class in self.classes.get_all_active().await? {
            lessons.extend(self.lessons.get_by_class(class.id).await?);
        }
        for group in self.groups.get_all().await? {
            lessons.extend(self.lessons.get_by_group(group.id).await?);
        }
        lessons.retain(|lesson| seen.insert(lesson.id));
        Ok(lessons)
    }

    /// Admin, or a teacher assigned to the lesson.
    async fn can_manage(&self, user: &User, lesson_id: Uuid) -> Result<bool, DomainError> {
        match user.role {
            UserRole::Admin => Ok(true),
            UserRole::Teacher => Ok(self.lessons.get_teacher_ids(lesson_id).await?.contains(&user.id)),
            UserRole::Student => Ok(false),
        }
    }

    async fn require_manager(&self, user: &User, lesson_id: Uuid) -> Result<(), DomainError> {
        if self.can_manage(user, lesson_id).await? {
            Ok(())
        } else {
            Err(DomainError::InsufficientPermissions)
        }
    }

    /// Is the student in the lesson's class or group?
    async fn is_in_audience(&self, student_id: Uuid, lesson: &Lesson) -> Result<bool, DomainError> {
        match lesson.target {
            LessonTarget::Class(class_id) => match self.users.get_by_id(student_id).await {
                Ok(student) => Ok(student.role.is_student()
                    && student.is_active
                    && student.class_id == Some(class_id)),
                Err(DomainError::UserNotFound) => Ok(false),
                Err(error) => Err(error),
            },
            LessonTarget::Group(group_id) => self.groups.has_member(group_id, student_id).await,
        }
    }

    /// Active students of the lesson's class or group.
    async fn lesson_students(&self, lesson: &Lesson) -> Result<Vec<User>, DomainError> {
        match lesson.target {
            LessonTarget::Class(class_id) => self.users.get_active_students_by_class(class_id).await,
            LessonTarget::Group(group_id) => {
                let mut students = Vec::new();
                for student_id in self.groups.get_member_ids(group_id).await? {
                    match self.users.get_by_id(student_id).await {
                        Ok(user) if user.is_active && user.role.is_student() => students.push(user),
                        Ok(_) | Err(DomainError::UserNotFound) => {}
                        Err(error) => return Err(error),
                    }
                }
                Ok(students)
            }
        }
    }

    async fn active_plus(
        &self,
        sheet_id: Uuid,
        task_id: Uuid,
        student_id: Uuid,
    ) -> Result<Option<PlusnikRecord>, DomainError> {
        Ok(self
            .plusnik
            .get_records_by_sheet_and_student(sheet_id, student_id)
            .await?
            .into_iter()
            .find(|record| record.task_id == task_id && record.is_active()))
    }

    async fn lesson_summary(&self, lesson: &Lesson) -> Result<LessonSummary, DomainError> {
        let subject_name = self.subjects.get_by_id(lesson.subject_id).await?.name;
        let target_name = match lesson.target {
            LessonTarget::Class(class_id) => self.classes.get_by_id(class_id).await?.full_name(),
            LessonTarget::Group(group_id) => self.groups.get_by_id(group_id).await?.name,
        };
        Ok(LessonSummary {
            lesson_id: lesson.id,
            subject_name,
            target_name,
        })
    }
}
