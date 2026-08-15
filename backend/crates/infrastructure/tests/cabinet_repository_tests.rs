//! Integration tests for `CabinetRepositoryPg`.
//!
//! Dependencies: Real PostgreSQL instance (via `sqlx::test`).
//! Guarantees:
//! - Tests run in isolated transactions (auto-rollback).
//! - Database schema is applied via migrations before tests.
//!
//! Coverage:
//! - Catalog: `get_by_id`, `get_by_number`, `get_all`, `get_by_floor`, `save`.
//! - Upsert semantics and unique-number violation.
//! - Domain invariant validation (pure unit tests, no DB).

use domain::entities::cabinet::{self, Cabinet};
use domain::errors::DomainError;
use domain::repositories::cabinet_repository::CabinetRepository;
use infrastructure::postgres::CabinetRepositoryPg;
use sqlx::PgPool;
use uuid::Uuid;

/// Helper: creates a test cabinet with a random ID and no optional fields.
fn create_test_cabinet(number: i32) -> Cabinet {
    Cabinet::try_new(Uuid::new_v4(), number, None, None)
        .expect("Test data should be valid and satisfy domain invariants")
}

// ============================================================================
// TESTS FOR get_by_id
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_not_found(pool: PgPool) {
    let repo = CabinetRepositoryPg::new(pool);
    let result = repo.get_by_id(Uuid::new_v4()).await;
    assert_eq!(result, Err(DomainError::CabinetNotFound));
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_success(pool: PgPool) {
    let repo = CabinetRepositoryPg::new(pool);
    let cabinet = Cabinet::try_new(
        Uuid::new_v4(),
        412,
        Some("Химическая лаборатория".to_string()),
        Some(30),
    )
    .expect("Valid cabinet");
    repo.save(cabinet.clone()).await.unwrap();

    let fetched = repo.get_by_id(cabinet.id).await.unwrap();
    assert_eq!(fetched, cabinet);
    assert_eq!(fetched.floor(), 4);
}

// ============================================================================
// TESTS FOR get_by_number
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_number_not_found(pool: PgPool) {
    let repo = CabinetRepositoryPg::new(pool);
    let result = repo.get_by_number(412).await;
    assert_eq!(result, Err(DomainError::CabinetNotFound));
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_number_success(pool: PgPool) {
    let repo = CabinetRepositoryPg::new(pool);
    let cabinet = create_test_cabinet(305);
    repo.save(cabinet.clone()).await.unwrap();

    let fetched = repo.get_by_number(305).await.unwrap();
    assert_eq!(fetched, cabinet);
}

// ============================================================================
// TESTS FOR get_all
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_all_empty(pool: PgPool) {
    let repo = CabinetRepositoryPg::new(pool);
    assert_eq!(repo.get_all().await.unwrap(), Vec::<Cabinet>::new());
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_all_sorted_by_number(pool: PgPool) {
    let repo = CabinetRepositoryPg::new(pool);
    repo.save(create_test_cabinet(412)).await.unwrap();
    repo.save(create_test_cabinet(305)).await.unwrap();
    repo.save(create_test_cabinet(101)).await.unwrap();

    let all = repo.get_all().await.unwrap();
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].number, 101);
    assert_eq!(all[1].number, 305);
    assert_eq!(all[2].number, 412);
}

// ============================================================================
// TESTS FOR get_by_floor
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_floor_empty(pool: PgPool) {
    let repo = CabinetRepositoryPg::new(pool);
    assert!(repo.get_by_floor(4).await.unwrap().is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_floor_filters_and_sorts(pool: PgPool) {
    let repo = CabinetRepositoryPg::new(pool);
    repo.save(create_test_cabinet(412)).await.unwrap();
    repo.save(create_test_cabinet(305)).await.unwrap();
    repo.save(create_test_cabinet(405)).await.unwrap();

    let floor_4 = repo.get_by_floor(4).await.unwrap();
    assert_eq!(floor_4.len(), 2);
    assert_eq!(floor_4[0].number, 405);
    assert_eq!(floor_4[1].number, 412);

    let floor_3 = repo.get_by_floor(3).await.unwrap();
    assert_eq!(floor_3.len(), 1);
    assert_eq!(floor_3[0].number, 305);
}

// ============================================================================
// TESTS FOR save (CREATE / UPDATE / ERRORS)
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_creates_new_cabinet(pool: PgPool) {
    let repo = CabinetRepositoryPg::new(pool);
    let cabinet = create_test_cabinet(201);

    let saved = repo.save(cabinet.clone()).await;
    assert_eq!(saved.unwrap(), cabinet);

    let fetched = repo.get_by_id(cabinet.id).await.unwrap();
    assert_eq!(fetched, cabinet);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_updates_existing_cabinet(pool: PgPool) {
    let repo = CabinetRepositoryPg::new(pool);
    let original = create_test_cabinet(412);
    repo.save(original.clone()).await.unwrap();

    // Change number, add description and capacity (same ID → upsert).
    let updated = Cabinet::try_new(
        original.id,
        413,
        Some("Компьютерный класс".to_string()),
        Some(25),
    )
    .expect("Valid update");
    repo.save(updated.clone()).await.unwrap();

    let fetched = repo.get_by_id(original.id).await.unwrap();
    assert_eq!(fetched, updated);
    // Floor follows the number (generated column recomputed by Postgres).
    assert_eq!(fetched.floor(), 4);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_duplicate_number_raises_error(pool: PgPool) {
    let repo = CabinetRepositoryPg::new(pool);
    let cabinet_1 = create_test_cabinet(412);
    let cabinet_2 = create_test_cabinet(412); // Different ID, same number
    repo.save(cabinet_1).await.unwrap();

    let result = repo.save(cabinet_2).await;
    assert_eq!(result, Err(DomainError::CabinetAlreadyExists));
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_same_number_same_id_succeeds(pool: PgPool) {
    let repo = CabinetRepositoryPg::new(pool);
    let cabinet = create_test_cabinet(412);
    repo.save(cabinet.clone()).await.unwrap();

    // Re-saving the same cabinet (upsert no-op) must not raise a conflict.
    let result = repo.save(cabinet.clone()).await;
    assert_eq!(result.unwrap(), cabinet);
}

// ============================================================================
// DOMAIN INVARIANT VALIDATION (pure unit tests, no DB)
// ============================================================================

#[test]
fn test_floor_is_derived_from_number() {
    assert_eq!(create_test_cabinet(412).floor(), 4);
    assert_eq!(create_test_cabinet(101).floor(), 1);
    assert_eq!(create_test_cabinet(999).floor(), 9);
}

#[test]
fn test_try_new_accepts_boundary_numbers() {
    assert!(Cabinet::try_new(Uuid::new_v4(), 100, None, None).is_ok());
    assert!(Cabinet::try_new(Uuid::new_v4(), 999, None, None).is_ok());
}

#[test]
fn test_try_new_rejects_out_of_range_numbers() {
    for number in [99, 1000, 0, -5] {
        let result = Cabinet::try_new(Uuid::new_v4(), number, None, None);
        assert!(matches!(result, Err(DomainError::InvalidCabinetNumber)));
    }
}

#[test]
fn test_try_new_rejects_non_positive_capacity() {
    for capacity in [0, -1] {
        let result = Cabinet::try_new(Uuid::new_v4(), 412, None, Some(capacity));
        assert!(matches!(result, Err(DomainError::InvalidCabinetCapacity)));
    }
}

#[test]
fn test_try_new_trims_description() {
    let cabinet = Cabinet::try_new(Uuid::new_v4(), 412, Some("  Лаб.  ".to_string()), None)
        .expect("Valid cabinet");
    assert_eq!(cabinet.description.as_deref(), Some("Лаб."));
}

#[test]
fn test_try_new_empty_whitespace_description() {
    let result = Cabinet::try_new(Uuid::new_v4(), 412, Some("   ".to_string()), None);
    assert!(result.is_ok());
    assert!(result.unwrap().description.is_none());
}

#[test]
fn test_try_new_rejects_overlong_description() {
    let result = Cabinet::try_new(Uuid::new_v4(), 412, Some("x".repeat(256)), None);
    assert!(matches!(
        result,
        Err(DomainError::InvalidCabinetDescription)
    ));
}

#[test]
fn test_try_new_accepts_max_length_description() {
    let result = Cabinet::try_new(Uuid::new_v4(), 412, Some("x".repeat(255)), None);
    assert!(result.is_ok());
}
