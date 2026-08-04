//! Integration tests for `ClassRepositoryPg`.
//!
//! Dependencies: Real PostgreSQL instance (via `sqlx::test`).
//! Guarantees:
//! - Tests run in isolated transactions (auto-rollback).
//! - Database schema is applied via migrations before tests.
//!
//! Setup:
//! - Requires `DATABASE_URL` environment variable.
//! - Uses `sqlx::test` macro for automatic test database management.

use domain::entities::class::Class;
use domain::value_objects::class_letter::ClassLetter;
use domain::errors::DomainError;
use domain::repositories::class_repository::ClassRepository;
use infrastructure::postgres::ClassRepositoryPg;
use sqlx::PgPool;
use uuid::Uuid;

/// Helper: creates a test class with random ID.
/// Panics if the domain invariants are violated (which shouldn't happen with valid test data).
fn create_test_class(year: i32, letter: ClassLetter) -> Class {
    Class::try_new(Uuid::new_v4(), year, letter, true)
        .expect("Test data should be valid and satisfy domain invariants")
}

/// Helper: creates an inactive test class.
fn create_inactive_test_class(year: i32, letter: ClassLetter) -> Class {
    Class::try_new(Uuid::new_v4(), year, letter, false)
        .expect("Test data should be valid and satisfy domain invariants")
}

// ============================================================================
// TESTS FOR get_by_id
// ============================================================================

/// Test: get_by_id returns ClassNotFound for non-existent ID.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_not_found(pool: PgPool) {
    // Arrange
    let repo = ClassRepositoryPg::new(pool);
    let non_existent_id = Uuid::new_v4();

    // Act
    let result = repo.get_by_id(non_existent_id).await;

    // Assert
    assert_eq!(result, Err(DomainError::ClassNotFound));
}

/// Test: get_by_id returns the correct class.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_success(pool: PgPool) {
    // Arrange
    let repo = ClassRepositoryPg::new(pool);
    let class = create_test_class(2025, ClassLetter::B);
    repo.save(class.clone()).await.unwrap();

    // Act
    let result = repo.get_by_id(class.id).await;

    // Assert
    assert_eq!(result.unwrap(), class);
}

/// Test: get_by_id returns inactive class (soft-delete scenario).
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_inactive_class(pool: PgPool) {
    // Arrange
    let repo = ClassRepositoryPg::new(pool);
    let inactive_class = create_inactive_test_class(2025, ClassLetter::B);
    repo.save(inactive_class.clone()).await.unwrap();

    // Act
    let result = repo.get_by_id(inactive_class.id).await;

    // Assert
    let fetched = result.unwrap();
    assert_eq!(fetched, inactive_class);
    assert_eq!(fetched.is_active, false);
}

// ============================================================================
// TESTS FOR get_active_by_year
// ============================================================================

/// Test: get_active_by_year returns empty vec when no classes exist.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_active_by_year_empty(pool: PgPool) {
    // Arrange
    let repo = ClassRepositoryPg::new(pool);

    // Act
    let result = repo.get_active_by_year(2025).await;

    // Assert
    assert_eq!(result.unwrap(), Vec::<Class>::new());
}

/// Test: get_active_by_year returns only active classes for the year.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_active_by_year_filters_inactive(pool: PgPool) {
    // Arrange
    let repo = ClassRepositoryPg::new(pool);

    let active_class = create_test_class(2025, ClassLetter::B);
    let inactive_class = create_inactive_test_class(2025, ClassLetter::V);
    let other_year_class = create_test_class(2024, ClassLetter::B);

    repo.save(active_class.clone()).await.unwrap();
    repo.save(inactive_class).await.unwrap();
    repo.save(other_year_class).await.unwrap();

    // Act
    let result = repo.get_active_by_year(2025).await.unwrap();

    // Assert
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], active_class);
}

/// Test: get_active_by_year returns classes sorted by letter.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_active_by_year_sorted_by_letter(pool: PgPool) {
    // Arrange
    let repo = ClassRepositoryPg::new(pool);

    let class_v = create_test_class(2025, ClassLetter::V);
    let class_b = create_test_class(2025, ClassLetter::B);
    let class_i = create_test_class(2025, ClassLetter::I);

    // Insert in random order
    repo.save(class_v.clone()).await.unwrap();
    repo.save(class_i.clone()).await.unwrap();
    repo.save(class_b.clone()).await.unwrap();

    // Act
    let result = repo.get_active_by_year(2025).await.unwrap();

    // Assert
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].letter, ClassLetter::B);
    assert_eq!(result[1].letter, ClassLetter::V);
    assert_eq!(result[2].letter, ClassLetter::I);
}

/// Test: get_active_by_year returns classes for different years independently.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_active_by_year_different_years(pool: PgPool) {
    // Arrange
    let repo = ClassRepositoryPg::new(pool);

    let class_2024 = create_test_class(2024, ClassLetter::B);
    let class_2025 = create_test_class(2025, ClassLetter::B);
    let class_2026 = create_test_class(2026, ClassLetter::B);

    repo.save(class_2024.clone()).await.unwrap();
    repo.save(class_2025.clone()).await.unwrap();
    repo.save(class_2026.clone()).await.unwrap();

    // Act & Assert
    let result_2024 = repo.get_active_by_year(2024).await.unwrap();
    let result_2025 = repo.get_active_by_year(2025).await.unwrap();
    let result_2026 = repo.get_active_by_year(2026).await.unwrap();

    assert_eq!(result_2024.len(), 1);
    assert_eq!(result_2025.len(), 1);
    assert_eq!(result_2026.len(), 1);

    assert_eq!(result_2024[0].graduation_year, 2024);
    assert_eq!(result_2025[0].graduation_year, 2025);
    assert_eq!(result_2026[0].graduation_year, 2026);
}

// ============================================================================
// TESTS FOR save (CREATE)
// ============================================================================

/// Test: save creates a new class.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_creates_new_class(pool: PgPool) {
    // Arrange
    let repo = ClassRepositoryPg::new(pool);
    let class = create_test_class(2025, ClassLetter::B);

    // Act
    let result = repo.save(class.clone()).await;

    // Assert
    assert_eq!(result.unwrap(), class);

    // Verify it's in the database
    let fetched = repo.get_by_id(class.id).await.unwrap();
    assert_eq!(fetched, class);
}

/// Test: save allows multiple classes with same year but different letters.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_same_year_different_letters(pool: PgPool) {
    // Arrange
    let repo = ClassRepositoryPg::new(pool);
    let class_b = create_test_class(2025, ClassLetter::B);
    let class_v = create_test_class(2025, ClassLetter::V);
    let class_i = create_test_class(2025, ClassLetter::I);

    // Act & Assert - all should succeed
    assert!(repo.save(class_b).await.is_ok());
    assert!(repo.save(class_v).await.is_ok());
    assert!(repo.save(class_i).await.is_ok());

    // Verify all three exist
    let result = repo.get_active_by_year(2025).await.unwrap();
    assert_eq!(result.len(), 3);
}

/// Test: save allows same letter with different years.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_same_letter_different_years(pool: PgPool) {
    // Arrange
    let repo = ClassRepositoryPg::new(pool);
    let class_2024 = create_test_class(2024, ClassLetter::B);
    let class_2025 = create_test_class(2025, ClassLetter::B);

    // Act & Assert
    assert!(repo.save(class_2024).await.is_ok());
    assert!(repo.save(class_2025).await.is_ok());
}

// ============================================================================
// TESTS FOR save (UPDATE / UPSERT)
// ============================================================================

/// Test: save updates an existing class (upsert).
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_updates_existing_class(pool: PgPool) {
    // Arrange
    let repo = ClassRepositoryPg::new(pool);
    let original = Class::try_new(Uuid::new_v4(), 2025, ClassLetter::B, true)
        .expect("Valid initial data");
    repo.save(original.clone()).await.unwrap();

    // Modify the class
    let updated = Class::try_new(original.id, 2026, ClassLetter::V, false)
        .expect("Valid updated data");

    // Act
    let result = repo.save(updated.clone()).await;

    // Assert
    assert_eq!(result.unwrap(), updated);

    // Verify the update
    let fetched = repo.get_by_id(original.id).await.unwrap();
    assert_eq!(fetched.graduation_year, 2026);
    assert_eq!(fetched.letter, ClassLetter::V);
    assert_eq!(fetched.is_active, false);
}

/// Test: save preserves class_id when updating.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_preserves_class_id_on_update(pool: PgPool) {
    // Arrange
    let repo = ClassRepositoryPg::new(pool);
    let class_id = Uuid::new_v4();
    let original = Class::try_new(class_id, 2025, ClassLetter::B, true)
        .expect("Valid initial data");
    repo.save(original).await.unwrap();

    // Act: update with same ID
    let updated = Class::try_new(class_id, 2026, ClassLetter::V, true)
        .expect("Valid updated data");
    repo.save(updated.clone()).await.unwrap();

    // Assert: same ID, different data
    let fetched = repo.get_by_id(class_id).await.unwrap();
    assert_eq!(fetched.id, class_id);
    assert_eq!(fetched.graduation_year, 2026);
    assert_eq!(fetched.letter, ClassLetter::V);
}

// ============================================================================
// TESTS FOR save (ERRORS)
// ============================================================================

/// Test: save raises ClassAlreadyExists when year+letter combination is duplicate.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_duplicate_year_letter_raises_error(pool: PgPool) {
    // Arrange
    let repo = ClassRepositoryPg::new(pool);
    let class1 = Class::try_new(Uuid::new_v4(), 2025, ClassLetter::B, true)
        .expect("Valid class 1");
    let class2 = Class::try_new(Uuid::new_v4(), 2025, ClassLetter::B, true)
        .expect("Valid class 2"); // Different ID, same year+letter

    repo.save(class1).await.unwrap();

    // Act
    let result = repo.save(class2).await;

    // Assert
    assert_eq!(result, Err(DomainError::ClassAlreadyExists));
}

/// Test: save raises ClassAlreadyExists even if first class is inactive.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_duplicate_inactive_class_raises_error(pool: PgPool) {
    // Arrange
    let repo = ClassRepositoryPg::new(pool);
    let inactive_class = Class::try_new(Uuid::new_v4(), 2025, ClassLetter::B, false)
        .expect("Valid inactive class");
    let new_class = Class::try_new(Uuid::new_v4(), 2025, ClassLetter::B, true)
        .expect("Valid new class");

    repo.save(inactive_class).await.unwrap();

    // Act
    let result = repo.save(new_class).await;

    // Assert
    assert_eq!(result, Err(DomainError::ClassAlreadyExists));
}

// ============================================================================
// COMPLEX SCENARIO TESTS
// ============================================================================

/// Test: multiple operations in sequence.
#[sqlx::test(migrations = "../../migrations")]
async fn test_multiple_operations(pool: PgPool) {
    // Arrange
    let repo = ClassRepositoryPg::new(pool);

    // Create multiple classes
    let class_10b = create_test_class(2025, ClassLetter::B);
    let class_10v = create_test_class(2025, ClassLetter::V);
    let class_11b = create_test_class(2024, ClassLetter::B);

    repo.save(class_10b.clone()).await.unwrap();
    repo.save(class_10v.clone()).await.unwrap();
    repo.save(class_11b.clone()).await.unwrap();

    // Act: fetch by year
    let classes_2025 = repo.get_active_by_year(2025).await.unwrap();
    let classes_2024 = repo.get_active_by_year(2024).await.unwrap();

    // Assert
    assert_eq!(classes_2025.len(), 2);
    assert_eq!(classes_2024.len(), 1);
    assert_eq!(classes_2024[0], class_11b);

    // Act: update one class (deactivate)
    let updated_10b = Class::try_new(class_10b.id, 2025, ClassLetter::B, false)
        .expect("Valid update");
    repo.save(updated_10b.clone()).await.unwrap();

    // Assert: inactive class should not appear in active list
    let active_2025 = repo.get_active_by_year(2025).await.unwrap();
    assert_eq!(active_2025.len(), 1);
    assert_eq!(active_2025[0], class_10v);

    // But should still be fetchable by ID
    let fetched = repo.get_by_id(class_10b.id).await.unwrap();
    assert_eq!(fetched.is_active, false);
}

/// Test: create, update, and fetch scenario.
#[sqlx::test(migrations = "../../migrations")]
async fn test_create_update_fetch_scenario(pool: PgPool) {
    // Arrange
    let repo = ClassRepositoryPg::new(pool);

    // Step 1: Create class
    let class = create_test_class(2025, ClassLetter::B);
    repo.save(class.clone()).await.unwrap();

    // Step 2: Fetch and verify
    let fetched = repo.get_by_id(class.id).await.unwrap();
    assert_eq!(fetched, class);

    // Step 3: Update class (change year)
    let updated = Class::try_new(class.id, 2026, ClassLetter::B, true)
        .expect("Valid update");
    repo.save(updated.clone()).await.unwrap();

    // Step 4: Fetch again and verify update
    let fetched_again = repo.get_by_id(class.id).await.unwrap();
    assert_eq!(fetched_again.graduation_year, 2026);
    assert_eq!(fetched_again.letter, ClassLetter::B);

    // Step 5: Verify old year doesn't return the class
    let old_year_classes = repo.get_active_by_year(2025).await.unwrap();
    assert_eq!(old_year_classes.len(), 0);

    // Step 6: Verify new year returns the class
    let new_year_classes = repo.get_active_by_year(2026).await.unwrap();
    assert_eq!(new_year_classes.len(), 1);
    assert_eq!(new_year_classes[0].id, class.id);
}

/// Test: all three letters for same year.
#[sqlx::test(migrations = "../../migrations")]
async fn test_all_three_letters_same_year(pool: PgPool) {
    // Arrange
    let repo = ClassRepositoryPg::new(pool);

    let class_b = create_test_class(2025, ClassLetter::B);
    let class_v = create_test_class(2025, ClassLetter::V);
    let class_i = create_test_class(2025, ClassLetter::I);

    // Act
    repo.save(class_b.clone()).await.unwrap();
    repo.save(class_v.clone()).await.unwrap();
    repo.save(class_i.clone()).await.unwrap();

    let result = repo.get_active_by_year(2025).await.unwrap();

    // Assert
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].letter, ClassLetter::B);
    assert_eq!(result[1].letter, ClassLetter::V);
    assert_eq!(result[2].letter, ClassLetter::I);
}