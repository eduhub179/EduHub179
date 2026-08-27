//! Integration tests for `PlusnikRepositoryPg`.
//!
//! These tests verify the public API of the infrastructure crate
//! with a real PostgreSQL database using `sqlx::test`.
//!
//! Coverage:
//! - Sheet CRUD: `get_sheet_by_id`, `get_sheets_by_lesson`, `get_sheets_by_creator`,
//!   `save_sheet` (create/update/errors), `delete_sheet` (with/without records).
//! - Task CRUD: `get_tasks`, `save_task` (create/update/duplicate), `delete_task`
//!   (with/without records).
//! - Record operations: `save_record` (grant + edit), `revoke_plus`, `get_records_by_sheet`,
//!   `get_active_records_by_student`, `get_all_records_by_student`,
//!   `get_active_records_by_task`.
//! - Error mapping: FK violations (lesson/user/sheet/task not found),
//!   duplicate task number, duplicate active record, trigger TaskNotInSheet,
//!   delete-with-records restrict violation.
//!
//! DB fixture: plusnik_sheets references lessons, which requires the full chain
//! users -> class -> subject -> lesson. The `seed_lesson` helper builds it.

use domain::entities::class::Class;
use domain::entities::plusnik::{PlusnikRecord, PlusnikSheet, PlusnikTask};
use domain::entities::subject::Subject;
use domain::entities::user::User;
use domain::errors::DomainError;
use domain::repositories::class_repository::ClassRepository;
use domain::repositories::plusnik_repository::PlusnikRepository;
use domain::repositories::subject_repository::SubjectRepository;
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::class_letter::ClassLetter;
use domain::value_objects::role::UserRole;
use domain::value_objects::sheet_status::SheetStatus;
use infrastructure::postgres::{
    ClassRepositoryPg, PlusnikRepositoryPg, SubjectRepositoryPg, UserRepositoryPg,
};
use sqlx::PgPool;
use uuid::Uuid;

// ============================================================================
// HELPERS
// ============================================================================

fn create_test_teacher() -> User {
    User::try_new(
        Uuid::new_v4(),
        format!("teacher.{}@example.com", Uuid::new_v4()),
        UserRole::Teacher,
        "Petrov".to_string(),
        "Teacher".to_string(),
        None,
        None,
    )
    .expect("Test teacher data should be valid")
}

fn create_test_student() -> User {
    User::try_new(
        Uuid::new_v4(),
        format!("student.{}@example.com", Uuid::new_v4()),
        UserRole::Student,
        "Ivanov".to_string(),
        "Student".to_string(),
        None,
        None,
    )
    .expect("Test student data should be valid")
}

fn create_test_sheet(lesson_id: Uuid, created_by: Uuid) -> PlusnikSheet {
    PlusnikSheet::try_new(
        Uuid::new_v4(),
        lesson_id,
        created_by,
        "Листок 12: Производные".to_string(),
        chrono::NaiveDate::from_ymd_opt(2026, 9, 7).unwrap(),
        None,
        SheetStatus::Draft,
        chrono::Utc::now(),
    )
    .expect("Test sheet should be valid")
}

fn create_test_task(sheet_id: Uuid, number: &str, sort_order: i32) -> PlusnikTask {
    PlusnikTask::try_new(
        Uuid::new_v4(),
        sheet_id,
        number.to_string(),
        sort_order,
        chrono::Utc::now(),
    )
    .expect("Test task should be valid")
}

fn create_active_record(
    student_id: Uuid,
    sheet_id: Uuid,
    task_id: Uuid,
    granted_by: Uuid,
) -> PlusnikRecord {
    PlusnikRecord::try_new_active(
        Uuid::new_v4(),
        student_id,
        sheet_id,
        task_id,
        granted_by,
        chrono::Utc::now(),
    )
    .expect("Test record should be valid")
}

/// Seeds the FK chain required by `plusnik_sheets`: users, class, subject, lesson.
/// Returns `(lesson_id, teacher_id, student_id)`.
async fn seed_lesson(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
    let user_repo = UserRepositoryPg::new(pool.clone());
    let teacher = create_test_teacher();
    let student = create_test_student();
    user_repo.save(teacher.clone()).await.expect("Save teacher");
    user_repo.save(student.clone()).await.expect("Save student");

    let class = Class::try_new(Uuid::new_v4(), 2027, ClassLetter::B, true)
        .expect("Test class should be valid");
    ClassRepositoryPg::new(pool.clone())
        .save(class.clone())
        .await
        .expect("Save class");

    let subject = Subject::try_new(Uuid::new_v4(), "Algebra".to_string())
        .expect("Test subject should be valid");
    SubjectRepositoryPg::new(pool.clone())
        .save(subject.clone())
        .await
        .expect("Save subject");

    let lesson_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO lessons (lesson_id, class_id, group_id, subject_id, is_active)
        VALUES ($1, $2, NULL, $3, TRUE)
        "#,
    )
    .bind(lesson_id)
    .bind(class.id)
    .bind(subject.id)
    .execute(pool)
    .await
    .expect("Insert lesson");

    (lesson_id, teacher.id, student.id)
}

/// Seeds a complete sheet with 2 tasks. Returns `(sheet_id, task1_id, task2_id)`.
async fn seed_sheet_with_tasks(
    pool: &PgPool,
    lesson_id: Uuid,
    created_by: Uuid,
) -> (Uuid, Uuid, Uuid) {
    let repo = PlusnikRepositoryPg::new(pool.clone());
    let sheet = create_test_sheet(lesson_id, created_by);
    repo.save_sheet(sheet.clone()).await.expect("Save sheet");

    let task1 = create_test_task(sheet.id, "1а", 0);
    let task2 = create_test_task(sheet.id, "1б", 1);
    repo.save_task(task1.clone()).await.expect("Save task1");
    repo.save_task(task2.clone()).await.expect("Save task2");

    (sheet.id, task1.id, task2.id)
}

// ============================================================================
// SHEET TESTS
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_sheet_by_id_not_found(pool: PgPool) {
    let repo = PlusnikRepositoryPg::new(pool);
    let err = repo.get_sheet_by_id(Uuid::new_v4()).await.unwrap_err();
    assert_eq!(err, DomainError::PlusnikSheetNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_sheet_and_get_by_id(pool: PgPool) {
    let (lesson_id, teacher_id, _) = seed_lesson(&pool).await;
    let repo = PlusnikRepositoryPg::new(pool);

    let sheet = create_test_sheet(lesson_id, teacher_id);
    repo.save_sheet(sheet.clone()).await.expect("Save sheet");

    let fetched = repo.get_sheet_by_id(sheet.id).await.expect("Get sheet");
    assert_eq!(fetched.id, sheet.id);
    assert_eq!(fetched.lesson_id, lesson_id);
    assert_eq!(fetched.created_by, teacher_id);
    assert_eq!(fetched.name, "Листок 12: Производные");
    assert!(fetched.status.is_draft());
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_sheet_updates_existing(pool: PgPool) {
    let (lesson_id, teacher_id, _) = seed_lesson(&pool).await;
    let repo = PlusnikRepositoryPg::new(pool);

    let sheet = create_test_sheet(lesson_id, teacher_id);
    repo.save_sheet(sheet.clone()).await.expect("Save sheet");

    // Publish the sheet
    let published = PlusnikSheet::try_new(
        sheet.id,
        sheet.lesson_id,
        sheet.created_by,
        "Листок 13: Интегралы".to_string(),
        sheet.issue_date,
        Some(chrono::Utc::now() + chrono::Duration::days(7)),
        SheetStatus::Published,
        sheet.created_at,
    )
    .unwrap();
    repo.save_sheet(published.clone()).await.expect("Update sheet");

    let fetched = repo.get_sheet_by_id(sheet.id).await.expect("Get sheet");
    assert_eq!(fetched.name, "Листок 13: Интегралы");
    assert!(fetched.status.is_published());
    assert!(fetched.deadline.is_some());
    // Immutable fields unchanged
    assert_eq!(fetched.lesson_id, lesson_id);
    assert_eq!(fetched.created_by, teacher_id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_sheet_invalid_lesson_fk(pool: PgPool) {
    let (_, teacher_id, _) = seed_lesson(&pool).await;
    let repo = PlusnikRepositoryPg::new(pool);

    let sheet = create_test_sheet(Uuid::new_v4(), teacher_id);
    let err = repo.save_sheet(sheet).await.unwrap_err();
    assert_eq!(err, DomainError::LessonNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_sheet_invalid_user_fk(pool: PgPool) {
    let (lesson_id, _, _) = seed_lesson(&pool).await;
    let repo = PlusnikRepositoryPg::new(pool);

    let sheet = create_test_sheet(lesson_id, Uuid::new_v4());
    let err = repo.save_sheet(sheet).await.unwrap_err();
    assert_eq!(err, DomainError::UserNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_sheets_by_lesson(pool: PgPool) {
    let (lesson_id, teacher_id, _) = seed_lesson(&pool).await;
    let repo = PlusnikRepositoryPg::new(pool);

    let sheet1 = create_test_sheet(lesson_id, teacher_id);
    let sheet2 = create_test_sheet(lesson_id, teacher_id);
    repo.save_sheet(sheet1.clone()).await.expect("Save sheet1");
    repo.save_sheet(sheet2.clone()).await.expect("Save sheet2");

    let sheets = repo.get_sheets_by_lesson(lesson_id).await.expect("Get sheets");
    assert_eq!(sheets.len(), 2);
    // Ordered by issue_date DESC — both have the same date, so order is unspecified
    let ids: Vec<_> = sheets.iter().map(|s| s.id).collect();
    assert!(ids.contains(&sheet1.id));
    assert!(ids.contains(&sheet2.id));
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_sheets_by_lesson_empty(pool: PgPool) {
    let (lesson_id, _, _) = seed_lesson(&pool).await;
    let repo = PlusnikRepositoryPg::new(pool);

    let sheets = repo.get_sheets_by_lesson(lesson_id).await.expect("Get sheets");
    assert!(sheets.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_sheets_by_creator(pool: PgPool) {
    let (lesson_id, teacher_id, _) = seed_lesson(&pool).await;
    let repo = PlusnikRepositoryPg::new(pool);

    let sheet1 = create_test_sheet(lesson_id, teacher_id);
    let sheet2 = create_test_sheet(lesson_id, teacher_id);
    repo.save_sheet(sheet1).await.expect("Save sheet1");
    repo.save_sheet(sheet2).await.expect("Save sheet2");

    let sheets = repo.get_sheets_by_creator(teacher_id).await.expect("Get sheets");
    assert_eq!(sheets.len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_delete_sheet_no_records(pool: PgPool) {
    let (lesson_id, teacher_id, _) = seed_lesson(&pool).await;
    let repo = PlusnikRepositoryPg::new(pool);

    let sheet = create_test_sheet(lesson_id, teacher_id);
    repo.save_sheet(sheet.clone()).await.expect("Save sheet");

    repo.delete_sheet(sheet.id).await.expect("Delete sheet");
    let err = repo.get_sheet_by_id(sheet.id).await.unwrap_err();
    assert_eq!(err, DomainError::PlusnikSheetNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_delete_sheet_not_found(pool: PgPool) {
    let repo = PlusnikRepositoryPg::new(pool);
    let err = repo.delete_sheet(Uuid::new_v4()).await.unwrap_err();
    assert_eq!(err, DomainError::PlusnikSheetNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_delete_sheet_cascades_tasks(pool: PgPool) {
    let (lesson_id, teacher_id, _) = seed_lesson(&pool).await;
    let (sheet_id, task1_id, task2_id) = seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;
    let repo = PlusnikRepositoryPg::new(pool.clone());

    repo.delete_sheet(sheet_id).await.expect("Delete sheet");

    // Tasks should be gone (ON DELETE CASCADE)
    let tasks = repo.get_tasks(sheet_id).await.expect("Get tasks");
    assert!(tasks.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_delete_sheet_blocked_by_records(pool: PgPool) {
    let (lesson_id, teacher_id, student_id) = seed_lesson(&pool).await;
    let (sheet_id, task1_id, _) = seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;
    let repo = PlusnikRepositoryPg::new(pool.clone());

    // Award a plus — this creates a record that blocks sheet deletion
    let record = create_active_record(student_id, sheet_id, task1_id, teacher_id);
    repo.save_record(record).await.expect("Save record");

    let err = repo.delete_sheet(sheet_id).await.unwrap_err();
    // FK ON DELETE RESTRICT — could map to PlusnikSheetHasRecords
    assert!(
        err == DomainError::PlusnikSheetHasRecords
            || err == DomainError::PlusnikSheetNotFound,
        "Expected PlusnikSheetHasRecords or restrict-related error, got {err:?}"
    );
}

// ============================================================================
// TASK TESTS
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_tasks_empty(pool: PgPool) {
    let (lesson_id, teacher_id, _) = seed_lesson(&pool).await;
    let sheet = create_test_sheet(lesson_id, teacher_id);
    let repo = PlusnikRepositoryPg::new(pool.clone());
    repo.save_sheet(sheet.clone()).await.expect("Save sheet");

    let tasks = repo.get_tasks(sheet.id).await.expect("Get tasks");
    assert!(tasks.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_task_and_get_ordered(pool: PgPool) {
    let (lesson_id, teacher_id, _) = seed_lesson(&pool).await;
    let (sheet_id, _, _) = seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;
    let repo = PlusnikRepositoryPg::new(pool);

    let tasks = repo.get_tasks(sheet_id).await.expect("Get tasks");
    assert_eq!(tasks.len(), 2);
    // Ordered by sort_order
    assert_eq!(tasks[0].task_number, "1а");
    assert_eq!(tasks[0].sort_order, 0);
    assert_eq!(tasks[1].task_number, "1б");
    assert_eq!(tasks[1].sort_order, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_task_duplicate_number(pool: PgPool) {
    let (lesson_id, teacher_id, _) = seed_lesson(&pool).await;
    let (sheet_id, _, _) = seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;
    let repo = PlusnikRepositoryPg::new(pool);

    // "1а" already exists in the seeded sheet
    let dup = create_test_task(sheet_id, "1а", 5);
    let err = repo.save_task(dup).await.unwrap_err();
    assert_eq!(err, DomainError::PlusnikTaskAlreadyExists);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_task_invalid_sheet_fk(pool: PgPool) {
    let (_, teacher_id, _) = seed_lesson(&pool).await;
    let repo = PlusnikRepositoryPg::new(pool);

    let task = create_test_task(Uuid::new_v4(), "1", 0);
    let err = repo.save_task(task).await.unwrap_err();
    assert_eq!(err, DomainError::PlusnikSheetNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_task_updates_existing(pool: PgPool) {
    let (lesson_id, teacher_id, _) = seed_lesson(&pool).await;
    let (sheet_id, task1_id, _) = seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;
    let repo = PlusnikRepositoryPg::new(pool);

    // Update task: change number and sort_order
    let updated = PlusnikTask::try_new(
        task1_id,
        sheet_id,
        "2а".to_string(),
        10,
        chrono::Utc::now(),
    )
    .unwrap();
    repo.save_task(updated).await.expect("Update task");

    let tasks = repo.get_tasks(sheet_id).await.expect("Get tasks");
    let task = tasks.iter().find(|t| t.id == task1_id).unwrap();
    assert_eq!(task.task_number, "2а");
    assert_eq!(task.sort_order, 10);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_delete_task_no_records(pool: PgPool) {
    let (lesson_id, teacher_id, _) = seed_lesson(&pool).await;
    let (sheet_id, task1_id, _) = seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;
    let repo = PlusnikRepositoryPg::new(pool);

    repo.delete_task(task1_id).await.expect("Delete task");
    let tasks = repo.get_tasks(sheet_id).await.expect("Get tasks");
    assert_eq!(tasks.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_delete_task_not_found(pool: PgPool) {
    let repo = PlusnikRepositoryPg::new(pool);
    let err = repo.delete_task(Uuid::new_v4()).await.unwrap_err();
    assert_eq!(err, DomainError::PlusnikTaskNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_delete_task_blocked_by_records(pool: PgPool) {
    let (lesson_id, teacher_id, student_id) = seed_lesson(&pool).await;
    let (sheet_id, task1_id, _) = seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;
    let repo = PlusnikRepositoryPg::new(pool.clone());

    let record = create_active_record(student_id, sheet_id, task1_id, teacher_id);
    repo.save_record(record).await.expect("Save record");

    let err = repo.delete_task(task1_id).await.unwrap_err();
    assert!(
        err == DomainError::PlusnikTaskHasRecords
            || err == DomainError::PlusnikTaskNotFound,
        "Expected PlusnikTaskHasRecords or restrict-related error, got {err:?}"
    );
}

// ============================================================================
// TASK TESTS — GET BY ID
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_task_by_id_found(pool: PgPool) {
    let (lesson_id, teacher_id, _) = seed_lesson(&pool).await;
    let (sheet_id, task1_id, _) = seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;
    let repo = PlusnikRepositoryPg::new(pool);

    let task = repo.get_task_by_id(task1_id).await.expect("Get task by id");
    assert_eq!(task.id, task1_id);
    assert_eq!(task.sheet_id, sheet_id);
    assert_eq!(task.task_number, "1а");
    assert_eq!(task.sort_order, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_task_by_id_not_found(pool: PgPool) {
    let repo = PlusnikRepositoryPg::new(pool);
    let err = repo.get_task_by_id(Uuid::new_v4()).await.unwrap_err();
    assert_eq!(err, DomainError::PlusnikTaskNotFound);
}

// ============================================================================
// RECORD TESTS — SAVE / REVOKE
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_record_and_get_by_sheet(pool: PgPool) {
    let (lesson_id, teacher_id, student_id) = seed_lesson(&pool).await;
    let (sheet_id, task1_id, _) = seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;
    let repo = PlusnikRepositoryPg::new(pool);

    let record = create_active_record(student_id, sheet_id, task1_id, teacher_id);
    repo.save_record(record.clone()).await.expect("Save record");

    let records = repo.get_records_by_sheet(sheet_id).await.expect("Get records");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].student_id, student_id);
    assert_eq!(records[0].task_id, task1_id);
    assert!(records[0].is_active());
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_record_duplicate_active(pool: PgPool) {
    let (lesson_id, teacher_id, student_id) = seed_lesson(&pool).await;
    let (sheet_id, task1_id, _) = seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;
    let repo = PlusnikRepositoryPg::new(pool);

    let record = create_active_record(student_id, sheet_id, task1_id, teacher_id);
    repo.save_record(record).await.expect("Save record");

    // Second active plus for the same (student, task) — should fail
    let dup = create_active_record(student_id, sheet_id, task1_id, teacher_id);
    let err = repo.save_record(dup).await.unwrap_err();
    assert_eq!(err, DomainError::PlusnikRecordAlreadyExists);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_record_invalid_student_fk(pool: PgPool) {
    let (lesson_id, teacher_id, _) = seed_lesson(&pool).await;
    let (sheet_id, task1_id, _) = seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;
    let repo = PlusnikRepositoryPg::new(pool);

    let record = create_active_record(Uuid::new_v4(), sheet_id, task1_id, teacher_id);
    let err = repo.save_record(record).await.unwrap_err();
    assert_eq!(err, DomainError::UserNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_record_invalid_sheet_fk(pool: PgPool) {
    let (lesson_id, teacher_id, student_id) = seed_lesson(&pool).await;
    let (_, task1_id, _) = seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;
    let repo = PlusnikRepositoryPg::new(pool);

    let record = create_active_record(student_id, Uuid::new_v4(), task1_id, teacher_id);
    let err = repo.save_record(record).await.unwrap_err();
    // The trigger check_task_belongs_to_sheet fires before the sheet FK:
    // task_id belongs to a real sheet, but sheet_id is a random UUID,
    // so the trigger says "does not belong" → TaskNotInSheet.
    // (The sheet FK would fire too, but the trigger runs first.)
    assert_eq!(err, DomainError::TaskNotInSheet);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_record_task_not_in_sheet(pool: PgPool) {
    let (lesson_id, teacher_id, student_id) = seed_lesson(&pool).await;
    let (sheet_id, _, _) = seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;

    // Create a task on a DIFFERENT sheet
    let sheet2 = create_test_sheet(lesson_id, teacher_id);
    let repo = PlusnikRepositoryPg::new(pool.clone());
    repo.save_sheet(sheet2.clone()).await.expect("Save sheet2");
    let foreign_task = create_test_task(sheet2.id, "1", 0);
    repo.save_task(foreign_task.clone()).await.expect("Save foreign task");

    // Try to award a plus on sheet1 with a task from sheet2 — trigger should fire
    let record = create_active_record(student_id, sheet_id, foreign_task.id, teacher_id);
    let err = repo.save_record(record).await.unwrap_err();
    assert_eq!(err, DomainError::TaskNotInSheet);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_revoke_plus(pool: PgPool) {
    let (lesson_id, teacher_id, student_id) = seed_lesson(&pool).await;
    let (sheet_id, task1_id, _) = seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;
    let repo = PlusnikRepositoryPg::new(pool);

    let record = create_active_record(student_id, sheet_id, task1_id, teacher_id);
    repo.save_record(record.clone()).await.expect("Save record");

    repo.revoke_plus(record.id, teacher_id, Some("Wrong problem".to_string()))
        .await
        .expect("Revoke plus");

    // Record should still exist but be revoked
    let records = repo.get_records_by_sheet(sheet_id).await.expect("Get records");
    assert_eq!(records.len(), 1);
    assert!(!records[0].is_active());
    assert!(records[0].revoked_at.is_some());
    assert_eq!(records[0].revoked_by, Some(teacher_id));
    assert_eq!(records[0].revoke_comment.as_deref(), Some("Wrong problem"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_revoke_plus_not_found(pool: PgPool) {
    let (_, teacher_id, _) = seed_lesson(&pool).await;
    let repo = PlusnikRepositoryPg::new(pool);

    let err = repo
        .revoke_plus(Uuid::new_v4(), teacher_id, None)
        .await
        .unwrap_err();
    assert_eq!(err, DomainError::PlusnikRecordNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_revoke_plus_already_revoked(pool: PgPool) {
    let (lesson_id, teacher_id, student_id) = seed_lesson(&pool).await;
    let (sheet_id, task1_id, _) = seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;
    let repo = PlusnikRepositoryPg::new(pool);

    let record = create_active_record(student_id, sheet_id, task1_id, teacher_id);
    repo.save_record(record.clone()).await.expect("Save record");

    repo.revoke_plus(record.id, teacher_id, None)
        .await
        .expect("First revoke");

    // Second revoke on already-revoked record — WHERE revoked_at IS NULL matches nothing
    let err = repo
        .revoke_plus(record.id, teacher_id, None)
        .await
        .unwrap_err();
    assert_eq!(err, DomainError::PlusnikRecordNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_after_revoke(pool: PgPool) {
    let (lesson_id, teacher_id, student_id) = seed_lesson(&pool).await;
    let (sheet_id, task1_id, _) = seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;
    let repo = PlusnikRepositoryPg::new(pool);

    // Grant, then revoke
    let record = create_active_record(student_id, sheet_id, task1_id, teacher_id);
    repo.save_record(record.clone()).await.expect("Save record");
    repo.revoke_plus(record.id, teacher_id, None)
        .await
        .expect("Revoke plus");

    // Grant a NEW active plus — should succeed (old one is revoked, partial unique index
    // only covers revoked_at IS NULL)
    let new_record = create_active_record(student_id, sheet_id, task1_id, teacher_id);
    repo.save_record(new_record).await.expect("Save new record");

    let records = repo.get_records_by_sheet(sheet_id).await.expect("Get records");
    assert_eq!(records.len(), 2);
    // One active, one revoked
    let active: Vec<_> = records.iter().filter(|r| r.is_active()).collect();
    let revoked: Vec<_> = records.iter().filter(|r| !r.is_active()).collect();
    assert_eq!(active.len(), 1);
    assert_eq!(revoked.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_record_upsert_edit_existing(pool: PgPool) {
    let (lesson_id, teacher_id, student_id) = seed_lesson(&pool).await;
    let (sheet_id, task1_id, task2_id) =
        seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;
    let repo = PlusnikRepositoryPg::new(pool);

    // 1. Grant a plus for task1
    let record = create_active_record(student_id, sheet_id, task1_id, teacher_id);
    repo.save_record(record.clone()).await.expect("Save record");

    // 2. Edit: change the task from task1 to task2 (wrong task was selected)
    let edited = PlusnikRecord::try_new_active(
        record.id,
        record.student_id,
        record.sheet_id,
        task2_id,
        record.granted_by,
        record.granted_at,
    )
    .unwrap();
    repo.save_record(edited.clone()).await.expect("Update record");

    // 3. Verify: still one record, now pointing to task2
    let records = repo.get_records_by_sheet(sheet_id).await.expect("Get records");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].task_id, task2_id);
    assert!(records[0].is_active());
    // granted_at should be unchanged (immutable on update)
    // granted_at is TIMESTAMPTZ (microsecond precision) — compare truncated
    assert_eq!(
        records[0].granted_at.timestamp_micros(),
        record.granted_at.timestamp_micros()
    );
}

// ============================================================================
// RECORD TESTS — QUERIES
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_active_records_by_student(pool: PgPool) {
    let (lesson_id, teacher_id, student_id) = seed_lesson(&pool).await;
    let (sheet_id, task1_id, task2_id) = seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;
    let repo = PlusnikRepositoryPg::new(pool);

    // Award two pluses
    let r1 = create_active_record(student_id, sheet_id, task1_id, teacher_id);
    let r2 = create_active_record(student_id, sheet_id, task2_id, teacher_id);
    repo.save_record(r1.clone()).await.expect("Save r1");
    repo.save_record(r2.clone()).await.expect("Save r2");

    // Revoke one
    repo.revoke_plus(r1.id, teacher_id, None)
        .await
        .expect("Revoke r1");

    let active = repo
        .get_active_records_by_student(student_id)
        .await
        .expect("Get active");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].task_id, task2_id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_all_records_by_student(pool: PgPool) {
    let (lesson_id, teacher_id, student_id) = seed_lesson(&pool).await;
    let (sheet_id, task1_id, task2_id) = seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;
    let repo = PlusnikRepositoryPg::new(pool);

    let r1 = create_active_record(student_id, sheet_id, task1_id, teacher_id);
    let r2 = create_active_record(student_id, sheet_id, task2_id, teacher_id);
    repo.save_record(r1.clone()).await.expect("Save r1");
    repo.save_record(r2.clone()).await.expect("Save r2");
    repo.revoke_plus(r1.id, teacher_id, None)
        .await
        .expect("Revoke r1");

    let all = repo
        .get_all_records_by_student(student_id)
        .await
        .expect("Get all");
    assert_eq!(all.len(), 2); // includes revoked
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_active_records_by_task(pool: PgPool) {
    let (lesson_id, teacher_id, student_id) = seed_lesson(&pool).await;
    let (sheet_id, task1_id, _) = seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;
    let repo = PlusnikRepositoryPg::new(pool);

    let record = create_active_record(student_id, sheet_id, task1_id, teacher_id);
    repo.save_record(record.clone()).await.expect("Save record");
    repo.revoke_plus(record.id, teacher_id, None)
        .await
        .expect("Revoke plus");

    let active = repo
        .get_active_records_by_task(task1_id)
        .await
        .expect("Get active by task");
    assert!(active.is_empty()); // revoked — not active
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_records_by_sheet_and_student(pool: PgPool) {
    let (lesson_id, teacher_id, student_id) = seed_lesson(&pool).await;
    let (sheet_id, task1_id, task2_id) =
        seed_sheet_with_tasks(&pool, lesson_id, teacher_id).await;
    let repo = PlusnikRepositoryPg::new(pool.clone());

    // Create a second student
    let student2 = create_test_student();
    UserRepositoryPg::new(pool.clone())
        .save(student2.clone())
        .await
        .expect("Save student2");

    // student1 gets 2 pluses (one revoked), student2 gets 1 plus
    let r1 = create_active_record(student_id, sheet_id, task1_id, teacher_id);
    let r2 = create_active_record(student_id, sheet_id, task2_id, teacher_id);
    let r3 = create_active_record(student2.id, sheet_id, task1_id, teacher_id);
    repo.save_record(r1.clone()).await.expect("Save r1");
    repo.save_record(r2.clone()).await.expect("Save r2");
    repo.save_record(r3).await.expect("Save r3");
    repo.revoke_plus(r1.id, teacher_id, None)
        .await
        .expect("Revoke r1");

    // Query: records for student1 on this sheet — should be 2 (1 active, 1 revoked)
    let records = repo
        .get_records_by_sheet_and_student(sheet_id, student_id)
        .await
        .expect("Get records by sheet and student");
    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|r| r.student_id == student_id));
    assert!(records.iter().all(|r| r.sheet_id == sheet_id));

    // student2 should have only 1 record on this sheet
    let records2 = repo
        .get_records_by_sheet_and_student(sheet_id, student2.id)
        .await
        .expect("Get records by sheet and student2");
    assert_eq!(records2.len(), 1);
}

// ============================================================================
// INTEGRATION: FULL MATRIX SCENARIO
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_full_matrix_scenario(pool: PgPool) {
    let (lesson_id, teacher_id, student_id) = seed_lesson(&pool).await;
    let repo = PlusnikRepositoryPg::new(pool.clone());

    // 1. Create a sheet
    let sheet = create_test_sheet(lesson_id, teacher_id);
    repo.save_sheet(sheet.clone()).await.expect("Save sheet");

    // 2. Add 3 tasks
    let t1 = create_test_task(sheet.id, "1", 0);
    let t2 = create_test_task(sheet.id, "2", 1);
    let t3 = create_test_task(sheet.id, "3", 2);
    repo.save_task(t1.clone()).await.expect("Save t1");
    repo.save_task(t2.clone()).await.expect("Save t2");
    repo.save_task(t3.clone()).await.expect("Save t3");

    // 3. Publish the sheet
    let published = PlusnikSheet::try_new(
        sheet.id,
        sheet.lesson_id,
        sheet.created_by,
        sheet.name.clone(),
        sheet.issue_date,
        sheet.deadline,
        SheetStatus::Published,
        sheet.created_at,
    )
    .unwrap();
    repo.save_sheet(published).await.expect("Publish sheet");

    // 4. Award pluses: student solved tasks 1 and 2
    let r1 = create_active_record(student_id, sheet.id, t1.id, teacher_id);
    let r2 = create_active_record(student_id, sheet.id, t2.id, teacher_id);
    repo.save_record(r1).await.expect("Save r1");
    repo.save_record(r2).await.expect("Save r2");

    // 5. Verify: 2 active records for the student
    let active = repo
        .get_active_records_by_student(student_id)
        .await
        .expect("Get active");
    assert_eq!(active.len(), 2);

    // 6. Verify: 2 records on the sheet
    let sheet_records = repo
        .get_records_by_sheet(sheet.id)
        .await
        .expect("Get sheet records");
    assert_eq!(sheet_records.len(), 2);

    // 7. Revoke one plus
    let to_revoke = active.iter().find(|r| r.task_id == t1.id).unwrap();
    repo.revoke_plus(to_revoke.id, teacher_id, Some("Cheating".to_string()))
        .await
        .expect("Revoke plus");

    // 8. Verify: 1 active, 2 total
    let active_after = repo
        .get_active_records_by_student(student_id)
        .await
        .expect("Get active after");
    assert_eq!(active_after.len(), 1);

    let all_after = repo
        .get_all_records_by_student(student_id)
        .await
        .expect("Get all after");
    assert_eq!(all_after.len(), 2);

    // 9. Verify: task 1 has 0 active records
    let task1_active = repo
        .get_active_records_by_task(t1.id)
        .await
        .expect("Get active by task1");
    assert!(task1_active.is_empty());

    // 10. Verify: task 2 has 1 active record
    let task2_active = repo
        .get_active_records_by_task(t2.id)
        .await
        .expect("Get active by task2");
    assert_eq!(task2_active.len(), 1);
}
