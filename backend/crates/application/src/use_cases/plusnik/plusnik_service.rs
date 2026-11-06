//! Plusnik use cases: problem sheets, their tasks, and pluses for solved tasks.
//!
//! Teacher workflow (master doc §3, §6.4): lesson → sheet → students × tasks matrix →
//! tap a cell → plus. Student workflow: dashboard "how many tasks are solved".
//!
//! Access rules:
//! - any teacher and any admin manage sheets and pluses of every lesson
//!   (`LessonAccess::can_manage`);
//! - a student sees published sheets of their lessons (their class or their groups)
//!   and the plus table of such a sheet (the whole lesson, read-only);
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
    value_objects::sheet_status::SheetStatus,
};
use uuid::Uuid;

use crate::use_cases::lesson_access::LessonAccess;

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

/// The plus table: tasks × students with active pluses (teacher's screen, read-only
/// for students).
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

/// Tasks of a sheet after an insert, and which of them are new.
#[derive(Debug, Clone, PartialEq)]
pub struct AddedTasks {
    /// All tasks of the sheet in the new order.
    pub tasks: Vec<PlusnikTask>,
    /// Ids of the inserted tasks, in the order given.
    pub new_ids: Vec<Uuid>,
}

/// Result of granting a plus.
#[derive(Debug, Clone, PartialEq)]
pub struct GrantedPlus {
    pub record: PlusnikRecord,
    /// False if the student already had this plus (a repeated tap is not an error).
    pub created: bool,
}

/// Most tasks one batch call may add or delete.
pub const MAX_TASKS_PER_BATCH: usize = 200;
/// Most plus changes (grants + revokes) in one batch call: a class of 40 × 50 tasks.
pub const MAX_PLUSES_PER_BATCH: usize = 2000;

/// A cell of the matrix: which task, which student.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlusCell {
    pub task_id: Uuid,
    pub student_id: Uuid,
}

/// Input for changing many cells of one sheet at once ("whole column", "whole row").
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlusBatchCommand {
    pub grant: Vec<PlusCell>,
    pub revoke: Vec<PlusCell>,
    /// Reason stored on every revoked record.
    pub comment: Option<String>,
}

/// Result of a batch: what changed and the sheet's active pluses afterwards.
#[derive(Debug, Clone, PartialEq)]
pub struct PlusBatchResult {
    /// Pluses actually added (already present ones are skipped).
    pub granted: usize,
    /// Pluses actually removed (absent ones are skipped).
    pub revoked: usize,
    pub pluses: Vec<PlusnikRecord>,
}

/// A column of the course summary.
#[derive(Debug, Clone, PartialEq)]
pub struct SummarySheet {
    pub sheet: PlusnikSheet,
    pub total_tasks: usize,
}

/// A row of the course summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryStudent {
    pub id: Uuid,
    pub name: String,
    /// Solved tasks per sheet, aligned with `CourseSummary::sheets`.
    pub solved: Vec<usize>,
    pub solved_total: usize,
}

/// Students × sheets of a course with solved counts (published and archived sheets).
#[derive(Debug, Clone, PartialEq)]
pub struct CourseSummary {
    pub lesson: LessonSummary,
    /// Oldest first (the order the course went).
    pub sheets: Vec<SummarySheet>,
    /// Sorted by name.
    pub students: Vec<SummaryStudent>,
    /// Tasks of all `sheets`.
    pub total_tasks: usize,
}

/// One record of a sheet's history with names resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct PlusHistoryEntry {
    pub record: PlusnikRecord,
    pub task_number: String,
    pub student_name: String,
    pub granted_by_name: String,
    pub revoked_by_name: Option<String>,
}

/// Filters of the history; `None` = any.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HistoryFilter {
    pub student_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
}

pub struct PlusnikService {
    plusnik: Arc<dyn PlusnikRepository>,
    lessons: Arc<dyn LessonRepository>,
    users: Arc<dyn UserRepository>,
    groups: Arc<dyn StudentGroupRepository>,
    subjects: Arc<dyn SubjectRepository>,
    access: LessonAccess,
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
            access: LessonAccess::new(
                lessons.clone(),
                users.clone(),
                groups.clone(),
                subjects.clone(),
                classes,
            ),
            plusnik,
            lessons,
            users,
            groups,
            subjects,
        }
    }

    // ========================================================================
    // Lessons and sheets
    // ========================================================================

    /// Lessons the user can manage sheets for, sorted by subject and class/group:
    /// every active lesson of the school for teachers and admins alike.
    ///
    /// With `class_id`, only lessons of that class: its whole-class lessons and group
    /// lessons attended by its students (the same rule as the class schedule), so
    /// profile groups show up on the class page too.
    ///
    /// # Fail-safe поведение
    /// `InsufficientPermissions` for students; `ClassNotFound` for an unknown class.
    pub async fn teacher_lessons(
        &self,
        user: &User,
        class_id: Option<Uuid>,
    ) -> Result<Vec<LessonSummary>, DomainError> {
        if user.role.is_student() {
            return Err(DomainError::InsufficientPermissions);
        }
        let candidates = match class_id {
            Some(class_id) => self.class_lessons(class_id).await?,
            None => self.all_lessons().await?,
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
    /// Any teacher or admin sees all statuses, a student of the lesson
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
        if tasks.is_empty() {
            return Ok(SheetWithTasks { sheet, tasks });
        }
        // Tasks go in one transaction; if it fails, the new empty sheet is removed so
        // that a retry does not leave a half-made duplicate behind.
        match self.plusnik.insert_tasks(sheet.id, tasks, None).await {
            Ok(tasks) => Ok(SheetWithTasks { sheet, tasks }),
            Err(error) => {
                let _ = self.plusnik.delete_sheet(sheet.id).await;
                Err(error)
            }
        }
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
        self.insert_task(user, sheet_id, task_number, None).await
    }

    /// Inserts one task at `position` (0-based; `None` or past the end = append).
    /// Tasks after it shift down.
    pub async fn insert_task(
        &self,
        user: &User,
        sheet_id: Uuid,
        task_number: String,
        position: Option<usize>,
    ) -> Result<PlusnikTask, DomainError> {
        let tasks = self.add_tasks(user, sheet_id, vec![task_number], position).await?;
        let created = tasks.new_ids.first().copied().ok_or(DomainError::InternalError)?;
        tasks
            .tasks
            .into_iter()
            .find(|task| task.id == created)
            .ok_or(DomainError::InternalError)
    }

    /// Inserts several tasks, in the given order, at `position` (0-based; `None` or past
    /// the end = append). All or nothing.
    ///
    /// # Fail-safe поведение
    /// `InvalidPlusnikBatch` for an empty list or more than `MAX_TASKS_PER_BATCH`;
    /// `InvalidTaskNumber` / `PlusnikTaskAlreadyExists` (also for a repeat inside the
    /// list) — nothing is saved then.
    pub async fn add_tasks(
        &self,
        user: &User,
        sheet_id: Uuid,
        task_numbers: Vec<String>,
        position: Option<usize>,
    ) -> Result<AddedTasks, DomainError> {
        if task_numbers.is_empty() || task_numbers.len() > MAX_TASKS_PER_BATCH {
            return Err(DomainError::InvalidPlusnikBatch);
        }
        let sheet = self.plusnik.get_sheet_by_id(sheet_id).await?;
        self.require_manager(user, sheet.lesson_id).await?;

        let now = Utc::now();
        let mut seen = HashSet::new();
        let mut new_tasks = Vec::with_capacity(task_numbers.len());
        for number in task_numbers {
            let task = PlusnikTask::try_new(Uuid::new_v4(), sheet_id, number, 0, now)?;
            if !seen.insert(task.task_number.clone()) {
                return Err(DomainError::PlusnikTaskAlreadyExists);
            }
            new_tasks.push(task);
        }
        let new_ids = new_tasks.iter().map(|task| task.id).collect();
        let tasks = self.plusnik.insert_tasks(sheet_id, new_tasks, position).await?;
        Ok(AddedTasks { tasks, new_ids })
    }

    /// Changes the number of a task ("1а" → "1а*"). Pluses stay with the task.
    ///
    /// # Fail-safe поведение
    /// `InvalidTaskNumber`; `PlusnikTaskAlreadyExists` if another task has this number.
    pub async fn rename_task(
        &self,
        user: &User,
        task_id: Uuid,
        task_number: String,
    ) -> Result<PlusnikTask, DomainError> {
        let task = self.plusnik.get_task_by_id(task_id).await?;
        let sheet = self.plusnik.get_sheet_by_id(task.sheet_id).await?;
        self.require_manager(user, sheet.lesson_id).await?;

        let renamed = PlusnikTask::try_new(task.id, task.sheet_id, task_number, task.sort_order, task.created_at)?;
        if renamed.task_number == task.task_number {
            return Ok(task);
        }
        self.plusnik.save_task(renamed).await
    }

    /// Sets the order of all tasks of a sheet.
    ///
    /// # Fail-safe поведение
    /// `InvalidTaskOrder` unless `task_ids` lists every task of the sheet exactly once.
    pub async fn reorder_tasks(
        &self,
        user: &User,
        sheet_id: Uuid,
        task_ids: Vec<Uuid>,
    ) -> Result<Vec<PlusnikTask>, DomainError> {
        let sheet = self.plusnik.get_sheet_by_id(sheet_id).await?;
        self.require_manager(user, sheet.lesson_id).await?;
        self.plusnik.reorder_tasks(sheet_id, &task_ids).await
    }

    /// Deletes several tasks of a sheet, all or nothing. Returns the remaining tasks.
    ///
    /// # Fail-safe поведение
    /// `InvalidPlusnikBatch` for an empty or too large list; `PlusnikTaskNotFound` if any
    /// id is not a task of this sheet; `PlusnikTaskHasRecords` if any has pluses.
    pub async fn delete_tasks(
        &self,
        user: &User,
        sheet_id: Uuid,
        task_ids: Vec<Uuid>,
    ) -> Result<Vec<PlusnikTask>, DomainError> {
        if task_ids.is_empty() || task_ids.len() > MAX_TASKS_PER_BATCH {
            return Err(DomainError::InvalidPlusnikBatch);
        }
        let sheet = self.plusnik.get_sheet_by_id(sheet_id).await?;
        self.require_manager(user, sheet.lesson_id).await?;
        self.plusnik.delete_tasks(sheet_id, &task_ids).await?;
        self.plusnik.get_tasks(sheet_id).await
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

    /// The plus table: tasks × students of the lesson with active pluses.
    /// Any teacher or admin sees it for any status, a student of the lesson
    /// only for a published sheet (the same sheets `lesson_sheets` shows them).
    ///
    /// # Fail-safe поведение
    /// `PlusnikSheetNotFound` for a student and a draft/archived sheet (drafts are not
    /// revealed); `InsufficientPermissions` for anyone outside the lesson.
    pub async fn sheet_matrix(&self, user: &User, sheet_id: Uuid) -> Result<SheetMatrix, DomainError> {
        let sheet = self.plusnik.get_sheet_by_id(sheet_id).await?;
        let lesson = self.lessons.get_by_id(sheet.lesson_id).await?;
        if !self.can_manage(user, lesson.id).await? {
            if !user.role.is_student() || !self.is_in_audience(user.id, &lesson).await? {
                return Err(DomainError::InsufficientPermissions);
            }
            if sheet.status != SheetStatus::Published {
                return Err(DomainError::PlusnikSheetNotFound);
            }
        }

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

    /// Grants and revokes many pluses of one sheet in one transaction: a whole column
    /// ("everyone solved 3а"), a whole row, or any set of cells.
    ///
    /// # Fail-safe поведение
    /// Everything is validated before anything changes:
    /// `InvalidPlusnikBatch` — empty, more than `MAX_PLUSES_PER_BATCH`, or a cell both
    /// granted and revoked; `TaskNotInSheet` — a task of another sheet;
    /// `PlusnikSheetNotPublished` — grants on a draft/archived sheet (revokes work on any
    /// status, like `revoke_plus`); `StudentNotInLesson` — a grant to an outsider.
    /// Cells already in the wanted state are skipped, so a repeated call is safe.
    pub async fn apply_pluses(
        &self,
        user: &User,
        sheet_id: Uuid,
        cmd: PlusBatchCommand,
    ) -> Result<PlusBatchResult, DomainError> {
        let changes = cmd.grant.len().saturating_add(cmd.revoke.len());
        if changes == 0 || changes > MAX_PLUSES_PER_BATCH {
            return Err(DomainError::InvalidPlusnikBatch);
        }
        let grant: HashSet<PlusCell> = cmd.grant.into_iter().collect();
        let revoke: HashSet<PlusCell> = cmd.revoke.into_iter().collect();
        if !grant.is_disjoint(&revoke) {
            return Err(DomainError::InvalidPlusnikBatch);
        }

        let sheet = self.plusnik.get_sheet_by_id(sheet_id).await?;
        self.require_manager(user, sheet.lesson_id).await?;
        if !grant.is_empty() && sheet.status != SheetStatus::Published {
            return Err(DomainError::PlusnikSheetNotPublished);
        }

        let task_ids: HashSet<Uuid> = self
            .plusnik
            .get_tasks(sheet_id)
            .await?
            .into_iter()
            .map(|task| task.id)
            .collect();
        if grant.iter().chain(revoke.iter()).any(|cell| !task_ids.contains(&cell.task_id)) {
            return Err(DomainError::TaskNotInSheet);
        }

        if !grant.is_empty() {
            let lesson = self.lessons.get_by_id(sheet.lesson_id).await?;
            let audience: HashSet<Uuid> = self
                .lesson_students(&lesson)
                .await?
                .into_iter()
                .map(|student| student.id)
                .collect();
            if grant.iter().any(|cell| !audience.contains(&cell.student_id)) {
                return Err(DomainError::StudentNotInLesson);
            }
        }

        let now = Utc::now();
        let records = grant
            .iter()
            .map(|cell| {
                PlusnikRecord::try_new_active(Uuid::new_v4(), cell.student_id, sheet_id, cell.task_id, user.id, now)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let revoke_pairs: Vec<(Uuid, Uuid)> =
            revoke.iter().map(|cell| (cell.task_id, cell.student_id)).collect();
        let comment = cmd
            .comment
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty());

        let (granted, revoked) = self
            .plusnik
            .apply_pluses(sheet_id, records, &revoke_pairs, user.id, comment)
            .await?;
        let pluses = self
            .plusnik
            .get_records_by_sheet(sheet_id)
            .await?
            .into_iter()
            .filter(PlusnikRecord::is_active)
            .collect();
        Ok(PlusBatchResult { granted, revoked, pluses })
    }

    /// The course summary for a teacher: students × sheets with solved counts, over
    /// published and archived sheets (drafts have no pluses). Only active pluses of
    /// current students of the course count.
    ///
    /// # Fail-safe поведение
    /// `InsufficientPermissions` unless the user manages the course.
    pub async fn course_summary(&self, user: &User, lesson_id: Uuid) -> Result<CourseSummary, DomainError> {
        self.require_manager(user, lesson_id).await?;
        let lesson = self.lessons.get_by_id(lesson_id).await?;

        let mut sheets: Vec<PlusnikSheet> = self
            .plusnik
            .get_sheets_by_lesson(lesson_id)
            .await?
            .into_iter()
            .filter(|sheet| sheet.status != SheetStatus::Draft)
            .collect();
        sheets.sort_by(|a, b| (a.issue_date, a.created_at).cmp(&(b.issue_date, b.created_at)));
        let column: HashMap<Uuid, usize> =
            sheets.iter().enumerate().map(|(index, sheet)| (sheet.id, index)).collect();

        let mut task_counts = vec![0usize; sheets.len()];
        for task in self.plusnik.get_tasks_by_lesson(lesson_id).await? {
            if let Some(&index) = column.get(&task.sheet_id) {
                task_counts[index] += 1;
            }
        }

        let mut students: Vec<SummaryStudent> = self
            .lesson_students(&lesson)
            .await?
            .into_iter()
            .map(|student| SummaryStudent {
                id: student.id,
                name: format!("{} {}", student.last_name, student.first_name),
                solved: vec![0; sheets.len()],
                solved_total: 0,
            })
            .collect();
        students.sort_by(|a, b| a.name.cmp(&b.name));
        let row: HashMap<Uuid, usize> =
            students.iter().enumerate().map(|(index, student)| (student.id, index)).collect();

        for record in self.plusnik.get_active_records_by_lesson(lesson_id).await? {
            if let (Some(&r), Some(&c)) = (row.get(&record.student_id), column.get(&record.sheet_id)) {
                students[r].solved[c] += 1;
                students[r].solved_total += 1;
            }
        }

        Ok(CourseSummary {
            lesson: self.lesson_summary(&lesson).await?,
            total_tasks: task_counts.iter().sum(),
            sheets: sheets
                .into_iter()
                .zip(task_counts)
                .map(|(sheet, total_tasks)| SummarySheet { sheet, total_tasks })
                .collect(),
            students,
        })
    }

    /// Every plus of a sheet ever put, including revoked ones, newest first, with who
    /// put and removed it — settles "who put / removed this plus" disputes.
    ///
    /// # Fail-safe поведение
    /// `InsufficientPermissions` unless the user manages the course. A user deleted
    /// from the directory shows as "—" instead of failing the whole history.
    pub async fn sheet_history(
        &self,
        user: &User,
        sheet_id: Uuid,
        filter: HistoryFilter,
    ) -> Result<Vec<PlusHistoryEntry>, DomainError> {
        let sheet = self.plusnik.get_sheet_by_id(sheet_id).await?;
        self.require_manager(user, sheet.lesson_id).await?;

        let numbers: HashMap<Uuid, String> = self
            .plusnik
            .get_tasks(sheet_id)
            .await?
            .into_iter()
            .map(|task| (task.id, task.task_number))
            .collect();
        let records: Vec<PlusnikRecord> = self
            .plusnik
            .get_records_by_sheet(sheet_id)
            .await?
            .into_iter()
            .filter(|record| filter.student_id.map_or(true, |id| record.student_id == id))
            .filter(|record| filter.task_id.map_or(true, |id| record.task_id == id))
            .collect();

        // Collected before the loop: an iterator held across `.await` makes the future !Send.
        let mut people: Vec<Uuid> = Vec::new();
        for record in &records {
            people.push(record.student_id);
            people.push(record.granted_by);
            people.extend(record.revoked_by);
        }
        let mut names: HashMap<Uuid, String> = HashMap::new();
        for id in people {
            if names.contains_key(&id) {
                continue;
            }
            let name = match self.users.get_by_id(id).await {
                Ok(found) => format!("{} {}", found.last_name, found.first_name),
                Err(DomainError::UserNotFound) => "—".to_string(),
                Err(error) => return Err(error),
            };
            names.insert(id, name);
        }
        let name_of = |id: Uuid| names.get(&id).cloned().unwrap_or_else(|| "—".to_string());

        Ok(records
            .into_iter()
            .map(|record| PlusHistoryEntry {
                task_number: numbers.get(&record.task_id).cloned().unwrap_or_default(),
                student_name: name_of(record.student_id),
                granted_by_name: name_of(record.granted_by),
                revoked_by_name: record.revoked_by.map(name_of),
                record,
            })
            .collect())
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

    // Access rules are shared with other course features: see `LessonAccess`.

    async fn all_lessons(&self) -> Result<Vec<Lesson>, DomainError> {
        self.access.all_lessons().await
    }

    async fn class_lessons(&self, class_id: Uuid) -> Result<Vec<Lesson>, DomainError> {
        self.access.class_lessons(class_id).await
    }

    async fn can_manage(&self, user: &User, lesson_id: Uuid) -> Result<bool, DomainError> {
        self.access.can_manage(user, lesson_id).await
    }

    async fn require_manager(&self, user: &User, lesson_id: Uuid) -> Result<(), DomainError> {
        self.access.require_manager(user, lesson_id).await
    }

    async fn is_in_audience(&self, student_id: Uuid, lesson: &Lesson) -> Result<bool, DomainError> {
        self.access.is_in_audience(student_id, lesson).await
    }

    async fn lesson_students(&self, lesson: &Lesson) -> Result<Vec<User>, DomainError> {
        self.access.lesson_students(lesson).await
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
        let names = self.access.names(lesson).await?;
        Ok(LessonSummary {
            lesson_id: lesson.id,
            subject_name: names.subject_name,
            target_name: names.target_name,
        })
    }
}
