//! Integration tests for `HomeworkRepositoryPg`.
//!
//! These tests verify the public API of the infrastructure crate
//! with a real PostgreSQL database using `sqlx::test` for automatic
//! transaction management and rollback.
//!
//! Coverage:
//! - Homework CRUD: `get_by_id`, `get_by_lesson_instance`, `save` (create/update/errors).
//! - Homework files: `add_file`, `remove_file`, `get_files` (sorting, empty cases).
//! - `delete` (including FK cascade to `homework_files`).
//! - `create_with_files` (single-transaction create, duplicate-lesson rollback).
//! - Domain invariant validation (pure unit tests, no DB).
//!
//! DB fixture note: `homeworks` references `lesson_instances`, which requires
//! the full chain lessons -> lesson_templates -> schedule_slots -> lesson_instances.
//! The `seed_lesson_instance` helper builds it via raw SQL (no repository exists
//! yet for the schedule layer).
use domain::entities::class::Class;
use domain::entities::homework::{Homework, HomeworkFile};
use domain::entities::subject::Subject;
use domain::entities::user::User;
use domain::errors::DomainError;
use domain::repositories::class_repository::ClassRepository;
use domain::repositories::homework_repository::HomeworkRepository;
use domain::repositories::subject_repository::SubjectRepository;
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::class_letter::ClassLetter;
use domain::value_objects::homework_status::HomeworkStatus;
use domain::value_objects::role::UserRole;
use infrastructure::postgres::{
    ClassRepositoryPg, HomeworkRepositoryPg, SubjectRepositoryPg, UserRepositoryPg,
};
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

// ============================================================================
// HELPERS
// ============================================================================

/// Helper: creates a test homework with a random ID.
/// Panics if the domain invariants are violated (valid test data should pass).
fn create_test_homework(lesson_instance_id: Uuid, created_by: Uuid, role: UserRole) -> Homework {
    Homework::try_new(
        Uuid::new_v4(),
        lesson_instance_id,
        created_by,
        role,
        Some("Do exercises 1-5 from the textbook".to_string()),
        HomeworkStatus::Draft,
        false,
        None,
    )
    .expect("Test homework should be valid and satisfy domain invariants")
}

/// Helper: creates a test homework file with a random ID.
fn create_test_file(homework_id: Uuid, sort_order: i32) -> HomeworkFile {
    HomeworkFile::try_new(
        Uuid::new_v4(),
        homework_id,
        format!("homeworks/2026/07/{}.pdf", Uuid::new_v4()),
        "homework.pdf".to_string(),
        "application/pdf".to_string(),
        1024,
        sort_order,
    )
    .expect("Test file should be valid and satisfy domain invariants")
}

/// Helper: creates a test teacher with a random ID and unique email.
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

/// Helper: creates a test student with a random ID and unique email.
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

/// Seeds the full FK chain required by `homeworks`:
/// users (teacher + student), class, subject, lesson, lesson_template,
/// schedule_slot and lesson_instance.
///
/// Returns `(lesson_instance_id, teacher_id, student_id)`.
///
/// The schedule layer has no repository yet, so the chain is inserted with raw
/// SQL. Each test runs in its own isolated database (`sqlx::test`), so fixed
/// names/dates do not collide across tests.
async fn seed_lesson_instance(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
    // 1. Users (via the existing UserRepository)
    let user_repo = UserRepositoryPg::new(pool.clone());
    let teacher = create_test_teacher();
    let student = create_test_student();
    user_repo.save(teacher.clone()).await.expect("Save teacher should succeed");
    user_repo.save(student.clone()).await.expect("Save student should succeed");

    // 2. Class + subject (via the existing repositories)
    let class = Class::try_new(Uuid::new_v4(), 2027, ClassLetter::B, true)
        .expect("Test class should be valid");
    ClassRepositoryPg::new(pool.clone())
        .save(class.clone())
        .await
        .expect("Save class should succeed");

    let subject = Subject::try_new(Uuid::new_v4(), "Algebra".to_string())
        .expect("Test subject should be valid");
    SubjectRepositoryPg::new(pool.clone())
        .save(subject.clone())
        .await
        .expect("Save subject should succeed");

    // 3. Lesson (class-based: class_id set, group_id NULL — satisfies chk_one_entity)
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
    .expect("Insert lesson should succeed");

    // 4. Lesson template (Monday 10:00-10:45, every week)
    let template_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO lesson_templates (template_id, lesson_id, day, start_time, end_time, parity)
        VALUES ($1, $2, $3::day_of_week, $4::TIME, $5::TIME, $6::week_parity)
        "#,
    )
    .bind(template_id)
    .bind(lesson_id)
    .bind("пн")
    .bind("10:00")
    .bind("10:45")
    .bind("every")
    .execute(pool)
    .await
    .expect("Insert lesson template should succeed");

    // 5. Schedule slot for the week of 2026-09-07
    let slot_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO schedule_slots (slot_id, template_id, week_start_date)
        VALUES ($1, $2, $3::DATE)
        "#,
    )
    .bind(slot_id)
    .bind(template_id)
    .bind("2026-09-07")
    .execute(pool)
    .await
    .expect("Insert schedule slot should succeed");

    // 6. Concrete lesson instance on 2026-09-07
    let instance_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO lesson_instances (instance_id, slot_id, lesson_date)
        VALUES ($1, $2, $3::DATE)
        "#,
    )
    .bind(instance_id)
    .bind(slot_id)
    .bind("2026-09-07")
    .execute(pool)
    .await
    .expect("Insert lesson instance should succeed");

    (instance_id, teacher.id, student.id)
}

// ============================================================================
// TESTS FOR get_by_id
// ============================================================================

/// Test: get_by_id returns HomeworkNotFound for a non-existent ID.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_not_found(pool: PgPool) {
    let repo = HomeworkRepositoryPg::new(pool);
    let fake_id = Uuid::new_v4();

    let result = repo.get_by_id(fake_id).await;

    assert!(matches!(result, Err(DomainError::HomeworkNotFound)));
}

/// Test: get_by_id returns the correct homework with all fields intact.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_success(pool: PgPool) {
    let (instance_id, teacher_id, _) = seed_lesson_instance(&pool).await;
    let repo = HomeworkRepositoryPg::new(pool);

    let homework = create_test_homework(instance_id, teacher_id, UserRole::Teacher);
    repo.save(homework.clone()).await.expect("Save should succeed");

    let fetched = repo.get_by_id(homework.id).await.expect("Get by ID should succeed");

    assert_eq!(fetched.id, homework.id);
    assert_eq!(fetched.lesson_instance_id, instance_id);
    assert_eq!(fetched.created_by, teacher_id);
    assert_eq!(fetched.created_by_role, UserRole::Teacher);
    assert_eq!(fetched.text_content, homework.text_content);
    assert_eq!(fetched.status, HomeworkStatus::Draft);
    assert!(!fetched.locked_by_teacher);
    assert_eq!(fetched.last_edited_by, None);
}

// ============================================================================
// TESTS FOR get_by_lesson_instance
// ============================================================================

/// Test: get_by_lesson_instance returns HomeworkNotFound when no homework exists.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_lesson_instance_not_found(pool: PgPool) {
    let (instance_id, _, _) = seed_lesson_instance(&pool).await;
    let repo = HomeworkRepositoryPg::new(pool);

    let result = repo.get_by_lesson_instance(instance_id).await;

    assert!(matches!(result, Err(DomainError::HomeworkNotFound)));
}

/// Test: get_by_lesson_instance returns the homework of the given lesson.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_lesson_instance_success(pool: PgPool) {
    let (instance_id, teacher_id, _) = seed_lesson_instance(&pool).await;
    let repo = HomeworkRepositoryPg::new(pool);

    let homework = create_test_homework(instance_id, teacher_id, UserRole::Teacher);
    repo.save(homework.clone()).await.expect("Save should succeed");

    let fetched = repo
        .get_by_lesson_instance(instance_id)
        .await
        .expect("Get by lesson instance should succeed");

    assert_eq!(fetched.id, homework.id);
}

// ============================================================================
// TESTS FOR save (CREATE)
// ============================================================================

/// Test: save creates a homework created by a teacher.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_creates_homework_by_teacher(pool: PgPool) {
    let (instance_id, teacher_id, _) = seed_lesson_instance(&pool).await;
    let repo = HomeworkRepositoryPg::new(pool);

    let homework = create_test_homework(instance_id, teacher_id, UserRole::Teacher);

    let result = repo.save(homework.clone()).await;

    assert_eq!(result.unwrap(), homework);
    let fetched = repo.get_by_id(homework.id).await.unwrap();
    assert_eq!(fetched.created_by_role, UserRole::Teacher);
}

/// Test: save creates a homework created by a student (duty officer).
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_creates_homework_by_student(pool: PgPool) {
    let (instance_id, _, student_id) = seed_lesson_instance(&pool).await;
    let repo = HomeworkRepositoryPg::new(pool);

    let homework = create_test_homework(instance_id, student_id, UserRole::Student);

    let result = repo.save(homework.clone()).await;

    assert!(result.is_ok());
    let fetched = repo.get_by_id(homework.id).await.unwrap();
    assert_eq!(fetched.created_by, student_id);
    assert_eq!(fetched.created_by_role, UserRole::Student);
}

/// Test: save round-trips published status, teacher lock and last_edited_by.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_roundtrips_status_lock_and_editor(pool: PgPool) {
    let (instance_id, teacher_id, student_id) = seed_lesson_instance(&pool).await;
    let repo = HomeworkRepositoryPg::new(pool);

    let homework = Homework::try_new(
        Uuid::new_v4(),
        instance_id,
        teacher_id,
        UserRole::Teacher,
        Some("Read chapter 3".to_string()),
        HomeworkStatus::Published,
        true,
        Some(student_id), // the student edited last; visible only to admins (audit)
    )
    .expect("Valid homework");

    repo.save(homework.clone()).await.expect("Save should succeed");

    let fetched = repo.get_by_id(homework.id).await.unwrap();
    assert_eq!(fetched.status, HomeworkStatus::Published);
    assert!(fetched.locked_by_teacher);
    assert_eq!(fetched.last_edited_by, Some(student_id));
}

// ============================================================================
// TESTS FOR save (UPDATE / UPSERT)
// ============================================================================

/// Test: save updates an existing homework (upsert) when the ID matches.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_updates_existing_homework(pool: PgPool) {
    let (instance_id, teacher_id, _) = seed_lesson_instance(&pool).await;
    let repo = HomeworkRepositoryPg::new(pool);

    let original = create_test_homework(instance_id, teacher_id, UserRole::Teacher);
    repo.save(original.clone()).await.unwrap();

    // Same ID, new content/status/lock
    let updated = Homework::try_new(
        original.id,
        instance_id,
        teacher_id,
        UserRole::Teacher,
        Some("Updated: solve problems 6-10".to_string()),
        HomeworkStatus::Published,
        true,
        Some(teacher_id),
    )
    .expect("Valid updated homework");

    let result = repo.save(updated.clone()).await;

    assert_eq!(result.unwrap(), updated);
    let fetched = repo.get_by_id(original.id).await.unwrap();
    assert_eq!(fetched.text_content, Some("Updated: solve problems 6-10".to_string()));
    assert_eq!(fetched.status, HomeworkStatus::Published);
    assert!(fetched.locked_by_teacher);
}

/// Test: save raises HomeworkAlreadyExists for a duplicate lesson instance.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_duplicate_lesson_instance_raises_error(pool: PgPool) {
    let (instance_id, teacher_id, _) = seed_lesson_instance(&pool).await;
    let repo = HomeworkRepositoryPg::new(pool);

    let homework_1 = create_test_homework(instance_id, teacher_id, UserRole::Teacher);
    repo.save(homework_1).await.unwrap();

    // Different homework_id, SAME lesson_instance_id
    let homework_2 = create_test_homework(instance_id, teacher_id, UserRole::Teacher);
    let result = repo.save(homework_2).await;

    assert!(matches!(result, Err(DomainError::HomeworkAlreadyExists)));
}

// ============================================================================
// TESTS FOR get_files
// ============================================================================

/// Test: get_files returns an empty vec when no files exist.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_files_empty(pool: PgPool) {
    let (instance_id, teacher_id, _) = seed_lesson_instance(&pool).await;
    let repo = HomeworkRepositoryPg::new(pool);

    let homework = create_test_homework(instance_id, teacher_id, UserRole::Teacher);
    repo.save(homework.clone()).await.unwrap();

    let result = repo.get_files(homework.id).await.unwrap();

    assert!(result.is_empty());
}

/// Test: get_files returns an empty vec for a non-existent homework.
///
/// Consistent with the list-method precedent (`get_member_ids`):
/// list methods return empty rather than error for a missing parent.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_files_non_existent_homework(pool: PgPool) {
    let repo = HomeworkRepositoryPg::new(pool);
    let fake_id = Uuid::new_v4();

    let result = repo.get_files(fake_id).await.unwrap();

    assert!(result.is_empty());
}

/// Test: get_files returns files sorted by sort_order.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_files_sorted_by_sort_order(pool: PgPool) {
    let (instance_id, teacher_id, _) = seed_lesson_instance(&pool).await;
    let repo = HomeworkRepositoryPg::new(pool);

    let homework = create_test_homework(instance_id, teacher_id, UserRole::Teacher);
    repo.save(homework.clone()).await.unwrap();

    // Insert in non-sorted order
    let file_a = create_test_file(homework.id, 5);
    let file_b = create_test_file(homework.id, 1);
    repo.add_file(file_a.clone()).await.unwrap();
    repo.add_file(file_b.clone()).await.unwrap();

    let files = repo.get_files(homework.id).await.unwrap();

    assert_eq!(files.len(), 2);
    assert_eq!(files[0].id, file_b.id); // sort_order 1 first
    assert_eq!(files[1].id, file_a.id); // sort_order 5 second
}

// ============================================================================
// TESTS FOR add_file
// ============================================================================

/// Test: add_file attaches a file to a homework.
#[sqlx::test(migrations = "../../migrations")]
async fn test_add_file_success(pool: PgPool) {
    let (instance_id, teacher_id, _) = seed_lesson_instance(&pool).await;
    let repo = HomeworkRepositoryPg::new(pool);

    let homework = create_test_homework(instance_id, teacher_id, UserRole::Teacher);
    repo.save(homework.clone()).await.unwrap();

    let file = create_test_file(homework.id, 0);
    let result = repo.add_file(file.clone()).await;

    assert_eq!(result.unwrap(), file);
    let files = repo.get_files(homework.id).await.unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].storage_key, file.storage_key);
    assert_eq!(files[0].file_name, "homework.pdf");
    assert_eq!(files[0].mime_type, "application/pdf");
    assert_eq!(files[0].size_bytes, 1024);
}

/// Test: add_file to a non-existent homework returns HomeworkNotFound (FK violation).
#[sqlx::test(migrations = "../../migrations")]
async fn test_add_file_non_existent_homework(pool: PgPool) {
    let repo = HomeworkRepositoryPg::new(pool);
    let fake_homework_id = Uuid::new_v4();

    let file = create_test_file(fake_homework_id, 0);
    let result = repo.add_file(file).await;

    assert!(matches!(result, Err(DomainError::HomeworkNotFound)));
}

// ============================================================================
// TESTS FOR remove_file
// ============================================================================

/// Test: remove_file deletes a file; other files remain.
#[sqlx::test(migrations = "../../migrations")]
async fn test_remove_file_success(pool: PgPool) {
    let (instance_id, teacher_id, _) = seed_lesson_instance(&pool).await;
    let repo = HomeworkRepositoryPg::new(pool);

    let homework = create_test_homework(instance_id, teacher_id, UserRole::Teacher);
    repo.save(homework.clone()).await.unwrap();

    let file_1 = create_test_file(homework.id, 0);
    let file_2 = create_test_file(homework.id, 1);
    repo.add_file(file_1.clone()).await.unwrap();
    repo.add_file(file_2.clone()).await.unwrap();

    let result = repo.remove_file(file_1.id).await;

    assert!(result.is_ok());
    let files = repo.get_files(homework.id).await.unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].id, file_2.id);
}

/// Test: remove_file of a non-existent file returns HomeworkFileNotFound.
#[sqlx::test(migrations = "../../migrations")]
async fn test_remove_file_not_found(pool: PgPool) {
    let repo = HomeworkRepositoryPg::new(pool);
    let fake_file_id = Uuid::new_v4();

    let result = repo.remove_file(fake_file_id).await;

    assert!(matches!(result, Err(DomainError::HomeworkFileNotFound)));
}

// ============================================================================
// TESTS FOR delete
// ============================================================================

/// Test: delete removes the homework and cascades to its files.
#[sqlx::test(migrations = "../../migrations")]
async fn test_delete_cascades_files(pool: PgPool) {
    let (instance_id, teacher_id, _) = seed_lesson_instance(&pool).await;
    let repo = HomeworkRepositoryPg::new(pool);

    let homework = create_test_homework(instance_id, teacher_id, UserRole::Teacher);
    repo.save(homework.clone()).await.unwrap();
    repo.add_file(create_test_file(homework.id, 0)).await.unwrap();
    repo.add_file(create_test_file(homework.id, 1)).await.unwrap();

    let result = repo.delete(homework.id).await;

    assert!(result.is_ok());
    assert!(matches!(repo.get_by_id(homework.id).await, Err(DomainError::HomeworkNotFound)));
    // Files must be gone via ON DELETE CASCADE
    assert!(repo.get_files(homework.id).await.unwrap().is_empty());
}

/// Test: delete of a non-existent homework returns HomeworkNotFound.
#[sqlx::test(migrations = "../../migrations")]
async fn test_delete_not_found(pool: PgPool) {
    let repo = HomeworkRepositoryPg::new(pool);
    let fake_id = Uuid::new_v4();

    let result = repo.delete(fake_id).await;

    assert!(matches!(result, Err(DomainError::HomeworkNotFound)));
}

// ============================================================================
// TESTS FOR create_with_files
// ============================================================================

/// Test: create_with_files persists the homework and all files in one call.
#[sqlx::test(migrations = "../../migrations")]
async fn test_create_with_files_success(pool: PgPool) {
    let (instance_id, teacher_id, _) = seed_lesson_instance(&pool).await;
    let repo = HomeworkRepositoryPg::new(pool);

    let homework = create_test_homework(instance_id, teacher_id, UserRole::Teacher);
    let file_1 = create_test_file(homework.id, 0);
    let file_2 = create_test_file(homework.id, 1);

    let result = repo.create_with_files(homework.clone(), vec![file_1, file_2]).await;

    assert_eq!(result.unwrap(), homework);
    let fetched = repo.get_by_id(homework.id).await.unwrap();
    assert_eq!(fetched.id, homework.id);

    let files = repo.get_files(homework.id).await.unwrap();
    assert_eq!(files.len(), 2);
}

/// Test: create_with_files fails atomically on a duplicate lesson instance.
///
/// The homework INSERT raises a unique violation (idx_homeworks_instance_unique),
/// the transaction rolls back, and NO files are left behind.
#[sqlx::test(migrations = "../../migrations")]
async fn test_create_with_files_duplicate_lesson_instance_rolls_back(pool: PgPool) {
    let (instance_id, teacher_id, _) = seed_lesson_instance(&pool).await;
    let repo = HomeworkRepositoryPg::new(pool);

    let homework_1 = create_test_homework(instance_id, teacher_id, UserRole::Teacher);
    repo.create_with_files(homework_1.clone(), vec![create_test_file(homework_1.id, 0)])
        .await
        .expect("First create should succeed");

    // Second homework for the SAME lesson instance must fail
    let homework_2 = create_test_homework(instance_id, teacher_id, UserRole::Teacher);
    let file = create_test_file(homework_2.id, 0);
    let result = repo.create_with_files(homework_2.clone(), vec![file]).await;

    assert!(matches!(result, Err(DomainError::HomeworkAlreadyExists)));

    // No partial state: homework_2 was never persisted, and its files are gone
    assert!(matches!(repo.get_by_id(homework_2.id).await, Err(DomainError::HomeworkNotFound)));
    assert!(repo.get_files(homework_2.id).await.unwrap().is_empty());
}

// ============================================================================
// DOMAIN INVARIANT VALIDATION (pure unit tests, no DB)
// ============================================================================

/// Test: try_new rejects whitespace-only text content.
#[test]
fn test_homework_try_new_rejects_empty_text() {
    let result = Homework::try_new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        UserRole::Teacher,
        Some("   ".to_string()),
        HomeworkStatus::Draft,
        false,
        None,
    );

    assert!(matches!(result, Err(DomainError::InvalidHomeworkTextFormat)));
}

/// Test: try_new allows None text content (file-only homework).
#[test]
fn test_homework_try_new_allows_no_text() {
    let result = Homework::try_new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        UserRole::Teacher,
        None,
        HomeworkStatus::Draft,
        false,
        None,
    );

    assert!(result.is_ok());
    assert_eq!(result.unwrap().text_content, None);
}

/// Test: try_new trims leading/trailing whitespace from text content.
#[test]
fn test_homework_try_new_trims_text() {
    let homework = Homework::try_new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        UserRole::Teacher,
        Some("  Solve problems  " .to_string()),
        HomeworkStatus::Draft,
        false,
        None,
    )
    .expect("Valid homework");

    assert_eq!(homework.text_content, Some("Solve problems".to_string()));
}

/// Test: HomeworkFile::try_new rejects an empty storage key.
#[test]
fn test_homework_file_rejects_empty_storage_key() {
    let result = HomeworkFile::try_new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "   ".to_string(),
        "hw.pdf".to_string(),
        "application/pdf".to_string(),
        100,
        0,
    );

    assert!(matches!(result, Err(DomainError::InvalidHomeworkFileFormat)));
}

/// Test: HomeworkFile::try_new rejects a negative size.
#[test]
fn test_homework_file_rejects_negative_size() {
    let result = HomeworkFile::try_new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "homeworks/2026/07/a.pdf".to_string(),
        "hw.pdf".to_string(),
        "application/pdf".to_string(),
        -1,
        0,
    );

    assert!(matches!(result, Err(DomainError::InvalidHomeworkFileSize)));
}

/// Test: HomeworkFile::try_new rejects a file name longer than the DB limit.
#[test]
fn test_homework_file_rejects_overlong_file_name() {
    let long_name = "x".repeat(256);
    let result = HomeworkFile::try_new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "homeworks/2026/07/a.pdf".to_string(),
        long_name,
        "application/pdf".to_string(),
        100,
        0,
    );

    assert!(matches!(result, Err(DomainError::InvalidHomeworkFileFormat)));
}

/// Test: HomeworkStatus parses valid values and rejects garbage.
#[test]
fn test_homework_status_from_str() {
    assert_eq!(HomeworkStatus::from_str("draft").unwrap(), HomeworkStatus::Draft);
    assert_eq!(HomeworkStatus::from_str("published").unwrap(), HomeworkStatus::Published);
    assert_eq!(HomeworkStatus::from_str("archived").unwrap(), HomeworkStatus::Archived);
    assert_eq!(HomeworkStatus::from_str("DRAFT").unwrap(), HomeworkStatus::Draft); // case-insensitive
    assert!(matches!(
        HomeworkStatus::from_str("foo"),
        Err(DomainError::InvalidHomeworkStatus)
    ));
}

/// Test: HomeworkStatus Display round-trips to lowercase DB values.
#[test]
fn test_homework_status_display() {
    assert_eq!(HomeworkStatus::Draft.to_string(), "draft");
    assert_eq!(HomeworkStatus::Published.to_string(), "published");
    assert_eq!(HomeworkStatus::Archived.to_string(), "archived");
}

/// Test: visibility helpers follow the migration semantics.
#[test]
fn test_homework_status_visibility() {
    assert!(!HomeworkStatus::Draft.is_visible_to_students());
    assert!(HomeworkStatus::Published.is_visible_to_students());
    assert!(!HomeworkStatus::Archived.is_visible_to_students());
}
