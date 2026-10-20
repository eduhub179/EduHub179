//! Integration tests for `AnnouncementRepositoryPg`.
//!
//! These tests verify the public API of the infrastructure crate
//! with a real PostgreSQL database using `sqlx::test` for automatic
//! transaction management and rollback.
//!
//! Coverage:
//! - Announcement CRUD: `get_by_id`, `save` (create/update/errors), `delete`.
//! - Targeting: `set_targets` (replace/clear/everyone), `get_visible_for_user`
//!   (class/group/user targeting, expiry, status filtering, ordering).
//! - Creator query: `get_by_creator`.
//! - Files: `add_file`, `get_files`, `remove_file`.
//! - Domain invariant validation (pure unit tests, no DB).
//!
//! DB fixture: `announcements` references `users`, `classes`, `student_groups`.
//! Fixtures are seeded through the real repositories.
use chrono::{DateTime, Utc};
use domain::entities::announcement::{Announcement, AnnouncementFile};
use domain::entities::class::Class;
use domain::entities::student_group::StudentGroup;
use domain::entities::user::User;
use domain::errors::DomainError;
use domain::repositories::announcement_repository::AnnouncementRepository;
use domain::repositories::class_repository::ClassRepository;
use domain::repositories::student_group_repository::StudentGroupRepository;
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::announcement_priority::AnnouncementPriority;
use domain::value_objects::announcement_status::AnnouncementStatus;
use domain::value_objects::class_letter::ClassLetter;
use domain::value_objects::role::UserRole;
use infrastructure::postgres::{
    AnnouncementRepositoryPg, ClassRepositoryPg, StudentGroupRepositoryPg, UserRepositoryPg,
};
use sqlx::PgPool;
use std::sync::atomic::{AtomicI32, Ordering};
use uuid::Uuid;

// ============================================================================
// GLOBAL COUNTERS
// ============================================================================

/// Monotonic counter so multiple `seed_*` calls inside one test produce
/// distinct (graduation_year, class_letter) pairs (unique index on classes).
static CLASS_SEQ: AtomicI32 = AtomicI32::new(0);

// ============================================================================
// HELPERS
// ============================================================================

/// Helper: creates a test teacher with a random ID and unique login.
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

/// Helper: creates a test student with a random ID and unique login.
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

/// Helper: creates a test class with a random ID and a unique graduation_year
/// (monotonic counter avoids unique-index collisions when multiple seeds run
/// in the same test).
fn create_test_class(letter: ClassLetter) -> Class {
    let year = 2027 + CLASS_SEQ.fetch_add(1, Ordering::Relaxed);
    Class::try_new(Uuid::new_v4(), year, letter, true).expect("Test class data should be valid")
}

/// Helper: creates a test student group with a random ID.
fn create_test_group(name: &str) -> StudentGroup {
    StudentGroup::try_new(Uuid::new_v4(), name.to_string())
        .expect("Test group data should be valid")
}

/// Helper: creates a ready-to-save announcement with random ID.
fn create_announcement(
    title: Option<String>,
    content: Option<String>,
    created_by: Uuid,
    status: AnnouncementStatus,
    priority: AnnouncementPriority,
    expires_at: Option<DateTime<Utc>>,
) -> Announcement {
    Announcement::try_new(
        Uuid::new_v4(),
        title,
        content,
        created_by,
        status,
        priority,
        expires_at,
        Utc::now(),
    )
    .expect("Test announcement should be valid")
}

/// Helper: creates a ready-to-save file with random ID.
fn create_file(announcement_id: Uuid, sort_order: i32) -> AnnouncementFile {
    AnnouncementFile::try_new(
        Uuid::new_v4(),
        announcement_id,
        format!("announcements/2026/10/{}.jpg", Uuid::new_v4()),
        "photo.jpg".to_string(),
        "image/jpeg".to_string(),
        1024,
        sort_order,
    )
    .expect("Test file should be valid")
}

/// Seeds a teacher and a student via the real repositories.
/// Returns `(teacher_id, student_id)`.
async fn seed_users(pool: &PgPool) -> (Uuid, Uuid) {
    let user_repo = UserRepositoryPg::new(pool.clone());
    let teacher = create_test_teacher();
    let student = create_test_student();
    user_repo.save(teacher.clone()).await.expect("Save teacher");
    user_repo.save(student.clone()).await.expect("Save student");
    (teacher.id, student.id)
}

/// Seeds a class and a group via the real repositories.
/// Returns `(class_id, group_id)`.
async fn seed_class_and_group(pool: &PgPool) -> (Uuid, Uuid) {
    let class_repo = ClassRepositoryPg::new(pool.clone());
    let group_repo = StudentGroupRepositoryPg::new(pool.clone());
    let class = create_test_class(ClassLetter::B);
    // Generate unique group name to avoid conflicts when called multiple times
    let group_name = format!("Английский B1 - {}", Uuid::new_v4());
    let group = create_test_group(&group_name);
    class_repo.save(class.clone()).await.expect("Save class");
    group_repo.save(group.clone()).await.expect("Save group");
    (class.id, group.id)
}
/// Seeds a student and assigns them to a class and a group.
/// Returns `(student_id, class_id, group_id)`.
async fn seed_student_with_targets(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
    let (_teacher_id, student_id) = seed_users(pool).await;
    let (class_id, group_id) = seed_class_and_group(pool).await;

    // Assign student to class (via users.class_id update)
    sqlx::query("UPDATE users SET class_id = $1 WHERE user_id = $2")
        .bind(class_id)
        .bind(student_id)
        .execute(pool)
        .await
        .expect("Assign student to class");

    // Assign student to group (via group_members)
    sqlx::query(
        "INSERT INTO group_members (student_id, group_id) VALUES ($1, $2)
         ON CONFLICT (student_id, group_id) DO NOTHING",
    )
    .bind(student_id)
    .bind(group_id)
    .execute(pool)
    .await
    .expect("Add student to group");

    (student_id, class_id, group_id)
}

// ============================================================================
// DOMAIN INVARIANT VALIDATION (pure unit tests, no DB)
// ============================================================================

#[test]
fn test_announcement_try_new_with_title_and_content() {
    let ann = Announcement::try_new(
        Uuid::new_v4(),
        Some("Title".to_string()),
        Some("**Bold** content".to_string()),
        Uuid::new_v4(),
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
        Utc::now(),
    )
    .expect("Valid announcement");
    assert_eq!(ann.title.as_deref(), Some("Title"));
    assert_eq!(ann.content.as_deref(), Some("**Bold** content"));
}

#[test]
fn test_announcement_try_new_file_only() {
    // Both title and content can be None (file-only announcement)
    let ann = Announcement::try_new(
        Uuid::new_v4(),
        None,
        None,
        Uuid::new_v4(),
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
        Utc::now(),
    )
    .expect("File-only announcement is valid");
    assert_eq!(ann.title, None);
    assert_eq!(ann.content, None);
}

#[test]
fn test_announcement_try_new_title_only() {
    let ann = Announcement::try_new(
        Uuid::new_v4(),
        Some("Title only".to_string()),
        None,
        Uuid::new_v4(),
        AnnouncementStatus::Published,
        AnnouncementPriority::High,
        None,
        Utc::now(),
    )
    .expect("Title-only announcement is valid");
    assert_eq!(ann.title.as_deref(), Some("Title only"));
    assert_eq!(ann.content, None);
}

#[test]
fn test_announcement_try_new_content_only() {
    let ann = Announcement::try_new(
        Uuid::new_v4(),
        None,
        Some("Content without title".to_string()),
        Uuid::new_v4(),
        AnnouncementStatus::Draft,
        AnnouncementPriority::Low,
        None,
        Utc::now(),
    )
    .expect("Content-only announcement is valid");
    assert_eq!(ann.title, None);
    assert_eq!(ann.content.as_deref(), Some("Content without title"));
}

#[test]
fn test_announcement_try_new_rejects_empty_title() {
    let err = Announcement::try_new(
        Uuid::new_v4(),
        Some("   ".to_string()),
        Some("Content".to_string()),
        Uuid::new_v4(),
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
        Utc::now(),
    )
    .unwrap_err();
    assert_eq!(err, DomainError::InvalidAnnouncementTitle);
}

#[test]
fn test_announcement_try_new_rejects_overlong_title() {
    let err = Announcement::try_new(
        Uuid::new_v4(),
        Some("x".repeat(256)),
        Some("Content".to_string()),
        Uuid::new_v4(),
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
        Utc::now(),
    )
    .unwrap_err();
    assert_eq!(err, DomainError::InvalidAnnouncementTitle);
}

#[test]
fn test_announcement_try_new_accepts_max_length_title() {
    let ann = Announcement::try_new(
        Uuid::new_v4(),
        Some("x".repeat(255)),
        Some("Content".to_string()),
        Uuid::new_v4(),
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
        Utc::now(),
    )
    .expect("255 chars should be valid");
    assert_eq!(ann.title.unwrap().chars().count(), 255);
}

#[test]
fn test_announcement_try_new_rejects_whitespace_only_content() {
    let err = Announcement::try_new(
        Uuid::new_v4(),
        Some("Title".to_string()),
        Some("   ".to_string()),
        Uuid::new_v4(),
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
        Utc::now(),
    )
    .unwrap_err();
    assert_eq!(err, DomainError::InvalidAnnouncementContent);
}

#[test]
fn test_announcement_try_new_trims_title_and_content() {
    let ann = Announcement::try_new(
        Uuid::new_v4(),
        Some("  Title  ".to_string()),
        Some("  **Bold**  ".to_string()),
        Uuid::new_v4(),
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
        Utc::now(),
    )
    .expect("Trimmed values should be valid");
    assert_eq!(ann.title.as_deref(), Some("Title"));
    assert_eq!(ann.content.as_deref(), Some("**Bold**"));
}

#[test]
fn test_announcement_file_try_new_valid() {
    let file = AnnouncementFile::try_new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "announcements/2026/10/abc.jpg".to_string(),
        "photo.jpg".to_string(),
        "image/jpeg".to_string(),
        1024,
        0,
    )
    .expect("Valid file");
    assert_eq!(file.file_name, "photo.jpg");
    assert_eq!(file.size_bytes, 1024);
}

#[test]
fn test_announcement_file_try_new_rejects_empty_storage_key() {
    let err = AnnouncementFile::try_new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "   ".to_string(),
        "photo.jpg".to_string(),
        "image/jpeg".to_string(),
        1024,
        0,
    )
    .unwrap_err();
    assert_eq!(err, DomainError::InvalidAnnouncementFileFormat);
}

#[test]
fn test_announcement_file_try_new_rejects_negative_size() {
    let err = AnnouncementFile::try_new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "announcements/2026/10/abc.jpg".to_string(),
        "photo.jpg".to_string(),
        "image/jpeg".to_string(),
        -1,
        0,
    )
    .unwrap_err();
    assert_eq!(err, DomainError::InvalidAnnouncementFileSize);
}

#[test]
fn test_announcement_file_try_new_accepts_zero_size() {
    let file = AnnouncementFile::try_new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "announcements/2026/10/abc.jpg".to_string(),
        "photo.jpg".to_string(),
        "image/jpeg".to_string(),
        0,
        0,
    )
    .expect("Zero size should be valid");
    assert_eq!(file.size_bytes, 0);
}

// ============================================================================
// GET BY ID
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_not_found(pool: PgPool) {
    let repo = AnnouncementRepositoryPg::new(pool);
    let err = repo.get_by_id(Uuid::new_v4()).await.unwrap_err();
    assert_eq!(err, DomainError::AnnouncementNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_success(pool: PgPool) {
    let (teacher_id, _) = seed_users(&pool).await;
    let repo = AnnouncementRepositoryPg::new(pool);
    let ann = create_announcement(
        Some("Test title".to_string()),
        Some("**Bold** content".to_string()),
        teacher_id,
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
    );
    repo.save(ann.clone()).await.expect("Save announcement");
    let fetched = repo.get_by_id(ann.id).await.expect("Get by ID");
    assert_eq!(fetched.id, ann.id);
    assert_eq!(fetched.title.as_deref(), Some("Test title"));
    assert_eq!(fetched.content.as_deref(), Some("**Bold** content"));
    assert_eq!(fetched.created_by, teacher_id);
    assert!(fetched.status.is_draft());
    assert_eq!(fetched.priority, AnnouncementPriority::Normal);
    assert_eq!(fetched.expires_at, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_file_only_roundtrip(pool: PgPool) {
    let (teacher_id, _) = seed_users(&pool).await;
    let repo = AnnouncementRepositoryPg::new(pool);
    let ann = create_announcement(
        None,
        None,
        teacher_id,
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
    );
    repo.save(ann.clone())
        .await
        .expect("Save file-only announcement");
    let fetched = repo.get_by_id(ann.id).await.expect("Get by ID");
    assert_eq!(fetched.title, None);
    assert_eq!(fetched.content, None);
}

// ============================================================================
// SAVE (CREATE / UPDATE / ERRORS)
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_creates_announcement(pool: PgPool) {
    let (teacher_id, _) = seed_users(&pool).await;
    let repo = AnnouncementRepositoryPg::new(pool);
    let ann = create_announcement(
        Some("New announcement".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
    );
    let saved = repo.save(ann.clone()).await.expect("Save should succeed");
    let fetched = repo.get_by_id(ann.id).await.expect("Fetch should succeed");
    assert_eq!(fetched.id, saved.id);
    // Postgres stores TIMESTAMPTZ with MICROsecond precision, while the
    // entity's chrono timestamp carries nanoseconds. Compare at the DB's
    // precision for exact equality.
    let expected_created_at =
        chrono::DateTime::from_timestamp_micros(ann.created_at.timestamp_micros()).unwrap();
    assert_eq!(fetched.created_at, expected_created_at);
    assert_eq!(fetched.title, ann.title);
    assert_eq!(fetched.content, ann.content);
    assert_eq!(fetched.created_by, teacher_id);
    assert!(fetched.status.is_draft());
    assert_eq!(fetched.priority, AnnouncementPriority::Normal);
    assert_eq!(fetched.expires_at, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_updates_existing_announcement(pool: PgPool) {
    let (teacher_id, _) = seed_users(&pool).await;
    let repo = AnnouncementRepositoryPg::new(pool);
    let ann = create_announcement(
        Some("Original title".to_string()),
        Some("Original content".to_string()),
        teacher_id,
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
    );
    repo.save(ann.clone()).await.expect("Save original");
    // Update: change title, content, status, priority
    let updated = Announcement::try_new(
        ann.id,
        Some("Updated title".to_string()),
        Some("Updated content".to_string()),
        ann.created_by,
        AnnouncementStatus::Published,
        AnnouncementPriority::Urgent,
        ann.expires_at,
        ann.created_at,
    )
    .expect("Valid updated announcement");
    repo.save(updated.clone())
        .await
        .expect("Update should succeed");
    let fetched = repo.get_by_id(ann.id).await.expect("Fetch updated");
    assert_eq!(fetched.title.as_deref(), Some("Updated title"));
    assert_eq!(fetched.content.as_deref(), Some("Updated content"));
    assert!(fetched.status.is_published());
    assert_eq!(fetched.priority, AnnouncementPriority::Urgent);
    // Immutable fields preserved
    assert_eq!(fetched.created_by, teacher_id);
    assert_eq!(
        fetched.created_at.timestamp_micros(),
        ann.created_at.timestamp_micros()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_with_non_existent_creator_returns_user_not_found(pool: PgPool) {
    let repo = AnnouncementRepositoryPg::new(pool);
    let ann = create_announcement(
        Some("Title".to_string()),
        Some("Content".to_string()),
        Uuid::new_v4(), // non-existent user
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
    );
    let err = repo.save(ann).await.unwrap_err();
    assert_eq!(err, DomainError::UserNotFound);
}

// ============================================================================
// DELETE
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_delete_success_cascades_files(pool: PgPool) {
    let (teacher_id, _) = seed_users(&pool).await;
    let repo = AnnouncementRepositoryPg::new(pool.clone());
    let ann = create_announcement(
        Some("To delete".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
    );
    repo.save(ann.clone()).await.expect("Save announcement");
    // Add files
    let file1 = create_file(ann.id, 0);
    let file2 = create_file(ann.id, 1);
    repo.add_file(file1).await.expect("Add file1");
    repo.add_file(file2).await.expect("Add file2");
    // Delete
    repo.delete(ann.id).await.expect("Delete should succeed");
    // Announcement is gone
    let err = repo.get_by_id(ann.id).await.unwrap_err();
    assert_eq!(err, DomainError::AnnouncementNotFound);
    // Files cascaded
    let files = repo.get_files(ann.id).await.expect("List files");
    assert!(files.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_delete_not_found(pool: PgPool) {
    let repo = AnnouncementRepositoryPg::new(pool);
    let err = repo.delete(Uuid::new_v4()).await.unwrap_err();
    assert_eq!(err, DomainError::AnnouncementNotFound);
}

// ============================================================================
// TARGETING
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_set_targets_replace(pool: PgPool) {
    let (teacher_id, student_id) = seed_users(&pool).await;
    let (class_id, group_id) = seed_class_and_group(&pool).await;
    let repo = AnnouncementRepositoryPg::new(pool.clone());
    let ann = create_announcement(
        Some("Targeted".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
    );
    repo.save(ann.clone()).await.expect("Save announcement");
    // Set initial targets
    repo.set_targets(ann.id, &[class_id], &[group_id], &[student_id])
        .await
        .expect("Set targets");
    // Replace with different targets
    let (_teacher_id2, student_id2) = seed_users(&pool).await;
    let (class_id2, group_id2) = seed_class_and_group(&pool).await;
    repo.set_targets(ann.id, &[class_id2], &[group_id2], &[student_id2])
        .await
        .expect("Replace targets");
    // Verify: old targets are gone, new targets are present
    // (We verify indirectly via get_visible_for_user in a separate test)
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_set_targets_clear(pool: PgPool) {
    let (teacher_id, student_id) = seed_users(&pool).await;
    let (class_id, group_id) = seed_class_and_group(&pool).await;
    let repo = AnnouncementRepositoryPg::new(pool.clone());
    let ann = create_announcement(
        Some("Targeted".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
    );
    repo.save(ann.clone()).await.expect("Save announcement");
    repo.set_targets(ann.id, &[class_id], &[group_id], &[student_id])
        .await
        .expect("Set targets");
    // Clear all targets
    repo.set_targets(ann.id, &[], &[], &[])
        .await
        .expect("Clear targets");
    // Announcement is now visible to everyone (verified in get_visible_for_user tests)
}

// ============================================================================
// GET VISIBLE FOR USER
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_visible_for_user_draft_not_visible(pool: PgPool) {
    let (teacher_id, student_id) = seed_users(&pool).await;
    let repo = AnnouncementRepositoryPg::new(pool);
    let ann = create_announcement(
        Some("Draft".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
    );
    repo.save(ann).await.expect("Save draft");
    let visible = repo
        .get_visible_for_user(student_id)
        .await
        .expect("Query should succeed");
    assert!(
        visible.is_empty(),
        "Draft announcements should not be visible"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_visible_for_user_published_no_targets_visible_to_everyone(pool: PgPool) {
    let (teacher_id, student_id) = seed_users(&pool).await;
    let repo = AnnouncementRepositoryPg::new(pool);
    let ann = create_announcement(
        Some("Published for everyone".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Published,
        AnnouncementPriority::Normal,
        None,
    );
    repo.save(ann.clone()).await.expect("Save announcement");
    // No targets set → visible to everyone
    let visible = repo
        .get_visible_for_user(student_id)
        .await
        .expect("Query should succeed");
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id, ann.id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_visible_for_user_class_targeting(pool: PgPool) {
    let (student_id, class_id, _group_id) = seed_student_with_targets(&pool).await;
    let (teacher_id, other_student_id) = seed_users(&pool).await;
    // Other student gets their own class (different year thanks to CLASS_SEQ)
    let other_class = create_test_class(ClassLetter::V);
    ClassRepositoryPg::new(pool.clone())
        .save(other_class.clone())
        .await
        .expect("Save other class");
    sqlx::query("UPDATE users SET class_id = $1 WHERE user_id = $2")
        .bind(other_class.id)
        .bind(other_student_id)
        .execute(&pool)
        .await
        .expect("Assign other student to other class");

    let repo = AnnouncementRepositoryPg::new(pool.clone());
    let ann = create_announcement(
        Some("For class 11б".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Published,
        AnnouncementPriority::Normal,
        None,
    );
    repo.save(ann.clone()).await.expect("Save announcement");
    // Target only class_id
    repo.set_targets(ann.id, &[class_id], &[], &[])
        .await
        .expect("Set class target");
    // student_id is in class_id → should see it
    let visible = repo
        .get_visible_for_user(student_id)
        .await
        .expect("Query should succeed");
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id, ann.id);
    // other_student_id is in other_class_id → should NOT see it
    let visible_other = repo
        .get_visible_for_user(other_student_id)
        .await
        .expect("Query should succeed");
    assert!(visible_other.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_visible_for_user_group_targeting(pool: PgPool) {
    let (student_id, _class_id, group_id) = seed_student_with_targets(&pool).await;
    let (teacher_id, other_student_id) = seed_users(&pool).await;
    let repo = AnnouncementRepositoryPg::new(pool.clone());
    let ann = create_announcement(
        Some("For group".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Published,
        AnnouncementPriority::Normal,
        None,
    );
    repo.save(ann.clone()).await.expect("Save announcement");
    // Target only group_id
    repo.set_targets(ann.id, &[], &[group_id], &[])
        .await
        .expect("Set group target");
    // student_id is in group_id → should see it
    let visible = repo
        .get_visible_for_user(student_id)
        .await
        .expect("Query should succeed");
    assert_eq!(visible.len(), 1);
    // other_student_id is not in group_id → should NOT see it
    let visible_other = repo
        .get_visible_for_user(other_student_id)
        .await
        .expect("Query should succeed");
    assert!(visible_other.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_visible_for_user_direct_user_targeting(pool: PgPool) {
    let (teacher_id, student_id) = seed_users(&pool).await;
    let (_, other_student_id) = seed_users(&pool).await;
    let repo = AnnouncementRepositoryPg::new(pool.clone());
    let ann = create_announcement(
        Some("For specific user".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Published,
        AnnouncementPriority::Normal,
        None,
    );
    repo.save(ann.clone()).await.expect("Save announcement");
    // Target only student_id
    repo.set_targets(ann.id, &[], &[], &[student_id])
        .await
        .expect("Set user target");
    // student_id should see it
    let visible = repo
        .get_visible_for_user(student_id)
        .await
        .expect("Query should succeed");
    assert_eq!(visible.len(), 1);
    // other_student_id should NOT see it
    let visible_other = repo
        .get_visible_for_user(other_student_id)
        .await
        .expect("Query should succeed");
    assert!(visible_other.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_visible_for_user_expired_not_visible(pool: PgPool) {
    let (teacher_id, student_id) = seed_users(&pool).await;
    let repo = AnnouncementRepositoryPg::new(pool);
    let ann = create_announcement(
        Some("Expired".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Published,
        AnnouncementPriority::Normal,
        Some(Utc::now() - chrono::Duration::days(1)), // expired yesterday
    );
    repo.save(ann).await.expect("Save expired announcement");
    let visible = repo
        .get_visible_for_user(student_id)
        .await
        .expect("Query should succeed");
    assert!(
        visible.is_empty(),
        "Expired announcements should not be visible"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_visible_for_user_not_yet_expired_visible(pool: PgPool) {
    let (teacher_id, student_id) = seed_users(&pool).await;
    let repo = AnnouncementRepositoryPg::new(pool);
    let ann = create_announcement(
        Some("Not expired".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Published,
        AnnouncementPriority::Normal,
        Some(Utc::now() + chrono::Duration::days(7)), // expires in a week
    );
    repo.save(ann.clone()).await.expect("Save announcement");
    let visible = repo
        .get_visible_for_user(student_id)
        .await
        .expect("Query should succeed");
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id, ann.id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_visible_for_user_ordered_by_priority_then_created_at(pool: PgPool) {
    let (teacher_id, student_id) = seed_users(&pool).await;
    let repo = AnnouncementRepositoryPg::new(pool.clone());
    // Create announcements with different priorities and creation times
    let low = create_announcement(
        Some("Low priority".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Published,
        AnnouncementPriority::Low,
        None,
    );
    let urgent = create_announcement(
        Some("Urgent".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Published,
        AnnouncementPriority::Urgent,
        None,
    );
    let normal = create_announcement(
        Some("Normal".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Published,
        AnnouncementPriority::Normal,
        None,
    );
    let high = create_announcement(
        Some("High".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Published,
        AnnouncementPriority::High,
        None,
    );
    // Save in non-sorted order
    repo.save(low.clone()).await.expect("Save low");
    repo.save(urgent.clone()).await.expect("Save urgent");
    repo.save(normal.clone()).await.expect("Save normal");
    repo.save(high.clone()).await.expect("Save high");
    let visible = repo
        .get_visible_for_user(student_id)
        .await
        .expect("Query should succeed");
    assert_eq!(visible.len(), 4);
    // Ordered by priority DESC (urgent → high → normal → low), then created_at DESC
    assert_eq!(visible[0].priority, AnnouncementPriority::Urgent);
    assert_eq!(visible[1].priority, AnnouncementPriority::High);
    assert_eq!(visible[2].priority, AnnouncementPriority::Normal);
    assert_eq!(visible[3].priority, AnnouncementPriority::Low);
}

// ============================================================================
// GET BY CREATOR
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_creator_returns_all_statuses(pool: PgPool) {
    let (teacher_id, _student_id) = seed_users(&pool).await;
    let repo = AnnouncementRepositoryPg::new(pool.clone());
    let draft = create_announcement(
        Some("Draft".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
    );
    let published = create_announcement(
        Some("Published".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Published,
        AnnouncementPriority::Normal,
        None,
    );
    let archived = create_announcement(
        Some("Archived".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Archived,
        AnnouncementPriority::Normal,
        None,
    );
    repo.save(draft.clone()).await.expect("Save draft");
    repo.save(published.clone()).await.expect("Save published");
    repo.save(archived.clone()).await.expect("Save archived");
    let created = repo
        .get_by_creator(teacher_id)
        .await
        .expect("Query should succeed");
    assert_eq!(created.len(), 3);
    // Ordered by created_at DESC
    assert_eq!(created[0].id, archived.id);
    assert_eq!(created[1].id, published.id);
    assert_eq!(created[2].id, draft.id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_creator_empty_for_unknown_user(pool: PgPool) {
    let repo = AnnouncementRepositoryPg::new(pool);
    let created = repo
        .get_by_creator(Uuid::new_v4())
        .await
        .expect("Query should succeed");
    assert!(created.is_empty());
}

// ============================================================================
// FILES
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_add_file_and_get_files(pool: PgPool) {
    let (teacher_id, _) = seed_users(&pool).await;
    let repo = AnnouncementRepositoryPg::new(pool.clone());
    let ann = create_announcement(
        Some("With files".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
    );
    repo.save(ann.clone()).await.expect("Save announcement");
    let file1 = create_file(ann.id, 1);
    let file2 = create_file(ann.id, 0); // lower sort_order
    repo.add_file(file1.clone()).await.expect("Add file1");
    repo.add_file(file2.clone()).await.expect("Add file2");
    let files = repo
        .get_files(ann.id)
        .await
        .expect("Get files should succeed");
    assert_eq!(files.len(), 2);
    // Ordered by sort_order
    assert_eq!(files[0].id, file2.id);
    assert_eq!(files[1].id, file1.id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_files_empty_for_announcement_without_files(pool: PgPool) {
    let (teacher_id, _) = seed_users(&pool).await;
    let repo = AnnouncementRepositoryPg::new(pool);
    let ann = create_announcement(
        Some("No files".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
    );
    repo.save(ann.clone()).await.expect("Save announcement");
    let files = repo
        .get_files(ann.id)
        .await
        .expect("Get files should succeed");
    assert!(files.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_files_empty_for_non_existent_announcement(pool: PgPool) {
    let repo = AnnouncementRepositoryPg::new(pool);
    let files = repo
        .get_files(Uuid::new_v4())
        .await
        .expect("Get files should succeed");
    assert!(files.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_remove_file_success(pool: PgPool) {
    let (teacher_id, _) = seed_users(&pool).await;
    let repo = AnnouncementRepositoryPg::new(pool.clone());
    let ann = create_announcement(
        Some("With file".to_string()),
        Some("Content".to_string()),
        teacher_id,
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
    );
    repo.save(ann.clone()).await.expect("Save announcement");
    let file = create_file(ann.id, 0);
    repo.add_file(file.clone()).await.expect("Add file");
    repo.remove_file(file.id).await.expect("Remove file");
    let files = repo
        .get_files(ann.id)
        .await
        .expect("Get files should succeed");
    assert!(files.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_remove_file_not_found(pool: PgPool) {
    let repo = AnnouncementRepositoryPg::new(pool);
    let err = repo.remove_file(Uuid::new_v4()).await.unwrap_err();
    assert_eq!(err, DomainError::AnnouncementFileNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_add_file_with_non_existent_announcement_returns_not_found(pool: PgPool) {
    let repo = AnnouncementRepositoryPg::new(pool);
    let file = create_file(Uuid::new_v4(), 0);
    let err = repo.add_file(file).await.unwrap_err();
    assert_eq!(err, DomainError::AnnouncementNotFound);
}

// ============================================================================
// COMPLEX SCENARIO: FULL LIFECYCLE
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_full_announcement_lifecycle(pool: PgPool) {
    let (teacher_id, student_id) = seed_users(&pool).await;
    let (class_id, group_id) = seed_class_and_group(&pool).await;

    // ВАЖНО: привязываем студента к классу и группе, иначе visibility query
    // не найдёт таргетов и вернёт 0 строк.
    sqlx::query("UPDATE users SET class_id = $1 WHERE user_id = $2")
        .bind(class_id)
        .bind(student_id)
        .execute(&pool)
        .await
        .expect("Assign student to class");

    sqlx::query(
        "INSERT INTO group_members (student_id, group_id) VALUES ($1, $2)
         ON CONFLICT (student_id, group_id) DO NOTHING",
    )
    .bind(student_id)
    .bind(group_id)
    .execute(&pool)
    .await
    .expect("Add student to group");

    let repo = AnnouncementRepositoryPg::new(pool.clone());

    // 1. Create draft announcement
    let ann = create_announcement(
        Some("Important announcement".to_string()),
        Some("**Bold** content with [link](https://example.com)".to_string()),
        teacher_id,
        AnnouncementStatus::Draft,
        AnnouncementPriority::Normal,
        None,
    );
    repo.save(ann.clone()).await.expect("Save draft");

    // 2. Add files
    let file1 = create_file(ann.id, 0);
    let file2 = create_file(ann.id, 1);
    repo.add_file(file1.clone()).await.expect("Add file1");
    repo.add_file(file2.clone()).await.expect("Add file2");

    // 3. Set targets (class + group)
    repo.set_targets(ann.id, &[class_id], &[group_id], &[])
        .await
        .expect("Set targets");

    // 4. Publish
    let published = Announcement::try_new(
        ann.id,
        ann.title.clone(),
        ann.content.clone(),
        ann.created_by,
        AnnouncementStatus::Published,
        AnnouncementPriority::High,
        ann.expires_at,
        ann.created_at,
    )
    .expect("Valid published announcement");
    repo.save(published.clone()).await.expect("Publish");

    // 5. Verify student can see it
    let visible = repo
        .get_visible_for_user(student_id)
        .await
        .expect("Query should succeed");
    assert_eq!(
        visible.len(),
        1,
        "Student should see the published announcement"
    );
    assert_eq!(visible[0].id, ann.id);
    assert_eq!(visible[0].priority, AnnouncementPriority::High);

    // 6. Verify files are attached
    let files = repo
        .get_files(ann.id)
        .await
        .expect("Get files should succeed");
    assert_eq!(files.len(), 2);

    // 7. Archive
    let archived = Announcement::try_new(
        ann.id,
        ann.title,
        ann.content,
        ann.created_by,
        AnnouncementStatus::Archived,
        ann.priority,
        ann.expires_at,
        ann.created_at,
    )
    .expect("Valid archived announcement");
    repo.save(archived).await.expect("Archive");

    // 8. Verify student can no longer see it (archived)
    let visible_after = repo
        .get_visible_for_user(student_id)
        .await
        .expect("Query should succeed");
    assert!(
        visible_after.is_empty(),
        "Archived announcement should not be visible"
    );

    // 9. Delete
    repo.delete(ann.id).await.expect("Delete should succeed");
    let err = repo.get_by_id(ann.id).await.unwrap_err();
    assert_eq!(err, DomainError::AnnouncementNotFound);
}
