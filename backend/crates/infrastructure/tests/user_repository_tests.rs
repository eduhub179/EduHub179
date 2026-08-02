//! Integration tests for UserRepositoryPg.
//!
//! These tests verify the public API of the infrastructure crate
//! with a real PostgreSQL database using sqlx::test for automatic
//! transaction management and rollback.

use domain::entities::user::User;
use domain::errors::DomainError;
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::role::UserRole;
use infrastructure::postgres::UserRepositoryPg;
use sqlx::PgPool;
use uuid::Uuid;

/// Test: save a new user and fetch them by ID.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_and_get_by_id(pool: PgPool) {
    // Arrange
    let repo = UserRepositoryPg::new(pool);
    let test_email = "integration.test@example.com";

    let user = User::try_new(
        Uuid::new_v4(),
        test_email.to_string(),
        UserRole::Student,
        "Testov".to_string(),
        "Test".to_string(),
        None,
        None,
    )
        .expect("User creation should succeed");

    // Act
    let saved = repo.save(user.clone()).await.expect("Save should succeed");
    let fetched = repo.get_by_id(saved.id).await.expect("Get by ID should succeed");

    // Assert
    assert_eq!(fetched.email, test_email);
    assert_eq!(fetched.last_name, "Testov");
    assert_eq!(fetched.role, UserRole::Student);
}

/// Test: fetch a non-existent user returns UserNotFound.
#[sqlx::test(migrations = "../../migrations")]
async fn test_not_found(pool: PgPool) {
    // Arrange
    let repo = UserRepositoryPg::new(pool);
    let fake_id = Uuid::new_v4();

    // Act
    let result = repo.get_by_id(fake_id).await;

    // Assert
    assert!(matches!(result, Err(DomainError::UserNotFound)));
}

/// Test: fetch a user by email.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_email(pool: PgPool) {
    // Arrange
    let repo = UserRepositoryPg::new(pool);
    let test_email = "email.test@example.com";

    let user = User::try_new(
        Uuid::new_v4(),
        test_email.to_string(),
        UserRole::Teacher,
        "Ivanov".to_string(),
        "Ivan".to_string(),
        Some("Ivanovich".to_string()),
        None,
    )
        .expect("User creation should succeed");

    repo.save(user.clone()).await.expect("Save should succeed");

    // Act
    let fetched = repo.get_by_email(test_email).await.expect("Get by email should succeed");

    // Assert
    assert_eq!(fetched.id, user.id);
    assert_eq!(fetched.middle_name, Some("Ivanovich".to_string()));
}

/// Test: saving a user with a duplicate email raises EmailAlreadyExists.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_duplicate_email_raises_error(pool: PgPool) {
    // Arrange
    let repo = UserRepositoryPg::new(pool);
    let test_email = "duplicate.test@example.com";

    let user1 = User::try_new(
        Uuid::new_v4(),
        test_email.to_string(),
        UserRole::Student,
        "Petrov".to_string(),
        "Petr".to_string(),
        None,
        None,
    )
        .expect("User 1 creation should succeed");

    let user2 = User::try_new(
        Uuid::new_v4(), // Different ID
        test_email.to_string(), // Same email
        UserRole::Student,
        "Sidorov".to_string(),
        "Sidor".to_string(),
        None,
        None,
    )
        .expect("User 2 creation should succeed");

    // Act
    repo.save(user1).await.expect("First save should succeed");
    let result = repo.save(user2).await;

    // Assert
    assert!(matches!(result, Err(DomainError::EmailAlreadyExists)));
}