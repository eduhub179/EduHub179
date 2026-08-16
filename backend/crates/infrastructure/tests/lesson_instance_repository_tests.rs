//! Integration tests for `LessonInstanceRepositoryPg`.
//!
//! These tests verify the public API of the infrastructure crate
//! with a real PostgreSQL database using `sqlx::test` for automatic
//! transaction management and rollback.
//!
//! Seeding rule: `lesson_instances.week_start_date` has an FK to
//! `schedule_weeks`, so every test saves the week BEFORE its instances
//! (ScheduleWeekRepositoryPg::save → LessonInstanceRepositoryPg::save).
//!
//! Coverage:
//! - Catalog: `get_by_id`, `save` (create/update/errors).
//! - Queries: `get_by_week`, `get_by_date`, `get_by_template`.
//! - Dedup index: same (template_id, week_start_date) rejected.
//! - Fail-safe: FK violations mapped by constraint name (template / week / cabinet).
use chrono::{NaiveDate, NaiveTime};
use domain::entities::cabinet::Cabinet;
use domain::entities::class::Class;
use domain::entities::lesson::Lesson;
use domain::entities::lesson_instance::LessonInstance;
use domain::entities::lesson_template::LessonTemplate;
use domain::entities::schedule_week::ScheduleWeek;
use domain::entities::subject::Subject;
use domain::errors::DomainError;
use domain::repositories::cabinet_repository::CabinetRepository;
use domain::repositories::class_repository::ClassRepository;
use domain::repositories::lesson_instance_repository::LessonInstanceRepository;
use domain::repositories::lesson_repository::LessonRepository;
use domain::repositories::lesson_template_repository::LessonTemplateRepository;
use domain::repositories::schedule_week_repository::ScheduleWeekRepository;
use domain::repositories::subject_repository::SubjectRepository;
use domain::value_objects::class_letter::ClassLetter;
use domain::value_objects::day_of_week::DayOfWeek;
use domain::value_objects::lesson_instance_status::LessonInstanceStatus;
use domain::value_objects::lesson_target::LessonTarget;
use domain::value_objects::week_parity::WeekParity;
use domain::value_objects::week_status::WeekStatus;
use infrastructure::postgres::{
    CabinetRepositoryPg, ClassRepositoryPg, LessonInstanceRepositoryPg, LessonRepositoryPg,
    LessonTemplateRepositoryPg, ScheduleWeekRepositoryPg, SubjectRepositoryPg,
};
use sqlx::PgPool;
use std::sync::atomic::{AtomicI32, Ordering};
use uuid::Uuid;

/// Monotonic counter so multiple `setup_lesson` calls inside one test produce
/// distinct (graduation_year, class_letter) pairs (unique index on classes).
static CLASS_SEQ: AtomicI32 = AtomicI32::new(0);

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

/// Helper: date constructor.
fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).expect("valid date")
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

/// Helper: creates a schedule week entity (not persisted).
fn create_week(
    week_start_date: NaiveDate,
    status: WeekStatus,
    copied_from: Option<NaiveDate>,
) -> ScheduleWeek {
    ScheduleWeek::new(week_start_date, status, copied_from)
}

/// Helper: creates a ready-to-save instance with a random ID.
/// `lesson_date` must fall within [week_start_date, week_start_date + 7).
fn create_instance(
    template_id: Uuid,
    week_start_date: NaiveDate,
    lesson_date: NaiveDate,
    status: LessonInstanceStatus,
    cabinet_id: Option<Uuid>,
) -> LessonInstance {
    LessonInstance::try_new(
        Uuid::new_v4(),
        template_id,
        week_start_date,
        lesson_date,
        status,
        cabinet_id,
    )
    .expect("Test instance data should be valid")
}

/// Test environment: repositories for the full seed chain
/// class → subject → lesson → template → week → instance.
struct TestEnv {
    lesson_instance_repo: LessonInstanceRepositoryPg,
    week_repo: ScheduleWeekRepositoryPg,
    template_repo: LessonTemplateRepositoryPg,
    lesson_repo: LessonRepositoryPg,
    class_repo: ClassRepositoryPg,
    subject_repo: SubjectRepositoryPg,
    cabinet_repo: CabinetRepositoryPg,
}

impl TestEnv {
    fn new(pool: PgPool) -> Self {
        Self {
            lesson_instance_repo: LessonInstanceRepositoryPg::new(pool.clone()),
            week_repo: ScheduleWeekRepositoryPg::new(pool.clone()),
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
        let seq = CLASS_SEQ.fetch_add(1, Ordering::Relaxed);
        let class = create_test_class(2027 + seq, ClassLetter::B);
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

    /// Creates and persists a week, returns it.
    /// MUST run before any instance of this week is saved (FK).
    async fn setup_week(&self, week_start_date: NaiveDate, status: WeekStatus) -> ScheduleWeek {
        let week = create_week(week_start_date, status, None);
        self.week_repo.save(week.clone()).await.unwrap();
        week
    }

    /// Creates and persists an active template (every-parity, no cabinet).
    async fn setup_template(
        &self,
        lesson_id: Uuid,
        day: DayOfWeek,
        start: NaiveTime,
        end: NaiveTime,
    ) -> LessonTemplate {
        let template = create_template(lesson_id, day, start, end, WeekParity::Every, None);
        self.template_repo.save(template.clone()).await.unwrap();
        template
    }

    /// Persists a ready-to-save instance, returns it.
    async fn setup_instance(&self, instance: LessonInstance) -> LessonInstance {
        self.lesson_instance_repo
            .save(instance.clone())
            .await
            .unwrap();
        instance
    }
}

// ============================================================================
// TESTS: get_by_id
// ============================================================================

/// Test: get_by_id returns LessonInstanceNotFound for a non-existent ID.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_not_found(pool: PgPool) {
    let env = TestEnv::new(pool);
    let fake_id = Uuid::new_v4();

    let result = env.lesson_instance_repo.get_by_id(fake_id).await;

    assert!(matches!(
        result,
        Err(DomainError::LessonInstanceNotFound)
    ));
}

/// Test: get_by_id roundtrip — all fields survive save → fetch.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_and_get_by_id_roundtrip(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson = env.setup_lesson("Алгебра").await;
    let cabinet = env.setup_cabinet(412).await;
    let template = env
        .setup_template(lesson.id, DayOfWeek::Mon, t(9, 0), t(9, 45))
        .await;
    let week = env.setup_week(d(2026, 9, 7), WeekStatus::Published).await;
    let instance = create_instance(
        template.id,
        week.week_start_date,
        d(2026, 9, 7),
        LessonInstanceStatus::Scheduled,
        Some(cabinet.id),
    );
    env.setup_instance(instance.clone()).await;

    let fetched = env
        .lesson_instance_repo
        .get_by_id(instance.id)
        .await
        .expect("Get by ID should succeed");

    assert_eq!(fetched.id, instance.id);
    assert_eq!(fetched.template_id, template.id);
    assert_eq!(fetched.week_start_date, d(2026, 9, 7));
    assert_eq!(fetched.lesson_date, d(2026, 9, 7));
    assert_eq!(fetched.status, LessonInstanceStatus::Scheduled);
    assert_eq!(fetched.cabinet_id, Some(cabinet.id));
}

// ============================================================================
// TESTS: save (create / update / errors)
// ============================================================================

/// Test: an instance without a cabinet roundtrips as NULL.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_without_cabinet_roundtrip(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson = env.setup_lesson("Физика").await;
    let template = env
        .setup_template(lesson.id, DayOfWeek::Wed, t(10, 50), t(11, 35))
        .await;
    let week = env.setup_week(d(2026, 9, 7), WeekStatus::Published).await;
    let instance = create_instance(
        template.id,
        week.week_start_date,
        d(2026, 9, 9),
        LessonInstanceStatus::Scheduled,
        None,
    );
    env.setup_instance(instance.clone()).await;

    let fetched = env
        .lesson_instance_repo
        .get_by_id(instance.id)
        .await
        .unwrap();
    assert_eq!(fetched.cabinet_id, None);
    assert_eq!(fetched.status, LessonInstanceStatus::Scheduled);
}

/// Test: save with the same instance_id updates fields (upsert semantics):
/// cancel the instance and reassign the cabinet.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_updates_existing_instance(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson = env.setup_lesson("История").await;
    let cabinet = env.setup_cabinet(310).await;
    let template = env
        .setup_template(lesson.id, DayOfWeek::Tue, t(9, 0), t(9, 45))
        .await;
    let week = env.setup_week(d(2026, 9, 7), WeekStatus::Published).await;
    let instance = create_instance(
        template.id,
        week.week_start_date,
        d(2026, 9, 8),
        LessonInstanceStatus::Scheduled,
        None,
    );
    env.setup_instance(instance.clone()).await;

    let updated = LessonInstance::try_new(
        instance.id,
        instance.template_id,
        instance.week_start_date,
        instance.lesson_date,
        LessonInstanceStatus::Cancelled,
        Some(cabinet.id),
    )
    .expect("Test instance data should be valid");
    env.setup_instance(updated.clone()).await;

    let fetched = env
        .lesson_instance_repo
        .get_by_id(instance.id)
        .await
        .unwrap();
    assert_eq!(fetched.status, LessonInstanceStatus::Cancelled);
    assert_eq!(fetched.cabinet_id, Some(cabinet.id));
    // The identity (template / week / date) survives the update.
    assert_eq!(fetched.template_id, template.id);
    assert_eq!(fetched.week_start_date, d(2026, 9, 7));
    assert_eq!(fetched.lesson_date, d(2026, 9, 8));
}

/// Test: a NEW instance with the same (template_id, week_start_date)
/// as an existing one is rejected (unique index idx_lesson_instances_unique).
#[sqlx::test(migrations = "../../migrations")]
async fn test_duplicate_template_week_rejected(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson = env.setup_lesson("Литература").await;
    let template = env
        .setup_template(lesson.id, DayOfWeek::Mon, t(9, 0), t(9, 45))
        .await;
    let week = env.setup_week(d(2026, 9, 7), WeekStatus::Published).await;
    let first = create_instance(
        template.id,
        week.week_start_date,
        d(2026, 9, 7),
        LessonInstanceStatus::Scheduled,
        None,
    );
    env.setup_instance(first.clone()).await;

    let duplicate = create_instance(
        template.id,
        week.week_start_date,
        d(2026, 9, 8),
        LessonInstanceStatus::Scheduled,
        None,
    );
    let result = env.lesson_instance_repo.save(duplicate).await;

    assert!(matches!(
        result,
        Err(DomainError::LessonInstanceAlreadyExists)
    ));
}

/// Test: saving with a non-existent template_id maps to LessonTemplateNotFound (FK).
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_with_unknown_template_rejected(pool: PgPool) {
    let env = TestEnv::new(pool);
    // The week exists, so only the template FK can fire.
    let week = env.setup_week(d(2026, 9, 7), WeekStatus::Published).await;
    let instance = create_instance(
        Uuid::new_v4(),
        week.week_start_date,
        d(2026, 9, 7),
        LessonInstanceStatus::Scheduled,
        None,
    );

    let result = env.lesson_instance_repo.save(instance).await;

    assert!(matches!(result, Err(DomainError::LessonTemplateNotFound)));
}

/// Test: saving an instance for a week that was NOT created maps to
/// ScheduleWeekNotFound (FK lesson_instances_week_start_date_fkey).
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_with_unknown_week_rejected(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson = env.setup_lesson("Физика").await;
    let template = env
        .setup_template(lesson.id, DayOfWeek::Mon, t(9, 0), t(9, 45))
        .await;
    // Week 2026-09-21 is never created — the FK must reject the instance.
    let instance = create_instance(
        template.id,
        d(2026, 9, 21),
        d(2026, 9, 21),
        LessonInstanceStatus::Scheduled,
        None,
    );

    let result = env.lesson_instance_repo.save(instance).await;

    assert!(matches!(result, Err(DomainError::ScheduleWeekNotFound)));
}

/// Test: saving with a non-existent cabinet_id maps to CabinetNotFound (FK).
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_with_unknown_cabinet_rejected(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson = env.setup_lesson("Биология").await;
    let template = env
        .setup_template(lesson.id, DayOfWeek::Mon, t(9, 0), t(9, 45))
        .await;
    let week = env.setup_week(d(2026, 9, 7), WeekStatus::Published).await;
    let instance = create_instance(
        template.id,
        week.week_start_date,
        d(2026, 9, 7),
        LessonInstanceStatus::Scheduled,
        Some(Uuid::new_v4()),
    );

    let result = env.lesson_instance_repo.save(instance).await;

    assert!(matches!(result, Err(DomainError::CabinetNotFound)));
}

// ============================================================================
// TESTS: get_by_week
// ============================================================================

/// Test: get_by_week returns all instances of the week, ordered by lesson_date.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_week_returns_all_instances_ordered(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson_a = env.setup_lesson("Математика").await;
    let lesson_b = env.setup_lesson("География").await;
    let template_a = env
        .setup_template(lesson_a.id, DayOfWeek::Mon, t(9, 0), t(9, 45))
        .await;
    let template_b = env
        .setup_template(lesson_b.id, DayOfWeek::Mon, t(10, 50), t(11, 35))
        .await;
    let week = env.setup_week(d(2026, 9, 7), WeekStatus::Published).await;

    // Monday and Tuesday of the same week.
    let mon = create_instance(
        template_a.id,
        week.week_start_date,
        d(2026, 9, 7),
        LessonInstanceStatus::Scheduled,
        None,
    );
    let tue = create_instance(
        template_b.id,
        week.week_start_date,
        d(2026, 9, 8),
        LessonInstanceStatus::Scheduled,
        None,
    );
    env.setup_instance(mon.clone()).await;
    env.setup_instance(tue.clone()).await;

    let all = env
        .lesson_instance_repo
        .get_by_week(d(2026, 9, 7))
        .await
        .unwrap();

    assert_eq!(all.len(), 2);
    assert_eq!(all[0].id, mon.id, "ordered by lesson_date");
    assert_eq!(all[1].id, tue.id);
}

// ============================================================================
// TESTS: get_by_date
// ============================================================================

/// Test: get_by_date returns only instances on the queried date.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_date_returns_only_that_date(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson_a = env.setup_lesson("Русский").await;
    let lesson_b = env.setup_lesson("Химия").await;
    let template_a = env
        .setup_template(lesson_a.id, DayOfWeek::Mon, t(9, 0), t(9, 45))
        .await;
    let template_b = env
        .setup_template(lesson_b.id, DayOfWeek::Wed, t(9, 0), t(9, 45))
        .await;
    let week = env.setup_week(d(2026, 9, 7), WeekStatus::Published).await;

    // Monday and Wednesday of the same week.
    let mon = create_instance(
        template_a.id,
        week.week_start_date,
        d(2026, 9, 7),
        LessonInstanceStatus::Scheduled,
        None,
    );
    let wed = create_instance(
        template_b.id,
        week.week_start_date,
        d(2026, 9, 9),
        LessonInstanceStatus::Scheduled,
        None,
    );
    env.setup_instance(mon.clone()).await;
    env.setup_instance(wed.clone()).await;

    let on_wed = env
        .lesson_instance_repo
        .get_by_date(d(2026, 9, 9))
        .await
        .unwrap();
    assert_eq!(on_wed.len(), 1);
    assert_eq!(on_wed[0].id, wed.id);

    let on_mon = env
        .lesson_instance_repo
        .get_by_date(d(2026, 9, 7))
        .await
        .unwrap();
    assert_eq!(on_mon.len(), 1);
    assert_eq!(on_mon[0].id, mon.id);
}

// ============================================================================
// TESTS: get_by_template
// ============================================================================

/// Test: get_by_template returns all instances generated from the template,
/// ordered by week_start_date.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_template_returns_all(pool: PgPool) {
    let env = TestEnv::new(pool);
    let lesson = env.setup_lesson("Информатика").await;
    let template = env
        .setup_template(lesson.id, DayOfWeek::Mon, t(9, 0), t(9, 45))
        .await;
    // The same template fires in two consecutive weeks; each week must exist.
    let week_1 = env.setup_week(d(2026, 9, 7), WeekStatus::Published).await;
    let week_2 = env.setup_week(d(2026, 9, 14), WeekStatus::Published).await;

    let inst_1 = create_instance(
        template.id,
        week_1.week_start_date,
        d(2026, 9, 7),
        LessonInstanceStatus::Scheduled,
        None,
    );
    let inst_2 = create_instance(
        template.id,
        week_2.week_start_date,
        d(2026, 9, 14),
        LessonInstanceStatus::Scheduled,
        None,
    );
    env.setup_instance(inst_1.clone()).await;
    env.setup_instance(inst_2.clone()).await;

    let all = env
        .lesson_instance_repo
        .get_by_template(template.id)
        .await
        .unwrap();

    assert_eq!(all.len(), 2);
    assert_eq!(all[0].id, inst_1.id, "ordered by week_start_date");
    assert_eq!(all[1].id, inst_2.id);
}
