//! Integration tests for UserRepositoryPg.
//!
//! These tests verify the public API of the infrastructure crate
//! with a real PostgreSQL database.

use domain::entities::user::User;
use domain::errors::DomainError;
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::role::UserRole;
use infrastructure::postgres::UserRepositoryPg;
use sqlx::PgPool;
use uuid::Uuid;

fn init() {
    dotenvy::dotenv().ok();
}
async fn get_test_pool() -> PgPool {
    init();
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

async fn cleanup_test_data(pool: &PgPool, email: &str) {
    init();
    let _ = sqlx::query("DELETE FROM users WHERE email = $1")
        .bind(email)
        .execute(pool)
        .await;
}

#[tokio::test]
async fn test_save_and_get_by_id() {
    init();
    let pool = get_test_pool().await;
    let repo = UserRepositoryPg::new(pool.clone());
    let test_email = "integration.test@example.com";

    cleanup_test_data(&pool, test_email).await;

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

    let saved = repo.save(user.clone()).await.expect("Save should succeed");
    assert_eq!(saved.email, user.email);

    let fetched = repo.get_by_id(user.id).await.expect("Get by ID should succeed");
    assert_eq!(fetched.email, test_email);
}

#[tokio::test]
async fn test_not_found() {
    init();
    let pool = get_test_pool().await;
    let repo = UserRepositoryPg::new(pool);

    let fake_id = Uuid::new_v4();
    let result = repo.get_by_id(fake_id).await;

    assert!(matches!(result, Err(DomainError::UserNotFound)));
}