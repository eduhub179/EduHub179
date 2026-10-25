//! Integration tests for the teacher schedule (`ScheduleService::teacher_*`) against
//! PostgreSQL: real repositories, real `lesson_teachers` links.
//!
//! Run from `backend/` with `DATABASE_URL` set (role needs CREATEDB):
//!     cargo test -p application --test teacher_schedule

use std::sync::Arc;

use application::use_cases::schedule::ScheduleService;
use chrono::NaiveDate;
use domain::errors::DomainError;
use infrastructure::postgres::{
    CabinetRepositoryPg, ClassRepositoryPg, HomeworkRepositoryPg, LessonInstanceRepositoryPg,
    LessonRepositoryPg, LessonTemplateRepositoryPg, ScheduleWeekRepositoryPg,
    StudentGroupRepositoryPg, SubjectRepositoryPg, UserRepositoryPg,
};
use sqlx::PgPool;
use uuid::Uuid;

const MONDAY: &str = "2026-10-19";

struct World {
    service: ScheduleService,
    teacher: Uuid,
    other_teacher: Uuid,
    student: Uuid,
}

fn day(raw: &str) -> NaiveDate {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d").unwrap()
}

async fn exec(pool: &PgPool, sql: &str, ids: &[Uuid]) {
    let mut query = sqlx::query(sql);
    for id in ids {
        query = query.bind(*id);
    }
    query.execute(pool).await.unwrap();
}

async fn insert_user(pool: &PgPool, role: &str, class_id: Option<Uuid>) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (user_id, login, role, last_name, first_name, class_id)
         VALUES ($1, $2, $3::user_role, 'Тест', 'Тест', $4)",
    )
    .bind(id)
    .bind(format!("{role}.{id}@example.com"))
    .bind(role)
    .bind(class_id)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// One lesson (template + instance) on `date` at `start`.
async fn insert_slot(pool: &PgPool, lesson_id: Uuid, weekday: &str, date: &str, start: &str, end: &str) {
    let template_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO lesson_templates (template_id, lesson_id, day, start_time, end_time)
         VALUES ($1, $2, $3::day_of_week, $4::time, $5::time)",
    )
    .bind(template_id)
    .bind(lesson_id)
    .bind(weekday)
    .bind(start)
    .bind(end)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO lesson_instances (instance_id, template_id, week_start_date, lesson_date)
         VALUES ($1, $2, $3::date, $4::date)",
    )
    .bind(Uuid::new_v4())
    .bind(template_id)
    .bind(MONDAY)
    .bind(date)
    .execute(pool)
    .await
    .unwrap();
}

/// The teacher teaches algebra to class 2027б (Mon, Wed) and a group special course (Mon);
/// another teacher has physics for 2027б on Monday.
async fn seed(pool: PgPool, week_status: &str) -> World {
    let class_id = Uuid::new_v4();
    exec(&pool, "INSERT INTO classes (class_id, graduation_year, letter) VALUES ($1, 2027, 'б')", &[class_id]).await;
    let (algebra, physics, special) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    exec(
        &pool,
        "INSERT INTO subjects (subject_id, name) VALUES ($1, 'Алгебра'), ($2, 'Физика'), ($3, 'Спецкурс')",
        &[algebra, physics, special],
    )
    .await;
    let group_id = Uuid::new_v4();
    exec(&pool, "INSERT INTO student_groups (group_id, name) VALUES ($1, 'Матанализ-проф')", &[group_id]).await;

    let teacher = insert_user(&pool, "teacher", None).await;
    let other_teacher = insert_user(&pool, "teacher", None).await;
    let student = insert_user(&pool, "student", Some(class_id)).await;

    let (algebra_lesson, physics_lesson, group_lesson) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    exec(&pool, "INSERT INTO lessons (lesson_id, class_id, subject_id) VALUES ($1, $2, $3)", &[algebra_lesson, class_id, algebra]).await;
    exec(&pool, "INSERT INTO lessons (lesson_id, class_id, subject_id) VALUES ($1, $2, $3)", &[physics_lesson, class_id, physics]).await;
    exec(&pool, "INSERT INTO lessons (lesson_id, group_id, subject_id) VALUES ($1, $2, $3)", &[group_lesson, group_id, special]).await;
    exec(&pool, "INSERT INTO lesson_teachers (lesson_id, teacher_id) VALUES ($1, $3), ($2, $3)", &[algebra_lesson, group_lesson, teacher]).await;
    exec(&pool, "INSERT INTO lesson_teachers (lesson_id, teacher_id) VALUES ($1, $2)", &[physics_lesson, other_teacher]).await;

    sqlx::query("INSERT INTO schedule_weeks (week_start_date, status) VALUES ($1::date, $2::week_status)")
        .bind(MONDAY)
        .bind(week_status)
        .execute(&pool)
        .await
        .unwrap();
    // Inserted out of time order: the schedule must sort by start time.
    insert_slot(&pool, group_lesson, "mon", "2026-10-19", "11:00", "11:45").await;
    insert_slot(&pool, algebra_lesson, "mon", "2026-10-19", "09:00", "09:45").await;
    insert_slot(&pool, physics_lesson, "mon", "2026-10-19", "09:55", "10:40").await;
    insert_slot(&pool, algebra_lesson, "wed", "2026-10-21", "09:55", "10:40").await;

    let service = ScheduleService::new(
        Arc::new(ScheduleWeekRepositoryPg::new(pool.clone())),
        Arc::new(StudentGroupRepositoryPg::new(pool.clone())),
        Arc::new(LessonInstanceRepositoryPg::new(pool.clone())),
        Arc::new(LessonTemplateRepositoryPg::new(pool.clone())),
        Arc::new(LessonRepositoryPg::new(pool.clone())),
        Arc::new(SubjectRepositoryPg::new(pool.clone())),
        Arc::new(UserRepositoryPg::new(pool.clone())),
        Arc::new(CabinetRepositoryPg::new(pool.clone())),
        Arc::new(HomeworkRepositoryPg::new(pool.clone())),
        Arc::new(ClassRepositoryPg::new(pool)),
    );
    World {
        service,
        teacher,
        other_teacher,
        student,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn teacher_week_contains_only_own_lessons_with_targets(pool: PgPool) {
    let w = seed(pool, "published").await;

    // Any day of the week works: Thursday resolves to the week of Monday 19th.
    let week = w.service.teacher_week_schedule(w.teacher, day("2026-10-22")).await.unwrap();

    assert_eq!(week.week_start_date, day(MONDAY));
    assert_eq!(week.days.len(), 7);
    let monday: Vec<_> = week.days[0]
        .lessons
        .iter()
        .map(|l| (l.subject.name.as_str(), l.target_name.as_str()))
        .collect();
    assert_eq!(monday, vec![("Алгебра", "2027б"), ("Спецкурс", "Матанализ-проф")]);
    assert_eq!(week.days[2].lessons.len(), 1, "Wednesday algebra");
    assert!(week.days[1].lessons.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn teacher_day_contains_only_that_day(pool: PgPool) {
    let w = seed(pool, "published").await;

    let monday = w.service.teacher_day_schedule(w.teacher, day("2026-10-19")).await.unwrap();
    let wednesday = w.service.teacher_day_schedule(w.teacher, day("2026-10-21")).await.unwrap();
    let other = w.service.teacher_day_schedule(w.other_teacher, day("2026-10-19")).await.unwrap();

    assert_eq!(monday.lessons.len(), 2);
    assert_eq!(monday.lessons[0].start_time.to_string(), "09:00:00");
    assert_eq!(wednesday.lessons.len(), 1);
    let other_subjects: Vec<_> = other.lessons.iter().map(|l| l.subject.name.as_str()).collect();
    assert_eq!(other_subjects, vec!["Физика"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn student_or_unknown_id_is_not_a_teacher(pool: PgPool) {
    let w = seed(pool, "published").await;

    let student = w.service.teacher_week_schedule(w.student, day(MONDAY)).await;
    let unknown = w.service.teacher_day_schedule(Uuid::new_v4(), day(MONDAY)).await;

    assert_eq!(student.unwrap_err(), DomainError::UserNotFound);
    assert_eq!(unknown.unwrap_err(), DomainError::UserNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn draft_week_is_hidden(pool: PgPool) {
    let w = seed(pool, "draft").await;

    let week = w.service.teacher_week_schedule(w.teacher, day(MONDAY)).await;
    let day_result = w.service.teacher_day_schedule(w.teacher, day(MONDAY)).await;

    assert_eq!(week.unwrap_err(), DomainError::ScheduleWeekNotFound);
    assert_eq!(day_result.unwrap_err(), DomainError::ScheduleWeekNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn student_schedule_also_gets_target_names(pool: PgPool) {
    let w = seed(pool, "published").await;

    let week = w.service.current_week_schedule(w.student, day(MONDAY)).await.unwrap();

    let targets: Vec<_> = week.days[0].lessons.iter().map(|l| l.target_name.as_str()).collect();
    assert_eq!(targets, vec!["2027б", "2027б"]);
}
