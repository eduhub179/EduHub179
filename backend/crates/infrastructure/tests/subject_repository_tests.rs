//! Integration tests for SubjectRepositoryPg.
//!
//! These tests verify the public API of the infrastructure crate
//! with a real PostgreSQL database using sqlx::test for automatic
//! transaction management and rollback.

use domain::entities::subject::Subject;
use domain::errors::DomainError;
use domain::repositories::subject_repository::SubjectRepository;
use infrastructure::postgres::SubjectRepositoryPg;
use sqlx::PgPool;
use uuid::Uuid;

/// Helper: creates a test subject with random ID.
/// Panics if the domain invariants are violated (which shouldn't happen with valid test data).
fn create_test_subject(name: &str) -> Subject {
    Subject::try_new(Uuid::new_v4(), name.to_string())
        .expect("Test data should be valid and satisfy domain invariants")
}

// ============================================================================
// TESTS FOR get_by_id
// ============================================================================

/// Test: get_by_id returns SubjectNotFound for non-existent ID.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_not_found(pool: PgPool) {
    let repo = SubjectRepositoryPg::new(pool);
    let fake_id = Uuid::new_v4();

    let result = repo.get_by_id(fake_id).await;

    assert!(matches!(result, Err(DomainError::SubjectNotFound)));
}

/// Test: get_by_id returns the correct subject.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_success(pool: PgPool) {
    let repo = SubjectRepositoryPg::new(pool);
    let subject = create_test_subject("Алгебра");

    repo.save(subject.clone()).await.expect("Save should succeed");

    let fetched = repo.get_by_id(subject.id).await.expect("Get by ID should succeed");

    assert_eq!(fetched.id, subject.id);
    assert_eq!(fetched.name, "Алгебра");
}

// ============================================================================
// TESTS FOR get_all
// ============================================================================

/// Test: get_all returns empty vec when no subjects exist.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_all_empty(pool: PgPool) {
    let repo = SubjectRepositoryPg::new(pool);

    let result = repo.get_all().await;

    assert_eq!(result.unwrap(), Vec::<Subject>::new());
}

/// Test: get_all returns subjects sorted alphabetically by name.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_all_sorted_by_name(pool: PgPool) {
    let repo = SubjectRepositoryPg::new(pool);

    let subject_fizika = create_test_subject("Физика");
    let subject_algebra = create_test_subject("Алгебра");
    let subject_informatika = create_test_subject("Информатика");

    // Insert in random order
    repo.save(subject_fizika.clone()).await.unwrap();
    repo.save(subject_informatika.clone()).await.unwrap();
    repo.save(subject_algebra.clone()).await.unwrap();

    let result = repo.get_all().await.unwrap();

    // Should return exactly 3 subjects, sorted alphabetically
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].name, "Алгебра");
    assert_eq!(result[1].name, "Информатика");
    assert_eq!(result[2].name, "Физика");
}

// ============================================================================
// TESTS FOR save (CREATE)
// ============================================================================

/// Test: save creates a new subject.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_creates_new_subject(pool: PgPool) {
    let repo = SubjectRepositoryPg::new(pool);
    let subject = create_test_subject("Химия");

    let result = repo.save(subject.clone()).await;

    assert_eq!(result.unwrap(), subject);

    let fetched = repo.get_by_id(subject.id).await.unwrap();
    assert_eq!(fetched.name, "Химия");
}

/// Test: save allows multiple subjects with different names.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_multiple_subjects(pool: PgPool) {
    let repo = SubjectRepositoryPg::new(pool);

    let subject_1 = create_test_subject("Математика");
    let subject_2 = create_test_subject("Литература");
    let subject_3 = create_test_subject("История");

    assert!(repo.save(subject_1).await.is_ok());
    assert!(repo.save(subject_2).await.is_ok());
    assert!(repo.save(subject_3).await.is_ok());

    let result = repo.get_all().await.unwrap();
    assert_eq!(result.len(), 3);
}

// ============================================================================
// TESTS FOR save (UPDATE / UPSERT)
// ============================================================================

/// Test: save updates an existing subject (upsert) when ID matches.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_updates_existing_subject(pool: PgPool) {
    let repo = SubjectRepositoryPg::new(pool);

    let original = Subject::try_new(Uuid::new_v4(), "Старое название".to_string())
        .expect("Valid initial data");
    repo.save(original.clone()).await.unwrap();

    // Modify the subject name but keep the same ID
    let updated = Subject::try_new(original.id, "Новое название".to_string())
        .expect("Valid updated data");

    let result = repo.save(updated.clone()).await;
    assert_eq!(result.unwrap(), updated);

    // Verify the update in the database
    let fetched = repo.get_by_id(original.id).await.unwrap();
    assert_eq!(fetched.name, "Новое название");
}

// ============================================================================
// TESTS FOR save (ERRORS)
// ============================================================================

/// Test: save raises SubjectAlreadyExists when name is duplicate.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_duplicate_name_raises_error(pool: PgPool) {
    let repo = SubjectRepositoryPg::new(pool);

    let subject_1 = Subject::try_new(Uuid::new_v4(), "Геометрия".to_string())
        .expect("Valid subject 1");
    let subject_2 = Subject::try_new(Uuid::new_v4(), "Геометрия".to_string()) // Different ID, same name
        .expect("Valid subject 2");

    repo.save(subject_1).await.unwrap();

    let result = repo.save(subject_2).await;

    assert!(matches!(result, Err(DomainError::SubjectAlreadyExists)));
}

/// Test: save allows same name if updating the same subject (upsert).
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_same_name_same_id_succeeds(pool: PgPool) {
    let repo = SubjectRepositoryPg::new(pool);

    let subject = Subject::try_new(Uuid::new_v4(), "Биология".to_string())
        .expect("Valid subject");
    repo.save(subject.clone()).await.unwrap();

    // Save again with same ID and same name (no-op update)
    let result = repo.save(subject.clone()).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap().name, "Биология");
}

// ============================================================================
// COMPLEX SCENARIO TESTS
// ============================================================================

/// Test: create, update, and fetch scenario.
#[sqlx::test(migrations = "../../migrations")]
async fn test_create_update_fetch_scenario(pool: PgPool) {
    let repo = SubjectRepositoryPg::new(pool);

    // Step 1: Create subject
    let subject = create_test_subject("Астрономия");
    repo.save(subject.clone()).await.unwrap();

    // Step 2: Fetch and verify
    let fetched = repo.get_by_id(subject.id).await.unwrap();
    assert_eq!(fetched, subject);

    // Step 3: Update subject name
    let updated = Subject::try_new(subject.id, "Астрофизика".to_string())
        .expect("Valid update");
    repo.save(updated.clone()).await.unwrap();

    // Step 4: Fetch again and verify update
    let fetched_again = repo.get_by_id(subject.id).await.unwrap();
    assert_eq!(fetched_again.name, "Астрофизика");

    // Step 5: Verify it appears in get_all
    let all_subjects = repo.get_all().await.unwrap();
    assert_eq!(all_subjects.len(), 1);
    assert_eq!(all_subjects[0].name, "Астрофизика");
}

/// Test: multiple subjects with Cyrillic names sort correctly.
#[sqlx::test(migrations = "../../migrations")]
async fn test_cyrillic_sorting(pool: PgPool) {
    let repo = SubjectRepositoryPg::new(pool);

    let subject_a = create_test_subject("Английский");
    let subject_ya = create_test_subject("Японский");
    let subject_m = create_test_subject("Математика");

    repo.save(subject_a.clone()).await.unwrap();
    repo.save(subject_ya.clone()).await.unwrap();
    repo.save(subject_m.clone()).await.unwrap();

    let result = repo.get_all().await.unwrap();

    assert_eq!(result.len(), 3);
    // PostgreSQL sorts Cyrillic by Unicode code points
    assert_eq!(result[0].name, "Английский");
    assert_eq!(result[1].name, "Математика");
    assert_eq!(result[2].name, "Японский");
}