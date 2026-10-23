//! Integration tests for `TimetableService` against PostgreSQL: courses, regular
//! slots (add / change / remove, with and without `apply_from`), one-off and moved
//! lessons.
//!
//! Run from `backend/` with `DATABASE_URL` set (role needs CREATEDB):
//!     cargo test -p application --test timetable

use std::sync::Arc;

use application::use_cases::admin::{SlotInput, TimetableChange, TimetableService};
use application::use_cases::schedule::ScheduleService;
use chrono::{NaiveDate, NaiveTime};
use domain::entities::user::User;
use domain::errors::DomainError;
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::{
    day_of_week::DayOfWeek, lesson_instance_status::LessonInstanceStatus,
    lesson_target::LessonTarget, week_parity::WeekParity,
};
use infrastructure::postgres::{
    CabinetRepositoryPg, ClassRepositoryPg, HomeworkRepositoryPg, LessonInstanceRepositoryPg,
    LessonRepositoryPg, LessonSubstitutionRepositoryPg, LessonTemplateRepositoryPg,
    ScheduleWeekRepositoryPg, StudentGroupRepositoryPg, SubjectRepositoryPg,
    TimetableRepositoryPg, UserRepositoryPg,
};
use sqlx::PgPool;
use uuid::Uuid;

const MONDAY: &str = "2026-10-19";
const TUESDAY: &str = "2026-10-20";
const NEXT_MONDAY: &str = "2026-10-26";

struct World {
    pool: PgPool,
    service: TimetableService,
    schedule: Arc<ScheduleService>,
    admin: User,
    teacher: User,
    student: User,
    class_id: Uuid,
    /// Algebra of 2027б, Monday 09:00–09:45, taught by `teacher`.
    algebra: Uuid,
    /// Physics of 2027б, no slots, no teachers.
    physics: Uuid,
    slot: Uuid,
    /// The algebra lesson in the week of `MONDAY`.
    instance: Uuid,
}

fn day(raw: &str) -> NaiveDate {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d").unwrap()
}

fn time(raw: &str) -> NaiveTime {
    NaiveTime::parse_from_str(raw, "%H:%M").unwrap()
}

async fn insert_user(pool: &PgPool, role: &str, class_id: Option<Uuid>) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (user_id, login, role, last_name, first_name, class_id)
         VALUES ($1, $2, $3::user_role, 'Тест', 'Тест', $4)",
    )
    .bind(id)
    .bind(format!("{role}.{}", id.simple()))
    .bind(role)
    .bind(class_id)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn insert_course(pool: &PgPool, class_id: Uuid, subject: &str) -> Uuid {
    let subject_id = Uuid::new_v4();
    sqlx::query("INSERT INTO subjects (subject_id, name) VALUES ($1, $2)")
        .bind(subject_id)
        .bind(subject)
        .execute(pool)
        .await
        .unwrap();
    let lesson = Uuid::new_v4();
    sqlx::query("INSERT INTO lessons (lesson_id, class_id, subject_id) VALUES ($1, $2, $3)")
        .bind(lesson)
        .bind(class_id)
        .bind(subject_id)
        .execute(pool)
        .await
        .unwrap();
    lesson
}

async fn add_homework(pool: &PgPool, instance: Uuid, author: Uuid) {
    sqlx::query(
        "INSERT INTO homeworks (lesson_instance_id, created_by, created_by_role, text_content, status)
         VALUES ($1, $2, 'teacher', '№ 1–5', 'published')",
    )
    .bind(instance)
    .bind(author)
    .execute(pool)
    .await
    .unwrap();
}

/// Class 2027б with algebra on Monday 09:00 in the published week of `MONDAY`
/// (one lesson), a physics course without slots, an admin, a teacher and a student.
async fn seed(pool: PgPool) -> World {
    let class_id = Uuid::new_v4();
    sqlx::query("INSERT INTO classes (class_id, graduation_year, letter) VALUES ($1, 2027, 'б')")
        .bind(class_id)
        .execute(&pool)
        .await
        .unwrap();
    let admin = insert_user(&pool, "admin", None).await;
    let teacher = insert_user(&pool, "teacher", None).await;
    let student = insert_user(&pool, "student", Some(class_id)).await;
    let algebra = insert_course(&pool, class_id, "Алгебра").await;
    let physics = insert_course(&pool, class_id, "Физика").await;
    sqlx::query("INSERT INTO lesson_teachers (lesson_id, teacher_id) VALUES ($1, $2)")
        .bind(algebra)
        .bind(teacher)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO schedule_weeks (week_start_date, status) VALUES ($1::date, 'published')",
    )
    .bind(MONDAY)
    .execute(&pool)
    .await
    .unwrap();
    let slot = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO lesson_templates (template_id, lesson_id, day, start_time, end_time)
         VALUES ($1, $2, 'mon', '09:00', '09:45')",
    )
    .bind(slot)
    .bind(algebra)
    .execute(&pool)
    .await
    .unwrap();
    let instance = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO lesson_instances (instance_id, template_id, week_start_date, lesson_date)
         VALUES ($1, $2, $3::date, $3::date)",
    )
    .bind(instance)
    .bind(slot)
    .bind(MONDAY)
    .execute(&pool)
    .await
    .unwrap();

    let lessons = Arc::new(LessonRepositoryPg::new(pool.clone()));
    let templates = Arc::new(LessonTemplateRepositoryPg::new(pool.clone()));
    let instances = Arc::new(LessonInstanceRepositoryPg::new(pool.clone()));
    let weeks = Arc::new(ScheduleWeekRepositoryPg::new(pool.clone()));
    let subjects = Arc::new(SubjectRepositoryPg::new(pool.clone()));
    let groups = Arc::new(StudentGroupRepositoryPg::new(pool.clone()));
    let users = Arc::new(UserRepositoryPg::new(pool.clone()));
    let cabinets = Arc::new(CabinetRepositoryPg::new(pool.clone()));
    let schedule = Arc::new(
        ScheduleService::new(
            weeks.clone(),
            groups.clone(),
            instances.clone(),
            templates.clone(),
            lessons.clone(),
            subjects.clone(),
            users.clone(),
            cabinets.clone(),
            Arc::new(HomeworkRepositoryPg::new(pool.clone())),
            Arc::new(ClassRepositoryPg::new(pool.clone())),
        )
        .with_substitutions(Arc::new(LessonSubstitutionRepositoryPg::new(pool.clone()))),
    );
    let service = TimetableService::new(
        lessons,
        templates,
        instances,
        weeks,
        subjects,
        groups,
        users.clone(),
        cabinets,
        Arc::new(TimetableRepositoryPg::new(pool.clone())),
        schedule.clone(),
    );
    World {
        service,
        schedule,
        admin: users.get_by_id(admin).await.unwrap(),
        teacher: users.get_by_id(teacher).await.unwrap(),
        student: users.get_by_id(student).await.unwrap(),
        pool,
        class_id,
        algebra,
        physics,
        slot,
        instance,
    }
}

fn slot(lesson_id: Uuid, day: DayOfWeek, start: &str, end: &str) -> SlotInput {
    SlotInput {
        lesson_id,
        day,
        start_time: time(start),
        end_time: time(end),
        parity: WeekParity::Every,
        cabinet_id: None,
    }
}

async fn student_lessons(w: &World, date: &str) -> Vec<(String, String, LessonInstanceStatus)> {
    w.schedule
        .day_schedule(w.student.id, day(date))
        .await
        .unwrap()
        .lessons
        .into_iter()
        .map(|l| {
            (
                l.subject.name,
                l.start_time.format("%H:%M").to_string(),
                l.instance.status,
            )
        })
        .collect()
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_admins_may_edit_the_timetable(pool: PgPool) {
    let w = seed(pool).await;

    let as_teacher = w.service.class_timetable(&w.teacher, w.class_id).await;
    let as_student = w
        .service
        .add_slot(&w.student, slot(w.physics, DayOfWeek::Tue, "10:00", "10:45"), None)
        .await;

    assert_eq!(as_teacher.unwrap_err(), DomainError::InsufficientPermissions);
    assert_eq!(as_student.unwrap_err(), DomainError::InsufficientPermissions);
}

#[sqlx::test(migrations = "../../migrations")]
async fn courses_get_teachers_and_are_unique_per_class(pool: PgPool) {
    let w = seed(pool).await;
    let physics_subject = sqlx::query_scalar::<_, Uuid>(
        "SELECT subject_id FROM lessons WHERE lesson_id = $1",
    )
    .bind(w.physics)
    .fetch_one(&w.pool)
    .await
    .unwrap();

    let course = w
        .service
        .set_course_teachers(&w.admin, w.physics, &[w.teacher.id])
        .await
        .unwrap();
    let duplicate = w
        .service
        .create_course(&w.admin, LessonTarget::Class(w.class_id), physics_subject, &[])
        .await;
    let student_as_teacher = w
        .service
        .set_course_teachers(&w.admin, w.physics, &[w.student.id])
        .await;
    let timetable = w.service.class_timetable(&w.admin, w.class_id).await.unwrap();

    assert_eq!(course.teachers.len(), 1);
    assert_eq!(course.target_name, "2027б");
    assert_eq!(duplicate.unwrap_err(), DomainError::LessonAlreadyExists);
    assert_eq!(student_as_teacher.unwrap_err(), DomainError::UserNotFound);
    assert_eq!(timetable.courses.len(), 2);
    assert_eq!(timetable.slots.len(), 1);
    assert_eq!(timetable.slots[0].template.id, w.slot);
}

#[sqlx::test(migrations = "../../migrations")]
async fn new_slot_reaches_existing_weeks_only_with_apply_from(pool: PgPool) {
    let w = seed(pool).await;

    let quiet = w
        .service
        .add_slot(&w.admin, slot(w.physics, DayOfWeek::Tue, "10:00", "10:45"), None)
        .await
        .unwrap();
    let before = student_lessons(&w, TUESDAY).await;
    let applied = w
        .service
        .add_slot(
            &w.admin,
            slot(w.physics, DayOfWeek::Wed, "10:00", "10:45"),
            Some(day(MONDAY)),
        )
        .await
        .unwrap();
    let again = w
        .service
        .add_slot(&w.admin, slot(w.physics, DayOfWeek::Wed, "10:00", "10:45"), None)
        .await;

    assert_eq!(quiet, TimetableChange::default());
    assert!(before.is_empty(), "without apply_from the week stays as it was");
    assert_eq!(applied.changed, 1);
    assert_eq!(student_lessons(&w, "2026-10-21").await.len(), 1);
    assert_eq!(again.unwrap_err(), DomainError::LessonTemplateAlreadyExists);
}

#[sqlx::test(migrations = "../../migrations")]
async fn changed_slot_moves_lessons_from_apply_from(pool: PgPool) {
    let w = seed(pool).await;

    let change = w
        .service
        .update_slot(
            &w.admin,
            w.slot,
            slot(w.algebra, DayOfWeek::Tue, "10:00", "10:45"),
            Some(day(MONDAY)),
        )
        .await
        .unwrap();
    let timetable = w.service.class_timetable(&w.admin, w.class_id).await.unwrap();

    assert_eq!(change, TimetableChange { changed: 1, kept: 0 });
    assert!(student_lessons(&w, MONDAY).await.is_empty());
    assert_eq!(
        student_lessons(&w, TUESDAY).await,
        vec![("Алгебра".to_string(), "10:00".to_string(), LessonInstanceStatus::Scheduled)]
    );
    assert_eq!(timetable.slots.len(), 1, "the old slot is no longer active");
    assert_eq!(timetable.slots[0].template.day, DayOfWeek::Tue);
}

#[sqlx::test(migrations = "../../migrations")]
async fn lesson_with_homework_never_changes_subject(pool: PgPool) {
    let w = seed(pool).await;
    add_homework(&w.pool, w.instance, w.teacher.id).await;

    let change = w
        .service
        .update_slot(
            &w.admin,
            w.slot,
            slot(w.physics, DayOfWeek::Mon, "09:00", "09:45"),
            Some(day(MONDAY)),
        )
        .await
        .unwrap();

    assert_eq!(change, TimetableChange { changed: 0, kept: 1 });
    assert_eq!(student_lessons(&w, MONDAY).await[0].0, "Алгебра");
}

#[sqlx::test(migrations = "../../migrations")]
async fn removed_slot_cancels_lessons_with_homework_and_deletes_the_rest(pool: PgPool) {
    let w = seed(pool.clone()).await;
    w.service
        .add_slot(
            &w.admin,
            slot(w.algebra, DayOfWeek::Tue, "09:00", "09:45"),
            Some(day(MONDAY)),
        )
        .await
        .unwrap();
    add_homework(&w.pool, w.instance, w.teacher.id).await;

    let monday_change = w
        .service
        .remove_slot(&w.admin, w.slot, Some(day(MONDAY)))
        .await
        .unwrap();
    let tuesday_slot = w.service.class_timetable(&w.admin, w.class_id).await.unwrap().slots[0]
        .template
        .id;
    let tuesday_change = w
        .service
        .remove_slot(&w.admin, tuesday_slot, Some(day(MONDAY)))
        .await
        .unwrap();

    assert_eq!(monday_change, TimetableChange { changed: 0, kept: 1 });
    assert_eq!(student_lessons(&w, MONDAY).await[0].2, LessonInstanceStatus::Cancelled);
    assert_eq!(tuesday_change, TimetableChange { changed: 1, kept: 0 });
    assert!(student_lessons(&w, TUESDAY).await.is_empty());
    assert!(w.service.class_timetable(&w.admin, w.class_id).await.unwrap().slots.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn one_off_lesson_stays_in_its_week(pool: PgPool) {
    let w = seed(pool).await;

    let lesson = w
        .service
        .add_one_off(&w.admin, w.physics, day(TUESDAY), time("12:00"), time("12:45"), None)
        .await
        .unwrap();
    let twice = w
        .service
        .add_one_off(&w.admin, w.physics, day(TUESDAY), time("12:00"), time("12:45"), None)
        .await;
    let sunday = w
        .service
        .add_one_off(&w.admin, w.physics, day("2026-10-25"), time("12:00"), time("12:45"), None)
        .await;
    let no_week = w
        .service
        .add_one_off(&w.admin, w.physics, day("2026-11-03"), time("12:00"), time("12:45"), None)
        .await;
    let copied = w.schedule.copy_week(day(MONDAY), day(NEXT_MONDAY)).await.unwrap();
    let timetable = w.service.class_timetable(&w.admin, w.class_id).await.unwrap();

    assert_eq!(lesson.subject.name, "Физика");
    assert_eq!(student_lessons(&w, TUESDAY).await.len(), 1);
    assert_eq!(twice.unwrap_err(), DomainError::LessonInstanceAlreadyExists);
    assert_eq!(sunday.unwrap_err(), DomainError::InvalidLessonInstanceDate);
    assert_eq!(no_week.unwrap_err(), DomainError::ScheduleWeekNotFound);
    assert_eq!(copied.len(), 1, "only the regular algebra lesson is copied");
    assert_eq!(timetable.slots.len(), 1, "a one-off lesson is not a regular slot");
}

#[sqlx::test(migrations = "../../migrations")]
async fn moved_lesson_takes_its_homework(pool: PgPool) {
    let w = seed(pool).await;
    add_homework(&w.pool, w.instance, w.teacher.id).await;

    let moved = w
        .service
        .move_lesson(&w.admin, w.instance, day(TUESDAY), time("14:00"), time("14:45"), None)
        .await
        .unwrap();
    let moved_back = w
        .service
        .move_lesson(&w.admin, moved.instance.id, day(MONDAY), time("09:00"), time("09:45"), None)
        .await
        .unwrap();

    assert_eq!(moved.instance.lesson_date, day(TUESDAY));
    assert!(moved.homework.is_some(), "homework moves with the lesson");
    // Moving back restores the cancelled original instead of adding a duplicate.
    assert_eq!(moved_back.instance.id, w.instance);
    assert!(moved_back.homework.is_some());
    assert_eq!(
        student_lessons(&w, TUESDAY).await[0].2,
        LessonInstanceStatus::Cancelled
    );
    assert_eq!(student_lessons(&w, MONDAY).await[0].2, LessonInstanceStatus::Scheduled);
}
