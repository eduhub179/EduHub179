//! Integration tests for LoginUseCase with real PostgreSQL.
//!
//! These tests verify the full authentication flow:
//! 1. Create user via UserRepositoryPg
//! 2. Set password via CredentialsStorePg
//! 3. Login via LoginUseCase
//!
//! Dependencies: Real PostgreSQL instance (via sqlx::test).
//! Guarantees:
//! - Tests run in isolated transactions (auto-rollback).
//! - Database schema is applied via migrations before tests.

use application::use_cases::auth::login::{LoginCommand, LoginUseCase};
use domain::entities::user::User;
use domain::errors::DomainError;
use domain::ports::auth::{CredentialsStore, TokenIssuer};
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::role::UserRole;
use infrastructure::auth::{Argon2PasswordHasher, JwtTokenIssuer};
use infrastructure::postgres::{CredentialsStorePg, UserRepositoryPg};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

const TEST_JWT_SECRET: &str = "test_secret_at_least_32_bytes_long_for_hs256!";
const TEST_JWT_TTL_SECONDS: i64 = 900;

// ============================================================================
// HELPERS
// ============================================================================

/// Helper: creates a test user with a random login.
fn create_test_user(login: &str) -> User {
    User::try_new(
        Uuid::new_v4(),
        login.to_string(),
        UserRole::Student,
        "Ivanov".to_string(),
        "Ivan".to_string(),
        None,
        None,
    )
    .expect("Test user should be valid")
}

/// Helper: creates a test teacher.
fn create_test_teacher(login: &str) -> User {
    User::try_new(
        Uuid::new_v4(),
        login.to_string(),
        UserRole::Teacher,
        "Petrov".to_string(),
        "Petr".to_string(),
        None,
        None,
    )
    .expect("Test teacher should be valid")
}

// ============================================================================
// TESTS
// ============================================================================

/// Test: successful login with correct credentials.
#[sqlx::test(migrations = "../../migrations")]
async fn test_login_success(pool: PgPool) {
    // Arrange
    let user_repo = Arc::new(UserRepositoryPg::new(pool.clone()));
    let hasher = Arc::new(Argon2PasswordHasher);
    let credentials = Arc::new(CredentialsStorePg::new(pool.clone(), hasher.clone()));
    let token_issuer = Arc::new(JwtTokenIssuer::new(
        TEST_JWT_SECRET.to_string(),
        TEST_JWT_TTL_SECONDS,
    ));

    let use_case = LoginUseCase::new(user_repo.clone(), credentials.clone(), token_issuer.clone());

    // Create user and set password
    let user = create_test_user("testuser1");
    user_repo
        .save(user.clone())
        .await
        .expect("Save user should succeed");
    credentials
        .set_password(user.id, "correct_password")
        .await
        .expect("Set password should succeed");

    // Act
    let cmd =
        LoginCommand::try_new("testuser1".to_string(), "correct_password".to_string()).unwrap();
    let result = use_case.execute(cmd).await;

    // Assert
    assert!(result.is_ok());
    let session = result.unwrap();
    assert_eq!(session.user.id, user.id);
    assert_eq!(session.user.login.as_str(), "testuser1");
    let claims = token_issuer
        .verify(&session.token)
        .expect("Token should be valid");
    assert_eq!(claims.user_id, user.id);
    assert_eq!(claims.role, UserRole::Student);
}

/// Test: login with wrong password returns InvalidCredentials.
#[sqlx::test(migrations = "../../migrations")]
async fn test_login_wrong_password(pool: PgPool) {
    let user_repo = Arc::new(UserRepositoryPg::new(pool.clone()));
    let hasher = Arc::new(Argon2PasswordHasher);
    let credentials = Arc::new(CredentialsStorePg::new(pool.clone(), hasher.clone()));
    let token_issuer = Arc::new(JwtTokenIssuer::new(
        TEST_JWT_SECRET.to_string(),
        TEST_JWT_TTL_SECONDS,
    ));

    let use_case = LoginUseCase::new(user_repo.clone(), credentials.clone(), token_issuer);

    let user = create_test_user("testuser2");
    user_repo
        .save(user.clone())
        .await
        .expect("Save user should succeed");
    credentials
        .set_password(user.id, "correct_password")
        .await
        .expect("Set password should succeed");

    let cmd = LoginCommand::try_new("testuser2".to_string(), "wrong_password".to_string()).unwrap();
    let result = use_case.execute(cmd).await;

    assert!(matches!(result, Err(DomainError::InvalidCredentials)));
}

/// Test: login with non-existent user returns InvalidCredentials.
#[sqlx::test(migrations = "../../migrations")]
async fn test_login_non_existent_user(pool: PgPool) {
    let user_repo = Arc::new(UserRepositoryPg::new(pool.clone()));
    let hasher = Arc::new(Argon2PasswordHasher);
    let credentials = Arc::new(CredentialsStorePg::new(pool.clone(), hasher.clone()));
    let token_issuer = Arc::new(JwtTokenIssuer::new(
        TEST_JWT_SECRET.to_string(),
        TEST_JWT_TTL_SECONDS,
    ));

    let use_case = LoginUseCase::new(user_repo.clone(), credentials.clone(), token_issuer);

    let cmd = LoginCommand::try_new("nonexistent".to_string(), "password".to_string()).unwrap();
    let result = use_case.execute(cmd).await;

    assert!(matches!(result, Err(DomainError::InvalidCredentials)));
}

/// Test: login with inactive user returns UserIsInactive.
#[sqlx::test(migrations = "../../migrations")]
async fn test_login_inactive_user(pool: PgPool) {
    let user_repo = Arc::new(UserRepositoryPg::new(pool.clone()));
    let hasher = Arc::new(Argon2PasswordHasher);
    let credentials = Arc::new(CredentialsStorePg::new(pool.clone(), hasher.clone()));
    let token_issuer = Arc::new(JwtTokenIssuer::new(
        TEST_JWT_SECRET.to_string(),
        TEST_JWT_TTL_SECONDS,
    ));

    let use_case = LoginUseCase::new(user_repo.clone(), credentials.clone(), token_issuer);

    let mut user = create_test_user("testuser3");
    user.is_active = false;
    user_repo
        .save(user.clone())
        .await
        .expect("Save user should succeed");
    credentials
        .set_password(user.id, "password")
        .await
        .expect("Set password should succeed");

    let cmd = LoginCommand::try_new("testuser3".to_string(), "password".to_string()).unwrap();
    let result = use_case.execute(cmd).await;

    assert!(matches!(result, Err(DomainError::UserIsInactive)));
}

/// Test: login with user who has no password set returns InvalidCredentials.
#[sqlx::test(migrations = "../../migrations")]
async fn test_login_no_password_set(pool: PgPool) {
    let user_repo = Arc::new(UserRepositoryPg::new(pool.clone()));
    let hasher = Arc::new(Argon2PasswordHasher);
    let credentials = Arc::new(CredentialsStorePg::new(pool.clone(), hasher.clone()));
    let token_issuer = Arc::new(JwtTokenIssuer::new(
        TEST_JWT_SECRET.to_string(),
        TEST_JWT_TTL_SECONDS,
    ));

    let use_case = LoginUseCase::new(user_repo.clone(), credentials.clone(), token_issuer);

    let user = create_test_user("testuser4");
    user_repo
        .save(user.clone())
        .await
        .expect("Save user should succeed");
    // Don't set password!

    let cmd = LoginCommand::try_new("testuser4".to_string(), "password".to_string()).unwrap();
    let result = use_case.execute(cmd).await;

    assert!(matches!(result, Err(DomainError::InvalidCredentials)));
}

/// Test: login is case-insensitive for login field.
#[sqlx::test(migrations = "../../migrations")]
async fn test_login_case_insensitive(pool: PgPool) {
    let user_repo = Arc::new(UserRepositoryPg::new(pool.clone()));
    let hasher = Arc::new(Argon2PasswordHasher);
    let credentials = Arc::new(CredentialsStorePg::new(pool.clone(), hasher.clone()));
    let token_issuer = Arc::new(JwtTokenIssuer::new(
        TEST_JWT_SECRET.to_string(),
        TEST_JWT_TTL_SECONDS,
    ));

    let use_case = LoginUseCase::new(user_repo.clone(), credentials.clone(), token_issuer);

    // Create user with lowercase login
    let user = create_test_user("testuser5");
    user_repo
        .save(user.clone())
        .await
        .expect("Save user should succeed");
    credentials
        .set_password(user.id, "password")
        .await
        .expect("Set password should succeed");

    // Login with uppercase - should work because Login::try_new lowercases
    let cmd = LoginCommand::try_new("TESTUSER5".to_string(), "password".to_string()).unwrap();
    let result = use_case.execute(cmd).await;

    // Should succeed - the login is normalized to lowercase
    assert!(result.is_ok());
}

/// Test: teacher can login (different role).
#[sqlx::test(migrations = "../../migrations")]
async fn test_login_teacher(pool: PgPool) {
    let user_repo = Arc::new(UserRepositoryPg::new(pool.clone()));
    let hasher = Arc::new(Argon2PasswordHasher);
    let credentials = Arc::new(CredentialsStorePg::new(pool.clone(), hasher.clone()));
    let token_issuer = Arc::new(JwtTokenIssuer::new(
        TEST_JWT_SECRET.to_string(),
        TEST_JWT_TTL_SECONDS,
    ));

    let use_case = LoginUseCase::new(user_repo.clone(), credentials.clone(), token_issuer);

    let teacher = create_test_teacher("teacher1");
    user_repo
        .save(teacher.clone())
        .await
        .expect("Save teacher should succeed");
    credentials
        .set_password(teacher.id, "teacher_password")
        .await
        .expect("Set password should succeed");

    let cmd =
        LoginCommand::try_new("teacher1".to_string(), "teacher_password".to_string()).unwrap();
    let result = use_case.execute(cmd).await;

    assert!(result.is_ok());
    let session = result.unwrap();
    assert_eq!(session.user.role, UserRole::Teacher);
}

/// Test: multiple login attempts with same credentials succeed.
#[sqlx::test(migrations = "../../migrations")]
async fn test_login_multiple_attempts(pool: PgPool) {
    let user_repo = Arc::new(UserRepositoryPg::new(pool.clone()));
    let hasher = Arc::new(Argon2PasswordHasher);
    let credentials = Arc::new(CredentialsStorePg::new(pool.clone(), hasher.clone()));
    let token_issuer = Arc::new(JwtTokenIssuer::new(
        TEST_JWT_SECRET.to_string(),
        TEST_JWT_TTL_SECONDS,
    ));

    let use_case = LoginUseCase::new(user_repo.clone(), credentials.clone(), token_issuer);

    let user = create_test_user("testuser6");
    user_repo
        .save(user.clone())
        .await
        .expect("Save user should succeed");
    credentials
        .set_password(user.id, "password")
        .await
        .expect("Set password should succeed");

    // First login
    let cmd1 = LoginCommand::try_new("testuser6".to_string(), "password".to_string()).unwrap();
    let result1 = use_case.execute(cmd1).await;
    assert!(result1.is_ok());

    // Second login (should also succeed)
    let cmd2 = LoginCommand::try_new("testuser6".to_string(), "password".to_string()).unwrap();
    let result2 = use_case.execute(cmd2).await;
    assert!(result2.is_ok());
}
