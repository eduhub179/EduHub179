//! Integration tests for `ScheduleWeekRepositoryPg`.
//!
//! These tests verify the public API of the infrastructure crate
//! with a real PostgreSQL database using `sqlx::test` for automatic
//! transaction management and rollback.
//!
//! Coverage:
//! - Catalog: `get_by_id`, `save` (create/update/errors).
//! - Query: `get_all` (most recent first — admin view).
//! - Provenance: `copied_from` roundtrip and FK validation.
//! - Lifecycle: draft → published via upsert on `week_start_date`.
use chrono::NaiveDate;
use domain::entities::schedule_week::ScheduleWeek;
use domain::errors::DomainError;
use domain::repositories::schedule_week_repository::ScheduleWeekRepository;
use domain::value_objects::week_status::WeekStatus;
use infrastructure::postgres::ScheduleWeekRepositoryPg;
use sqlx::PgPool;

// ============================================================================
// HELPERS
// ============================================================================

/// Helper: date constructor.
fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).expect("valid date")
}

/// Helper: creates a schedule week entity (not persisted).
fn create_week(
    week_start_date: NaiveDate,
    status: WeekStatus,
    copied_from: Option<NaiveDate>,
) -> ScheduleWeek {
    ScheduleWeek::new(week_start_date, status, copied_from)
}

/// Test environment: the week repository.
struct TestEnv {
    week_repo: ScheduleWeekRepositoryPg,
}

impl TestEnv {
    fn new(pool: PgPool) -> Self {
        Self {
            week_repo: ScheduleWeekRepositoryPg::new(pool),
        }
    }

    /// Creates and persists a week, returns it.
    async fn setup_week(
        &self,
        week_start_date: NaiveDate,
        status: WeekStatus,
        copied_from: Option<NaiveDate>,
    ) -> ScheduleWeek {
        let week = create_week(week_start_date, status, copied_from);
        self.week_repo.save(week.clone()).await.unwrap();
        week
    }
}

// ============================================================================
// TESTS: get_by_id
// ============================================================================

/// Test: get_by_id returns ScheduleWeekNotFound for a non-existent start date.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_not_found(pool: PgPool) {
    let env = TestEnv::new(pool);

    let result = env.week_repo.get_by_id(d(2026, 9, 7)).await;

    assert!(matches!(result, Err(DomainError::ScheduleWeekNotFound)));
}

/// Test: get_by_id roundtrip — a fresh draft week survives save → fetch.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_and_get_by_id_roundtrip(pool: PgPool) {
    let env = TestEnv::new(pool);
    let week = create_week(d(2026, 9, 7), WeekStatus::Draft, None);

    env.week_repo.save(week.clone()).await.unwrap();

    let fetched = env
        .week_repo
        .get_by_id(d(2026, 9, 7))
        .await
        .expect("Get by ID should succeed");

    assert_eq!(fetched.week_start_date, d(2026, 9, 7));
    assert_eq!(fetched.status, WeekStatus::Draft);
    assert!(fetched.is_draft());
    assert_eq!(fetched.copied_from, None);
}

// ============================================================================
// TESTS: save (create / update / errors)
// ============================================================================

/// Test: a week copied from another existing week keeps its provenance.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_with_copied_from_roundtrip(pool: PgPool) {
    let env = TestEnv::new(pool);
    // The source week must exist before the copy references it (FK).
    env.setup_week(d(2026, 9, 7), WeekStatus::Published, None).await;
    let copied = create_week(d(2026, 9, 14), WeekStatus::Draft, Some(d(2026, 9, 7)));

    env.week_repo.save(copied.clone()).await.unwrap();

    let fetched = env
        .week_repo
        .get_by_id(d(2026, 9, 14))
        .await
        .unwrap();
    assert_eq!(fetched.status, WeekStatus::Draft);
    assert_eq!(fetched.copied_from, Some(d(2026, 9, 7)));

    // The source week is untouched.
    let source = env.week_repo.get_by_id(d(2026, 9, 7)).await.unwrap();
    assert_eq!(source.status, WeekStatus::Published);
    assert_eq!(source.copied_from, None);
}

/// Test: save with the same week_start_date updates fields (upsert semantics):
/// publishing a draft is done by saving the updated entity.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_updates_week(pool: PgPool) {
    let env = TestEnv::new(pool);
    let week = create_week(d(2026, 9, 7), WeekStatus::Draft, None);
    env.week_repo.save(week.clone()).await.unwrap();

    let published = create_week(d(2026, 9, 7), WeekStatus::Published, None);
    env.week_repo.save(published.clone()).await.unwrap();

    let fetched = env
        .week_repo
        .get_by_id(d(2026, 9, 7))
        .await
        .unwrap();
    assert!(fetched.is_published());
    assert_eq!(fetched.status, WeekStatus::Published);
    assert_eq!(fetched.copied_from, None);
}

/// Test: get_all returns all weeks, most recent first (admin view).
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_all_returns_most_recent_first(pool: PgPool) {
    let env = TestEnv::new(pool);
    env.setup_week(d(2026, 9, 7), WeekStatus::Draft, None).await;
    env.setup_week(d(2026, 9, 14), WeekStatus::Published, None).await;

    let all = env.week_repo.get_all().await.unwrap();

    assert_eq!(all.len(), 2);
    assert_eq!(all[0].week_start_date, d(2026, 9, 14), "most recent first");
    assert_eq!(all[0].status, WeekStatus::Published);
    assert_eq!(all[1].week_start_date, d(2026, 9, 7));
    assert_eq!(all[1].status, WeekStatus::Draft);
}

/// Test: saving with copied_from pointing at a missing week maps to
/// ScheduleWeekNotFound (FK schedule_weeks_copied_from_fkey).
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_with_unknown_copied_from_rejected(pool: PgPool) {
    let env = TestEnv::new(pool);
    // Week 2026-09-21 was never created.
    let week = create_week(d(2026, 9, 14), WeekStatus::Draft, Some(d(2026, 9, 21)));

    let result = env.week_repo.save(week).await;

    assert!(matches!(result, Err(DomainError::ScheduleWeekNotFound)));
}
