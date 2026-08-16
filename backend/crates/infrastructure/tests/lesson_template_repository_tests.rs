//! Integration tests for `LessonTemplateRepositoryPg`.
//!
//! These tests verify the public API of the infrastructure crate
//! with a real PostgreSQL database using `sqlx::test` for automatic
//! transaction management and rollback.
//!
//! Coverage:
//! - Catalog: `get_by_id`, `save` (create/update/errors).
//! - Queries: `get_by_lesson`, `get_active_for_day`, `get_all_active`.
//! - Dedup index: same (lesson, day, time, parity) rejected; different parity/time allowed.
//! - Fail-safe: FK violations mapped by constraint name (lesson / cabinet).
//! - Soft-archive via `is_active` flag.
use chrono::NaiveTime;
use domain::entities::cabinet::Cabinet;
use domain::entities::class::Class;
use domain::entities::lesson::Lesson;
use domain::entities::lesson_template::LessonTemplate;
use domain::entities::subject::Subject;
use domain::errors::DomainError;
use domain::repositories::cabinet_repository::CabinetRepository;
use domain::repositories::class_repository::ClassRepository;
use domain::repositories::lesson_repository::LessonRepository;
use domain::repositories::lesson_template_repository::LessonTemplateRepository;
use domain::repositories::subject_repository::SubjectRepository;
use domain::value_objects::class_letter::ClassLetter;
use domain::value_objects::day_of_week::DayOfWeek;
use domain::value_objects::lesson_target::LessonTarget;
use domain::value_objects::week_parity::WeekParity;
use infrastructure::postgres::{
    CabinetRepositoryPg, ClassRepositoryPg, LessonRepositoryPg, LessonTemplateRepositoryPg,
    SubjectRepositoryPg,
};
use sqlx::PgPool;
use uuid::Uuid;

// ============================================================================
// HELPERS
// ============================================================================

/// Helper: creates a test class with a random ID.
fn create_test_class(year: i32, letter: ClassLetter) -> Class {
    Class::try_new(Uuid::new_v4(), year, letter, true).expect("Test class data should be valid")
}

/// Helper: creates a test subject with a random ID.
fn create_test_subject(name: &str) -> Subject {
    Subject::try_new(Uuid::new_v4(), name.to_string()).expect("Test subject data should be valid")
}

/// Helper: creates a class-targeted lesson with random IDs.
fn create_class_lesson(class_id: Uuid, subject_id: Uuid) -> Lesson {
    Lesson::new(
        Uuid::new_v4(),
        LessonTarget::Class(class_id),
        subject_id,
        true,
    )
}

/// Helper: creates a test cabinet with a random ID.
fn create_test_cabinet(number: i32) -> Cabinet {
    Cabinet::try_new(Uuid::new_v4(), number, None, None).expect("Test cabinet data should be valid")
}

/// Helper: time-of-day constructor.
fn t(h: u32, m: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(h, m, 0).expect("valid time")
}

/// Helper: creates a ready-to-save template with random IDs.
fn create_template(
    lesson_id: Uuid,
    day: DayOfWeek,
    start: NaiveTime,
    end: NaiveTime,
    parity: WeekParity,
    cabinet_id: Option<Uuid>,
) -> LessonTemplate {
    LessonTemplate::try_new(
        Uuid::new_v4(),
        lesson_id,
        day,
        start,
        end,
        parity,
        cabinet_id,
        true,
    )
    .expect("Test template data should be valid")
}

/// Test environment: repositories + a seeded lesson + (optionally) a cabinet.
struct TestEnv {
    template_repo: LessonTemplateRepositoryPg,
    lesson_repo: LessonRepositoryPg,
    class_repo: ClassRepositoryPg,
    subject_repo: SubjectRepositoryPg,
    cabinet_repo: CabinetRepositoryPg,
}

impl TestEnv {
    fn new(pool: PgPool) -> Self {
        Self {
            template_repo: LessonTemplateRepositoryPg::new(pool.clone()),
            lesson_repo: LessonRepositoryPg::new(pool.clone()),
            class_repo: ClassRepositoryPg::new(pool.clone()),
            subject_repo: SubjectRepositoryPg::new(pool.clone()),
            cabinet_repo: CabinetRepositoryPg::new(pool.clone()),
        }
    }

    /// Creates and persists a class + subject + lesson.
    /// Returns the ready-to-save lesson (already persisted).
    async fn setup_lesson(&self, subject_name: &str) -> Lesson {
        let class = create_test_class(2027, ClassLetter::B);
        let subject = create_test_subject(subject_name);
        self.class_repo.save(class.clone()).await.unwrap();
        self.subject_repo.save(subject.clone()).await.unwrap();
        let lesson = create_class_lesson(class.id, subject.id);
        self.lesson_repo.save(lesson.clone()).await.unwrap();
        lesson
    }

    /// Creates and persists a cabinet, returns it.
    async fn setup_cabinet(&self, number: i32) -> Cabinet {
        let cabinet = create_test_cabinet(number);
        self.cabinet_repo.save(cabinet.clone()).await.unwrap();
        cabinet
    }
}

// ============================================================================
// TESTS: get_by_id
// ============================================================================

/// Test: get_by_id returns LessonTemplateNotFound for a non-existent ID.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_not_found(pool: PgPool) {
    let env = TestEnv::new(pool);
    let fake_id = Uuid::new_v4();

    let result = env.template_repo.get_by_id(fake_id).await;

    assert!(matches!(
        result,
        Err(DomainError::LessonTemplateNotFound)
    ));
}

/// Test: get_by_id roundtrip — all fields survive save → fetch.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_roundtrip(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson = env.setup_lesson("Алгебра").await;
    let cabinet = env.setup_cabinet(412).await;
    let template = create_template(
        lesson.id,
        DayOfWeek::Mon,
        t(9, 0),
        t(9, 45),
        WeekParity::Every,
        Some(cabinet.id),
    );

    env.template_repo.save(template.clone()).await.unwrap();

    let fetched = env
        .template_repo
        .get_by_id(template.id)
        .await
        .expect("Get by ID should succeed");

    assert_eq!(fetched.id, template.id);
    assert_eq!(fetched.lesson_id, lesson.id);
    assert_eq!(fetched.day, DayOfWeek::Mon);
    assert_eq!(fetched.start_time, t(9, 0));
    assert_eq!(fetched.end_time, t(9, 45));
    assert_eq!(fetched.parity, WeekParity::Every);
    assert_eq!(fetched.cabinet_id, Some(cabinet.id));
    assert!(fetched.is_active);
}

// ============================================================================
// TESTS: save (create / update / errors)
// ============================================================================

/// Test: a template without a cabinet roundtrips as NULL.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_without_cabinet_roundtrip(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson = env.setup_lesson("Физика").await;
    let template = create_template(
        lesson.id,
        DayOfWeek::Wed,
        t(10, 50),
        t(11, 35),
        WeekParity::Odd,
        None,
    );

    env.template_repo.save(template.clone()).await.unwrap();

    let fetched = env.template_repo.get_by_id(template.id).await.unwrap();
    assert_eq!(fetched.cabinet_id, None);
    assert_eq!(fetched.parity, WeekParity::Odd);
}

/// Test: save with the same template_id updates fields (upsert semantics).
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_updates_existing_template(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson = env.setup_lesson("История").await;
    let template = create_template(
        lesson.id,
        DayOfWeek::Mon,
        t(9, 0),
        t(9, 45),
        WeekParity::Every,
        None,
    );
    env.template_repo.save(template.clone()).await.unwrap();

    let updated = LessonTemplate::try_new(
        template.id,
        lesson.id,
        DayOfWeek::Thu,
        t(12, 0),
        t(12, 45),
        template.parity,
        None,
        false,
    )
    .unwrap();
    env.template_repo.save(updated.clone()).await.unwrap();

    let fetched = env.template_repo.get_by_id(template.id).await.unwrap();
    assert_eq!(fetched.day, DayOfWeek::Thu);
    assert_eq!(fetched.start_time, t(12, 0));
    assert!(!fetched.is_active);
}

/// Test: a NEW template with the same (lesson, day, time, parity) is rejected.
#[sqlx::test(migrations = "../../migrations")]
async fn test_duplicate_slot_same_parity_rejected(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson = env.setup_lesson("Литература").await;
    let first = create_template(
        lesson.id,
        DayOfWeek::Mon,
        t(9, 0),
        t(9, 45),
        WeekParity::Every,
        None,
    );
    env.template_repo.save(first.clone()).await.unwrap();

    let duplicate = create_template(
        lesson.id,
        DayOfWeek::Mon,
        t(9, 0),
        t(9, 45),
        WeekParity::Every,
        None,
    );
    let result = env.template_repo.save(duplicate).await;

    assert!(matches!(
        result,
        Err(DomainError::LessonTemplateAlreadyExists)
    ));
}

/// Test: same slot with a different parity is allowed (dedup index includes parity).
#[sqlx::test(migrations = "../../migrations")]
async fn test_same_slot_different_parity_allowed(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson = env.setup_lesson("Физкультура").await;
    let odd = create_template(
        lesson.id,
        DayOfWeek::Fri,
        t(14, 0),
        t(14, 45),
        WeekParity::Odd,
        None,
    );
    env.template_repo.save(odd.clone()).await.unwrap();

    let even = create_template(
        lesson.id,
        DayOfWeek::Fri,
        t(14, 0),
        t(14, 45),
        WeekParity::Even,
        None,
    );
    env.template_repo.save(even.clone()).await.unwrap();

    let fetched_odd = env.template_repo.get_by_id(odd.id).await.unwrap();
    let fetched_even = env.template_repo.get_by_id(even.id).await.unwrap();
    assert_eq!(fetched_odd.parity, WeekParity::Odd);
    assert_eq!(fetched_even.parity, WeekParity::Even);
}

/// Test: same lesson at a different time is allowed.
#[sqlx::test(migrations = "../../migrations")]
async fn test_same_lesson_different_time_allowed(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson = env.setup_lesson("Химия").await;
    let morning = create_template(
        lesson.id,
        DayOfWeek::Tue,
        t(9, 0),
        t(9, 45),
        WeekParity::Every,
        None,
    );
    env.template_repo.save(morning.clone()).await.unwrap();

    let afternoon = create_template(
        lesson.id,
        DayOfWeek::Tue,
        t(13, 0),
        t(13, 45),
        WeekParity::Every,
        None,
    );
    env.template_repo.save(afternoon.clone()).await.unwrap();

    let all = env.template_repo.get_by_lesson(lesson.id).await.unwrap();
    assert_eq!(all.len(), 2);
}

/// Test: saving with a non-existent lesson_id maps to LessonNotFound (FK).
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_with_unknown_lesson_rejected(pool: PgPool) {
    let env = TestEnv::new(pool);
    let template = create_template(
        Uuid::new_v4(),
        DayOfWeek::Mon,
        t(9, 0),
        t(9, 45),
        WeekParity::Every,
        None,
    );

    let result = env.template_repo.save(template).await;

    assert!(matches!(result, Err(DomainError::LessonNotFound)));
}

/// Test: saving with a non-existent cabinet_id maps to CabinetNotFound (FK).
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_with_unknown_cabinet_rejected(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson = env.setup_lesson("Биология").await;
    let template = create_template(
        lesson.id,
        DayOfWeek::Mon,
        t(9, 0),
        t(9, 45),
        WeekParity::Every,
        Some(Uuid::new_v4()),
    );

    let result = env.template_repo.save(template).await;

    assert!(matches!(result, Err(DomainError::CabinetNotFound)));
}

// ============================================================================
// TESTS: get_by_lesson
// ============================================================================

/// Test: get_by_lesson returns all templates (active and archived) for the lesson.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_lesson_returns_active_and_archived(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson = env.setup_lesson("География").await;

    let active = create_template(
        lesson.id,
        DayOfWeek::Mon,
        t(9, 0),
        t(9, 45),
        WeekParity::Every,
        None,
    );
    let archived = LessonTemplate::try_new(
        Uuid::new_v4(),
        lesson.id,
        DayOfWeek::Wed,
        t(10, 0),
        t(10, 45),
        WeekParity::Every,
        None,
        false,
    )
    .unwrap();
    env.template_repo.save(active.clone()).await.unwrap();
    env.template_repo.save(archived.clone()).await.unwrap();

    let all = env.template_repo.get_by_lesson(lesson.id).await.unwrap();

    assert_eq!(all.len(), 2);
    let active_found = all.iter().any(|t| t.id == active.id && t.is_active);
    let archived_found = all.iter().any(|t| t.id == archived.id && !t.is_active);
    assert!(active_found && archived_found);
}

/// Test: get_by_lesson returns empty vec for a lesson without templates.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_lesson_empty(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson = env.setup_lesson("ОБЖ").await;

    let all = env.template_repo.get_by_lesson(lesson.id).await.unwrap();

    assert!(all.is_empty());
}

// ============================================================================
// TESTS: get_active_for_day / get_all_active
// ============================================================================

/// Test: get_active_for_day returns only active templates of that day, by time.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_active_for_day_filters_day_and_is_active(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson = env.setup_lesson("Математика").await;

    // Monday 9:00 (active) — should appear
    let mon_early = create_template(
        lesson.id,
        DayOfWeek::Mon,
        t(9, 0),
        t(9, 45),
        WeekParity::Every,
        None,
    );
    // Monday 13:00 (active) — should appear
    let mon_late = create_template(
        lesson.id,
        DayOfWeek::Mon,
        t(13, 0),
        t(13, 45),
        WeekParity::Every,
        None,
    );
    // Tuesday 9:00 (active) — should NOT appear
    let tue = create_template(
        lesson.id,
        DayOfWeek::Tue,
        t(9, 0),
        t(9, 45),
        WeekParity::Every,
        None,
    );
    // Monday 11:00 (archived) — should NOT appear
    let mon_archived = LessonTemplate::try_new(
        Uuid::new_v4(),
        lesson.id,
        DayOfWeek::Mon,
        t(11, 0),
        t(11, 45),
        WeekParity::Every,
        None,
        false,
    )
    .unwrap();
    for template in [&mon_early, &mon_late, &tue, &mon_archived] {
        env.template_repo.save(template.clone()).await.unwrap();
    }

    let monday = env
        .template_repo
        .get_active_for_day(DayOfWeek::Mon)
        .await
        .unwrap();

    assert_eq!(monday.len(), 2);
    assert_eq!(monday[0].id, mon_early.id, "ordered by start_time");
    assert_eq!(monday[1].id, mon_late.id);
}

/// Test: get_all_active returns only active templates, ordered by (day, start_time).
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_all_active_filters_is_active(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson = env.setup_lesson("Русский").await;

    let active = create_template(
        lesson.id,
        DayOfWeek::Mon,
        t(9, 0),
        t(9, 45),
        WeekParity::Every,
        None,
    );
    let archived = LessonTemplate::try_new(
        Uuid::new_v4(),
        lesson.id,
        DayOfWeek::Fri,
        t(9, 0),
        t(9, 45),
        WeekParity::Every,
        None,
        false,
    )
    .unwrap();
    env.template_repo.save(active.clone()).await.unwrap();
    env.template_repo.save(archived.clone()).await.unwrap();

    let all = env.template_repo.get_all_active().await.unwrap();

    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, active.id);
}

/// Test: archiving via save (is_active = false) removes the template
/// from active queries while keeping it readable by ID.
#[sqlx::test(migrations = "../../migrations")]
async fn test_archive_via_save(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson = env.setup_lesson("Информатика").await;
    let template = create_template(
        lesson.id,
        DayOfWeek::Thu,
        t(9, 0),
        t(9, 45),
        WeekParity::Every,
        None,
    );
    env.template_repo.save(template.clone()).await.unwrap();

    let archived = LessonTemplate::try_new(
        template.id,
        template.lesson_id,
        template.day,
        template.start_time,
        template.end_time,
        template.parity,
        template.cabinet_id,
        false,
    )
    .unwrap();
    env.template_repo.save(archived.clone()).await.unwrap();

    let day_templates = env
        .template_repo
        .get_active_for_day(DayOfWeek::Thu)
        .await
        .unwrap();
    assert!(day_templates.is_empty());

    let all_active = env.template_repo.get_all_active().await.unwrap();
    assert!(all_active.is_empty());

    // Still readable by ID (soft-archive pattern).
    let fetched = env.template_repo.get_by_id(template.id).await.unwrap();
    assert!(!fetched.is_active);
}
