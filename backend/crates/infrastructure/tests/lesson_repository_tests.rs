//! Integration tests for `LessonRepositoryPg`.
//!
//! These tests verify the public API of the infrastructure crate
//! with a real PostgreSQL database using `sqlx::test` for automatic
//! transaction management and rollback.
//!
//! Coverage:
//! - Lesson catalog: `get_by_id`, `save` (create/update/errors).
//! - Queries: `get_by_class`, `get_by_group`, `get_by_teacher`.
//! - Teacher assignment: `assign_teacher`, `unassign_teacher`, `get_teacher_ids`.
//! - Idempotency of teacher assignment operations.
//! - Fail-safe behavior for non-existent lessons, classes, subjects, teachers.
//! - Soft-delete via `is_active` flag.
use domain::entities::class::Class;
use domain::entities::lesson::Lesson;
use domain::entities::student_group::StudentGroup;
use domain::entities::subject::Subject;
use domain::entities::user::User;
use domain::errors::DomainError;
use domain::repositories::class_repository::ClassRepository;
use domain::repositories::lesson_repository::LessonRepository;
use domain::repositories::student_group_repository::StudentGroupRepository;
use domain::repositories::subject_repository::SubjectRepository;
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::class_letter::ClassLetter;
use domain::value_objects::lesson_target::LessonTarget;
use domain::value_objects::role::UserRole;
use infrastructure::postgres::{
    ClassRepositoryPg, LessonRepositoryPg, StudentGroupRepositoryPg, SubjectRepositoryPg,
    UserRepositoryPg,
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

/// Helper: creates a test student group with a random ID.
fn create_test_group(name: &str) -> StudentGroup {
    StudentGroup::try_new(Uuid::new_v4(), name.to_string())
        .expect("Test group data should be valid")
}

/// Helper: creates a test teacher with a random ID and unique email.
fn create_test_teacher(last_name: &str) -> User {
    User::try_new(
        Uuid::new_v4(),
        format!("teacher.{}@example.com", Uuid::new_v4()),
        UserRole::Teacher,
        last_name.to_string(),
        "Test".to_string(),
        None,
        None,
    )
    .expect("Test teacher data should be valid")
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

/// Helper: creates a group-targeted lesson with random IDs.
fn create_group_lesson(group_id: Uuid, subject_id: Uuid) -> Lesson {
    Lesson::new(
        Uuid::new_v4(),
        LessonTarget::Group(group_id),
        subject_id,
        true,
    )
}

/// Helper: sets up a complete test environment with class, subject, and lesson repo.
#[allow(dead_code)]
struct TestEnv {
    pool: PgPool,
    lesson_repo: LessonRepositoryPg,
    class_repo: ClassRepositoryPg,
    subject_repo: SubjectRepositoryPg,
    group_repo: StudentGroupRepositoryPg,
    user_repo: UserRepositoryPg,
}

impl TestEnv {
    fn new(pool: PgPool) -> Self {
        Self {
            lesson_repo: LessonRepositoryPg::new(pool.clone()),
            class_repo: ClassRepositoryPg::new(pool.clone()),
            subject_repo: SubjectRepositoryPg::new(pool.clone()),
            group_repo: StudentGroupRepositoryPg::new(pool.clone()),
            user_repo: UserRepositoryPg::new(pool.clone()),
            pool,
        }
    }

    /// Creates and persists a class + subject, returns a ready-to-save class lesson.
    async fn setup_class_lesson(&self, subject_name: &str) -> (Class, Subject, Lesson) {
        let class = create_test_class(2027, ClassLetter::B);
        let subject = create_test_subject(subject_name);
        self.class_repo.save(class.clone()).await.unwrap();
        self.subject_repo.save(subject.clone()).await.unwrap();
        let lesson = create_class_lesson(class.id, subject.id);
        (class, subject, lesson)
    }

    /// Creates and persists a group + subject, returns a ready-to-save group lesson.
    async fn setup_group_lesson(&self, subject_name: &str) -> (StudentGroup, Subject, Lesson) {
        let group = create_test_group("Английский B1");
        let subject = create_test_subject(subject_name);
        self.group_repo.save(group.clone()).await.unwrap();
        self.subject_repo.save(subject.clone()).await.unwrap();
        let lesson = create_group_lesson(group.id, subject.id);
        (group, subject, lesson)
    }
}

// ============================================================================
// TESTS FOR get_by_id
// ============================================================================

/// Test: get_by_id returns LessonNotFound for non-existent ID.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_not_found(pool: PgPool) {
    let env = TestEnv::new(pool);
    let fake_id = Uuid::new_v4();

    let result = env.lesson_repo.get_by_id(fake_id).await;

    assert!(matches!(result, Err(DomainError::LessonNotFound)));
}

/// Test: get_by_id returns the correct class-targeted lesson.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_class_lesson(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (_class, _subject, lesson) = env.setup_class_lesson("Алгебра").await;
    env.lesson_repo.save(lesson.clone()).await.unwrap();

    let fetched = env
        .lesson_repo
        .get_by_id(lesson.id)
        .await
        .expect("Get by ID should succeed");

    assert_eq!(fetched.id, lesson.id);
    assert_eq!(fetched.target, LessonTarget::Class(_class.id));
    assert_eq!(fetched.subject_id, _subject.id);
    assert!(fetched.is_active);
}

/// Test: get_by_id returns the correct group-targeted lesson.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_group_lesson(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (_group, _subject, lesson) = env.setup_group_lesson("Английский").await;
    env.lesson_repo.save(lesson.clone()).await.unwrap();

    let fetched = env
        .lesson_repo
        .get_by_id(lesson.id)
        .await
        .expect("Get by ID should succeed");

    assert_eq!(fetched.id, lesson.id);
    assert_eq!(fetched.target, LessonTarget::Group(_group.id));
    assert_eq!(fetched.subject_id, _subject.id);
}

/// Test: get_by_id returns inactive lesson (soft-delete aware).
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_returns_inactive_lesson(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (_class, _subject, mut lesson) = env.setup_class_lesson("Физика").await;
    lesson.is_active = false;
    env.lesson_repo.save(lesson.clone()).await.unwrap();

    let fetched = env
        .lesson_repo
        .get_by_id(lesson.id)
        .await
        .expect("Should return inactive lesson");

    assert_eq!(fetched.is_active, false);
}

// ============================================================================
// TESTS FOR save (CREATE)
// ============================================================================

/// Test: save creates a new class-targeted lesson.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_creates_class_lesson(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (_class, _subject, lesson) = env.setup_class_lesson("Геометрия").await;

    let result = env.lesson_repo.save(lesson.clone()).await;

    assert!(result.is_ok());
    let fetched = env.lesson_repo.get_by_id(lesson.id).await.unwrap();
    assert_eq!(fetched, lesson);
}

/// Test: save creates a new group-targeted lesson.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_creates_group_lesson(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (_group, _subject, lesson) = env.setup_group_lesson("Информатика").await;

    let result = env.lesson_repo.save(lesson.clone()).await;

    assert!(result.is_ok());
    let fetched = env.lesson_repo.get_by_id(lesson.id).await.unwrap();
    assert_eq!(fetched, lesson);
}

/// Test: save allows multiple lessons for the same class with different subjects.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_multiple_lessons_same_class(pool: PgPool) {
    let env = TestEnv::new(pool);
    let class = create_test_class(2027, ClassLetter::B);
    env.class_repo.save(class.clone()).await.unwrap();

    let subj_algebra = create_test_subject("Алгебра");
    let subj_physics = create_test_subject("Физика");
    env.subject_repo.save(subj_algebra.clone()).await.unwrap();
    env.subject_repo.save(subj_physics.clone()).await.unwrap();

    let lesson_1 = create_class_lesson(class.id, subj_algebra.id);
    let lesson_2 = create_class_lesson(class.id, subj_physics.id);

    assert!(env.lesson_repo.save(lesson_1).await.is_ok());
    assert!(env.lesson_repo.save(lesson_2).await.is_ok());

    let lessons = env.lesson_repo.get_by_class(class.id).await.unwrap();
    assert_eq!(lessons.len(), 2);
}

// ============================================================================
// TESTS FOR save (UPDATE / UPSERT)
// ============================================================================

/// Test: save updates an existing lesson (upsert) when ID matches.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_updates_existing_lesson(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (_class, _subject_algebra, lesson) = env.setup_class_lesson("Алгебра").await;
    env.lesson_repo.save(lesson.clone()).await.unwrap();

    // Change subject to Physics (same class, different subject)
    let subject_physics = create_test_subject("Физика");
    env.subject_repo
        .save(subject_physics.clone())
        .await
        .unwrap();

    let updated = Lesson::new(
        lesson.id,
        LessonTarget::Class(_class.id),
        subject_physics.id,
        true,
    );
    let result = env.lesson_repo.save(updated.clone()).await;

    assert!(result.is_ok());
    let fetched = env.lesson_repo.get_by_id(lesson.id).await.unwrap();
    assert_eq!(fetched.subject_id, subject_physics.id);
}

/// Test: save can deactivate a lesson (soft-delete).
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_deactivates_lesson(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (_class, _subject, lesson) = env.setup_class_lesson("Химия").await;
    env.lesson_repo.save(lesson.clone()).await.unwrap();

    let deactivated = Lesson::new(lesson.id, lesson.target, lesson.subject_id, false);
    env.lesson_repo.save(deactivated).await.unwrap();

    let fetched = env.lesson_repo.get_by_id(lesson.id).await.unwrap();
    assert!(!fetched.is_active);
}

/// Test: save can change target from class to group.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_changes_target_from_class_to_group(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (class, subject, lesson) = env.setup_class_lesson("Литература").await;
    env.lesson_repo.save(lesson.clone()).await.unwrap();

    let group = create_test_group("Литература углублённая");
    env.group_repo.save(group.clone()).await.unwrap();

    let updated = Lesson::new(lesson.id, LessonTarget::Group(group.id), subject.id, true);
    env.lesson_repo.save(updated).await.unwrap();

    let fetched = env.lesson_repo.get_by_id(lesson.id).await.unwrap();
    assert_eq!(fetched.target, LessonTarget::Group(group.id));

    // Should no longer appear in class lessons
    let class_lessons = env.lesson_repo.get_by_class(class.id).await.unwrap();
    assert!(class_lessons.is_empty());
}

// ============================================================================
// TESTS FOR save (ERRORS)
// ============================================================================

/// Test: save raises LessonAlreadyExists for duplicate (class + subject).
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_duplicate_class_subject_raises_error(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (_class, _subject, lesson_1) = env.setup_class_lesson("Алгебра").await;
    env.lesson_repo.save(lesson_1.clone()).await.unwrap();

    // Same class + same subject, different lesson_id
    let lesson_2 = create_class_lesson(_class.id, _subject.id);
    let result = env.lesson_repo.save(lesson_2).await;

    assert!(matches!(result, Err(DomainError::LessonAlreadyExists)));
}

/// Test: save raises LessonAlreadyExists for duplicate (group + subject).
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_duplicate_group_subject_raises_error(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (_group, _subject, lesson_1) = env.setup_group_lesson("Английский").await;
    env.lesson_repo.save(lesson_1.clone()).await.unwrap();

    let lesson_2 = create_group_lesson(_group.id, _subject.id);
    let result = env.lesson_repo.save(lesson_2).await;

    assert!(matches!(result, Err(DomainError::LessonAlreadyExists)));
}

/// Test: save allows same subject for different classes (no conflict).
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_same_subject_different_classes_succeeds(pool: PgPool) {
    let env = TestEnv::new(pool);
    let class_b = create_test_class(2027, ClassLetter::B);
    let class_v = create_test_class(2027, ClassLetter::V);
    env.class_repo.save(class_b.clone()).await.unwrap();
    env.class_repo.save(class_v.clone()).await.unwrap();

    let subject = create_test_subject("Алгебра");
    env.subject_repo.save(subject.clone()).await.unwrap();

    let lesson_b = create_class_lesson(class_b.id, subject.id);
    let lesson_v = create_class_lesson(class_v.id, subject.id);

    assert!(env.lesson_repo.save(lesson_b).await.is_ok());
    assert!(env.lesson_repo.save(lesson_v).await.is_ok());
}

/// Test: save with non-existent class raises InvalidLessonReference.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_non_existent_class_raises_invalid_reference(pool: PgPool) {
    let env = TestEnv::new(pool);
    let subject = create_test_subject("Алгебра");
    env.subject_repo.save(subject.clone()).await.unwrap();

    let lesson = create_class_lesson(Uuid::new_v4(), subject.id);
    let result = env.lesson_repo.save(lesson).await;

    assert!(matches!(result, Err(DomainError::InvalidLessonReference)));
}

/// Test: save with non-existent subject raises InvalidLessonReference.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_non_existent_subject_raises_invalid_reference(pool: PgPool) {
    let env = TestEnv::new(pool);
    let class = create_test_class(2027, ClassLetter::B);
    env.class_repo.save(class.clone()).await.unwrap();

    let lesson = create_class_lesson(class.id, Uuid::new_v4());
    let result = env.lesson_repo.save(lesson).await;

    assert!(matches!(result, Err(DomainError::InvalidLessonReference)));
}

/// Test: save with non-existent group raises InvalidLessonReference.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_non_existent_group_raises_invalid_reference(pool: PgPool) {
    let env = TestEnv::new(pool);
    let subject = create_test_subject("Английский");
    env.subject_repo.save(subject.clone()).await.unwrap();

    let lesson = create_group_lesson(Uuid::new_v4(), subject.id);
    let result = env.lesson_repo.save(lesson).await;

    assert!(matches!(result, Err(DomainError::InvalidLessonReference)));
}

// ============================================================================
// TESTS FOR get_by_class
// ============================================================================

/// Test: get_by_class returns empty vec when no lessons exist for the class.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_class_empty(pool: PgPool) {
    let env = TestEnv::new(pool);
    let class = create_test_class(2027, ClassLetter::B);
    env.class_repo.save(class.clone()).await.unwrap();

    let result = env.lesson_repo.get_by_class(class.id).await.unwrap();

    assert!(result.is_empty());
}

/// Test: get_by_class returns only active lessons for the specified class.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_class_filters_inactive(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (class, _, active_lesson) = env.setup_class_lesson("Алгебра").await;
    env.lesson_repo.save(active_lesson.clone()).await.unwrap();

    // Create an inactive lesson for the same class
    let subject_physics = create_test_subject("Физика");
    env.subject_repo
        .save(subject_physics.clone())
        .await
        .unwrap();
    let inactive_lesson = Lesson::new(
        Uuid::new_v4(),
        LessonTarget::Class(class.id),
        subject_physics.id,
        false,
    );
    env.lesson_repo.save(inactive_lesson).await.unwrap();

    let result = env.lesson_repo.get_by_class(class.id).await.unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, active_lesson.id);
}

/// Test: get_by_class does not return lessons from other classes.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_class_does_not_return_other_classes(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (class_b, _, lesson_b) = env.setup_class_lesson("Алгебра").await;
    env.lesson_repo.save(lesson_b).await.unwrap();

    let class_v = create_test_class(2027, ClassLetter::V);
    env.class_repo.save(class_v.clone()).await.unwrap();
    let subject = create_test_subject("Физика");
    env.subject_repo.save(subject.clone()).await.unwrap();
    let lesson_v = create_class_lesson(class_v.id, subject.id);
    env.lesson_repo.save(lesson_v).await.unwrap();

    let result_b = env.lesson_repo.get_by_class(class_b.id).await.unwrap();
    let result_v = env.lesson_repo.get_by_class(class_v.id).await.unwrap();

    assert_eq!(result_b.len(), 1);
    assert_eq!(result_v.len(), 1);
    assert_ne!(result_b[0].id, result_v[0].id);
}

/// Test: get_by_class returns lessons sorted by subject name.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_class_sorted_by_subject_name(pool: PgPool) {
    let env = TestEnv::new(pool);
    let class = create_test_class(2027, ClassLetter::B);
    env.class_repo.save(class.clone()).await.unwrap();

    let subj_fizika = create_test_subject("Физика");
    let subj_algebra = create_test_subject("Алгебра");
    let subj_informatika = create_test_subject("Информатика");
    env.subject_repo.save(subj_fizika.clone()).await.unwrap();
    env.subject_repo.save(subj_algebra.clone()).await.unwrap();
    env.subject_repo
        .save(subj_informatika.clone())
        .await
        .unwrap();

    // Insert in random order
    env.lesson_repo
        .save(create_class_lesson(class.id, subj_fizika.id))
        .await
        .unwrap();
    env.lesson_repo
        .save(create_class_lesson(class.id, subj_informatika.id))
        .await
        .unwrap();
    env.lesson_repo
        .save(create_class_lesson(class.id, subj_algebra.id))
        .await
        .unwrap();

    let result = env.lesson_repo.get_by_class(class.id).await.unwrap();

    assert_eq!(result.len(), 3);
    assert_eq!(result[0].subject_id, subj_algebra.id);
    assert_eq!(result[1].subject_id, subj_informatika.id);
    assert_eq!(result[2].subject_id, subj_fizika.id);
}

// ============================================================================
// TESTS FOR get_by_group
// ============================================================================

/// Test: get_by_group returns empty vec when no lessons exist for the group.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_group_empty(pool: PgPool) {
    let env = TestEnv::new(pool);
    let group = create_test_group("Пустая группа");
    env.group_repo.save(group.clone()).await.unwrap();

    let result = env.lesson_repo.get_by_group(group.id).await.unwrap();

    assert!(result.is_empty());
}

/// Test: get_by_group returns only active lessons for the specified group.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_group_filters_inactive(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (group, _, active_lesson) = env.setup_group_lesson("Английский").await;
    env.lesson_repo.save(active_lesson.clone()).await.unwrap();

    let subject_fr = create_test_subject("Французский");
    env.subject_repo.save(subject_fr.clone()).await.unwrap();
    let inactive_lesson = Lesson::new(
        Uuid::new_v4(),
        LessonTarget::Group(group.id),
        subject_fr.id,
        false,
    );
    env.lesson_repo.save(inactive_lesson).await.unwrap();

    let result = env.lesson_repo.get_by_group(group.id).await.unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, active_lesson.id);
}

/// Test: get_by_group returns lessons sorted by subject name.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_group_sorted_by_subject_name(pool: PgPool) {
    let env = TestEnv::new(pool);
    let group = create_test_group("Языковая группа");
    env.group_repo.save(group.clone()).await.unwrap();

    let subj_en = create_test_subject("Английский");
    let subj_de = create_test_subject("Немецкий");
    env.subject_repo.save(subj_en.clone()).await.unwrap();
    env.subject_repo.save(subj_de.clone()).await.unwrap();

    env.lesson_repo
        .save(create_group_lesson(group.id, subj_en.id))
        .await
        .unwrap();
    env.lesson_repo
        .save(create_group_lesson(group.id, subj_de.id))
        .await
        .unwrap();

    let result = env.lesson_repo.get_by_group(group.id).await.unwrap();

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].subject_id, subj_en.id);
    assert_eq!(result[1].subject_id, subj_de.id);
}

// ============================================================================
// TESTS FOR assign_teacher
// ============================================================================

/// Test: assign_teacher adds a teacher to a lesson.
#[sqlx::test(migrations = "../../migrations")]
async fn test_assign_teacher_success(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (_class, _subject, lesson) = env.setup_class_lesson("Алгебра").await;
    env.lesson_repo.save(lesson.clone()).await.unwrap();

    let teacher = create_test_teacher("Иванов");
    env.user_repo.save(teacher.clone()).await.unwrap();

    let result = env.lesson_repo.assign_teacher(lesson.id, teacher.id).await;

    assert!(result.is_ok());
    let teacher_ids = env.lesson_repo.get_teacher_ids(lesson.id).await.unwrap();
    assert_eq!(teacher_ids, vec![teacher.id]);
}

/// Test: assign_teacher is idempotent — assigning the same teacher twice is a no-op.
#[sqlx::test(migrations = "../../migrations")]
async fn test_assign_teacher_idempotent(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (_class, _subject, lesson) = env.setup_class_lesson("Алгебра").await;
    env.lesson_repo.save(lesson.clone()).await.unwrap();

    let teacher = create_test_teacher("Петров");
    env.user_repo.save(teacher.clone()).await.unwrap();

    env.lesson_repo
        .assign_teacher(lesson.id, teacher.id)
        .await
        .unwrap();
    let result = env.lesson_repo.assign_teacher(lesson.id, teacher.id).await;

    assert!(result.is_ok());
    let teacher_ids = env.lesson_repo.get_teacher_ids(lesson.id).await.unwrap();
    assert_eq!(teacher_ids.len(), 1);
}

/// Test: assign_teacher to a non-existent lesson raises InvalidLessonReference.
#[sqlx::test(migrations = "../../migrations")]
async fn test_assign_teacher_non_existent_lesson(pool: PgPool) {
    let env = TestEnv::new(pool);
    let teacher = create_test_teacher("Сидоров");
    env.user_repo.save(teacher.clone()).await.unwrap();

    let fake_lesson_id = Uuid::new_v4();
    let result = env
        .lesson_repo
        .assign_teacher(fake_lesson_id, teacher.id)
        .await;

    assert!(matches!(result, Err(DomainError::InvalidLessonReference)));
}

/// Test: assign_teacher with a non-existent teacher raises InvalidLessonReference.
#[sqlx::test(migrations = "../../migrations")]
async fn test_assign_teacher_non_existent_teacher(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (_class, _subject, lesson) = env.setup_class_lesson("Алгебра").await;
    env.lesson_repo.save(lesson.clone()).await.unwrap();

    let fake_teacher_id = Uuid::new_v4();
    let result = env
        .lesson_repo
        .assign_teacher(lesson.id, fake_teacher_id)
        .await;

    assert!(matches!(result, Err(DomainError::InvalidLessonReference)));
}

/// Test: multiple teachers can be assigned to the same lesson.
#[sqlx::test(migrations = "../../migrations")]
async fn test_assign_multiple_teachers(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (_class, _subject, lesson) = env.setup_class_lesson("Спецмат").await;
    env.lesson_repo.save(lesson.clone()).await.unwrap();

    let teacher_1 = create_test_teacher("Иванов");
    let teacher_2 = create_test_teacher("Петров");
    let teacher_3 = create_test_teacher("Сидоров");
    env.user_repo.save(teacher_1.clone()).await.unwrap();
    env.user_repo.save(teacher_2.clone()).await.unwrap();
    env.user_repo.save(teacher_3.clone()).await.unwrap();

    env.lesson_repo
        .assign_teacher(lesson.id, teacher_1.id)
        .await
        .unwrap();
    env.lesson_repo
        .assign_teacher(lesson.id, teacher_2.id)
        .await
        .unwrap();
    env.lesson_repo
        .assign_teacher(lesson.id, teacher_3.id)
        .await
        .unwrap();

    let teacher_ids = env.lesson_repo.get_teacher_ids(lesson.id).await.unwrap();
    assert_eq!(teacher_ids.len(), 3);
    assert!(teacher_ids.contains(&teacher_1.id));
    assert!(teacher_ids.contains(&teacher_2.id));
    assert!(teacher_ids.contains(&teacher_3.id));
}

// ============================================================================
// TESTS FOR unassign_teacher
// ============================================================================

/// Test: unassign_teacher removes a teacher from a lesson.
#[sqlx::test(migrations = "../../migrations")]
async fn test_unassign_teacher_success(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (_class, _subject, lesson) = env.setup_class_lesson("Алгебра").await;
    env.lesson_repo.save(lesson.clone()).await.unwrap();

    let teacher = create_test_teacher("Иванов");
    env.user_repo.save(teacher.clone()).await.unwrap();
    env.lesson_repo
        .assign_teacher(lesson.id, teacher.id)
        .await
        .unwrap();

    let result = env
        .lesson_repo
        .unassign_teacher(lesson.id, teacher.id)
        .await;

    assert!(result.is_ok());
    let teacher_ids = env.lesson_repo.get_teacher_ids(lesson.id).await.unwrap();
    assert!(teacher_ids.is_empty());
}

/// Test: unassign_teacher is idempotent — removing a non-assigned teacher is a no-op.
#[sqlx::test(migrations = "../../migrations")]
async fn test_unassign_teacher_non_assigned_is_noop(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (_class, _subject, lesson) = env.setup_class_lesson("Алгебра").await;
    env.lesson_repo.save(lesson.clone()).await.unwrap();

    let teacher = create_test_teacher("Петров");
    env.user_repo.save(teacher.clone()).await.unwrap();

    // Teacher was never assigned
    let result = env
        .lesson_repo
        .unassign_teacher(lesson.id, teacher.id)
        .await;

    assert!(result.is_ok());
}

/// Test: unassign_teacher from a non-existent lesson returns LessonNotFound.
#[sqlx::test(migrations = "../../migrations")]
async fn test_unassign_teacher_non_existent_lesson(pool: PgPool) {
    let env = TestEnv::new(pool);
    let teacher = create_test_teacher("Сидоров");
    env.user_repo.save(teacher.clone()).await.unwrap();

    let fake_lesson_id = Uuid::new_v4();
    let result = env
        .lesson_repo
        .unassign_teacher(fake_lesson_id, teacher.id)
        .await;

    assert!(matches!(result, Err(DomainError::LessonNotFound)));
}

// ============================================================================
// TESTS FOR get_teacher_ids
// ============================================================================

/// Test: get_teacher_ids returns empty vec for a lesson with no teachers.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_teacher_ids_empty(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (_class, _subject, lesson) = env.setup_class_lesson("Алгебра").await;
    env.lesson_repo.save(lesson.clone()).await.unwrap();

    let result = env.lesson_repo.get_teacher_ids(lesson.id).await.unwrap();

    assert!(result.is_empty());
}

/// Test: get_teacher_ids returns empty vec for a non-existent lesson.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_teacher_ids_non_existent_lesson(pool: PgPool) {
    let env = TestEnv::new(pool);
    let fake_lesson_id = Uuid::new_v4();

    let result = env
        .lesson_repo
        .get_teacher_ids(fake_lesson_id)
        .await
        .unwrap();

    assert!(result.is_empty());
}

// ============================================================================
// TESTS FOR get_by_teacher
// ============================================================================

/// Test: get_by_teacher returns empty vec when teacher has no lessons.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_teacher_empty(pool: PgPool) {
    let env = TestEnv::new(pool);
    let teacher = create_test_teacher("Безуроков");
    env.user_repo.save(teacher.clone()).await.unwrap();

    let result = env.lesson_repo.get_by_teacher(teacher.id).await.unwrap();

    assert!(result.is_empty());
}

/// Test: get_by_teacher returns only active lessons.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_teacher_filters_inactive(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (_class, _, active_lesson) = env.setup_class_lesson("Алгебра").await;
    env.lesson_repo.save(active_lesson.clone()).await.unwrap();

    let subject_physics = create_test_subject("Физика");
    env.subject_repo
        .save(subject_physics.clone())
        .await
        .unwrap();
    let inactive_lesson = Lesson::new(
        Uuid::new_v4(),
        LessonTarget::Class(_class.id),
        subject_physics.id,
        false,
    );
    env.lesson_repo.save(inactive_lesson.clone()).await.unwrap();

    let teacher = create_test_teacher("Иванов");
    env.user_repo.save(teacher.clone()).await.unwrap();

    env.lesson_repo
        .assign_teacher(active_lesson.id, teacher.id)
        .await
        .unwrap();
    env.lesson_repo
        .assign_teacher(inactive_lesson.id, teacher.id)
        .await
        .unwrap();

    let result = env.lesson_repo.get_by_teacher(teacher.id).await.unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, active_lesson.id);
}

/// Test: get_by_teacher returns lessons sorted by subject name.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_teacher_sorted_by_subject_name(pool: PgPool) {
    let env = TestEnv::new(pool);

    let class_b = create_test_class(2027, ClassLetter::B);
    let class_v = create_test_class(2027, ClassLetter::V);
    env.class_repo.save(class_b.clone()).await.unwrap();
    env.class_repo.save(class_v.clone()).await.unwrap();

    let subj_fizika = create_test_subject("Физика");
    let subj_algebra = create_test_subject("Алгебра");
    env.subject_repo.save(subj_fizika.clone()).await.unwrap();
    env.subject_repo.save(subj_algebra.clone()).await.unwrap();

    let lesson_fizika = create_class_lesson(class_b.id, subj_fizika.id);
    let lesson_algebra = create_class_lesson(class_v.id, subj_algebra.id);
    env.lesson_repo.save(lesson_fizika.clone()).await.unwrap();
    env.lesson_repo.save(lesson_algebra.clone()).await.unwrap();

    let teacher = create_test_teacher("Многопредметов");
    env.user_repo.save(teacher.clone()).await.unwrap();

    env.lesson_repo
        .assign_teacher(lesson_fizika.id, teacher.id)
        .await
        .unwrap();
    env.lesson_repo
        .assign_teacher(lesson_algebra.id, teacher.id)
        .await
        .unwrap();

    let result = env.lesson_repo.get_by_teacher(teacher.id).await.unwrap();

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].subject_id, subj_algebra.id);
    assert_eq!(result[1].subject_id, subj_fizika.id);
}

// ============================================================================
// COMPLEX SCENARIO TESTS
// ============================================================================

/// Test: full lifecycle — create lesson, assign teachers, query, deactivate.
#[sqlx::test(migrations = "../../migrations")]
async fn test_full_lesson_lifecycle(pool: PgPool) {
    let env = TestEnv::new(pool);

    // Step 1: Create class + subject + lesson
    let (class, subject, lesson) = env.setup_class_lesson("Спецмат").await;
    env.lesson_repo.save(lesson.clone()).await.unwrap();

    // Step 2: Assign two teachers
    let teacher_1 = create_test_teacher("Иванов");
    let teacher_2 = create_test_teacher("Петров");
    env.user_repo.save(teacher_1.clone()).await.unwrap();
    env.user_repo.save(teacher_2.clone()).await.unwrap();

    env.lesson_repo
        .assign_teacher(lesson.id, teacher_1.id)
        .await
        .unwrap();
    env.lesson_repo
        .assign_teacher(lesson.id, teacher_2.id)
        .await
        .unwrap();

    // Step 3: Verify teachers
    let teacher_ids = env.lesson_repo.get_teacher_ids(lesson.id).await.unwrap();
    assert_eq!(teacher_ids.len(), 2);

    // Step 4: Verify both teachers see the lesson
    let lessons_t1 = env.lesson_repo.get_by_teacher(teacher_1.id).await.unwrap();
    let lessons_t2 = env.lesson_repo.get_by_teacher(teacher_2.id).await.unwrap();
    assert_eq!(lessons_t1.len(), 1);
    assert_eq!(lessons_t2.len(), 1);
    assert_eq!(lessons_t1[0].id, lesson.id);

    // Step 5: Verify class sees the lesson
    let class_lessons = env.lesson_repo.get_by_class(class.id).await.unwrap();
    assert_eq!(class_lessons.len(), 1);
    assert_eq!(class_lessons[0].subject_id, subject.id);

    // Step 6: Remove one teacher
    env.lesson_repo
        .unassign_teacher(lesson.id, teacher_1.id)
        .await
        .unwrap();

    let teacher_ids_after = env.lesson_repo.get_teacher_ids(lesson.id).await.unwrap();
    assert_eq!(teacher_ids_after.len(), 1);
    assert_eq!(teacher_ids_after[0], teacher_2.id);

    let lessons_t1_after = env.lesson_repo.get_by_teacher(teacher_1.id).await.unwrap();
    assert!(lessons_t1_after.is_empty());

    // Step 7: Deactivate the lesson
    let deactivated = Lesson::new(lesson.id, lesson.target, lesson.subject_id, false);
    env.lesson_repo.save(deactivated).await.unwrap();

    // Lesson is still fetchable by ID
    let fetched = env.lesson_repo.get_by_id(lesson.id).await.unwrap();
    assert!(!fetched.is_active);

    // But no longer appears in class or teacher queries
    let class_lessons_after = env.lesson_repo.get_by_class(class.id).await.unwrap();
    assert!(class_lessons_after.is_empty());

    let lessons_t2_after = env.lesson_repo.get_by_teacher(teacher_2.id).await.unwrap();
    assert!(lessons_t2_after.is_empty());
}

/// Test: renaming a subject does not break existing lessons.
#[sqlx::test(migrations = "../../migrations")]
async fn test_lesson_survives_subject_rename(pool: PgPool) {
    let env = TestEnv::new(pool);
    let (_class, subject, lesson) = env.setup_class_lesson("Старое название").await;
    env.lesson_repo.save(lesson.clone()).await.unwrap();

    // Rename the subject
    let renamed = Subject::try_new(subject.id, "Новое название".to_string()).expect("Valid rename");
    env.subject_repo.save(renamed).await.unwrap();

    // Lesson is still fetchable and references the same subject_id
    let fetched = env.lesson_repo.get_by_id(lesson.id).await.unwrap();
    assert_eq!(fetched.subject_id, subject.id);

    // And appears in class lessons
    let class_lessons = env.lesson_repo.get_by_class(_class.id).await.unwrap();
    assert_eq!(class_lessons.len(), 1);
}

/// Test: class lesson and group lesson with the same subject coexist.
#[sqlx::test(migrations = "../../migrations")]
async fn test_class_and_group_lessons_same_subject_coexist(pool: PgPool) {
    let env = TestEnv::new(pool);

    let (class, subject, class_lesson) = env.setup_class_lesson("Английский").await;
    env.lesson_repo.save(class_lesson.clone()).await.unwrap();

    let group = create_test_group("Английский B1");
    env.group_repo.save(group.clone()).await.unwrap();
    let group_lesson = create_group_lesson(group.id, subject.id);
    env.lesson_repo.save(group_lesson.clone()).await.unwrap();

    let class_lessons = env.lesson_repo.get_by_class(class.id).await.unwrap();
    let group_lessons = env.lesson_repo.get_by_group(group.id).await.unwrap();

    assert_eq!(class_lessons.len(), 1);
    assert_eq!(group_lessons.len(), 1);
    assert_ne!(class_lessons[0].id, group_lessons[0].id);
    assert_eq!(class_lessons[0].subject_id, group_lessons[0].subject_id);
}
