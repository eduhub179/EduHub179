//! Integration tests for `ScheduleService::next_course_lessons` ("задать ДЗ на следующий
//! урок") against PostgreSQL.
//!
//! Run from `backend/` with `DATABASE_URL` set (role needs CREATEDB):
//!     cargo test -p application --test next_lesson

use std::sync::Arc;

use application::use_cases::schedule::ScheduleService;
use infrastructure::postgres::{
    CabinetRepositoryPg, ClassRepositoryPg, HomeworkRepositoryPg, LessonInstanceRepositoryPg,
    LessonRepositoryPg, LessonTemplateRepositoryPg, ScheduleWeekRepositoryPg,
    StudentGroupRepositoryPg, SubjectRepositoryPg, UserRepositoryPg,
};
use sqlx::PgPool;
use uuid::Uuid;

/// Week 1 published, week 2 draft, week 3 published.
const WEEK1: &str = "2026-10-19";
const WEEK2: &str = "2026-10-26";
const WEEK3: &str = "2026-11-02";

struct World {
    service: ScheduleService,
    pool: PgPool,
    class_id: Uuid,
    /// Algebra: double on Monday (09:00 + 09:55), single on Thursday (11:00).
    algebra_mon_1: [Uuid; 3],
    algebra_mon_2: [Uuid; 3],
    algebra_thu: [Uuid; 3],
}

async fn exec(pool: &PgPool, sql: &str, ids: &[Uuid]) {
    let mut query = sqlx::query(sql);
    for id in ids {
        query = query.bind(*id);
    }
    query.execute(pool).await.unwrap();
}

async fn template(pool: &PgPool, lesson_id: Uuid, weekday: &str, start: &str, end: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO lesson_templates (template_id, lesson_id, day, start_time, end_time)
         VALUES ($1, $2, $3::day_of_week, $4::time, $5::time)",
    )
    .bind(id)
    .bind(lesson_id)
    .bind(weekday)
    .bind(start)
    .bind(end)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// Instances of `template` in the three weeks; `offset` = days after Monday.
async fn instances(pool: &PgPool, template_id: Uuid, offset: i32) -> [Uuid; 3] {
    let mut ids = [Uuid::nil(); 3];
    for (index, week) in [WEEK1, WEEK2, WEEK3].iter().enumerate() {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO lesson_instances (instance_id, template_id, week_start_date, lesson_date)
             VALUES ($1, $2, $3::date, $3::date + $4)",
        )
        .bind(id)
        .bind(template_id)
        .bind(week)
        .bind(offset)
        .execute(pool)
        .await
        .unwrap();
        ids[index] = id;
    }
    ids
}

async fn seed(pool: PgPool) -> World {
    let class_id = Uuid::new_v4();
    exec(&pool, "INSERT INTO classes (class_id, graduation_year, letter) VALUES ($1, 2027, 'б')", &[class_id]).await;
    let (algebra, physics) = (Uuid::new_v4(), Uuid::new_v4());
    exec(&pool, "INSERT INTO subjects (subject_id, name) VALUES ($1, 'Алгебра'), ($2, 'Физика')", &[algebra, physics]).await;
    let (algebra_lesson, physics_lesson) = (Uuid::new_v4(), Uuid::new_v4());
    exec(&pool, "INSERT INTO lessons (lesson_id, class_id, subject_id) VALUES ($1, $2, $3)", &[algebra_lesson, class_id, algebra]).await;
    exec(&pool, "INSERT INTO lessons (lesson_id, class_id, subject_id) VALUES ($1, $2, $3)", &[physics_lesson, class_id, physics]).await;

    for (week, status) in [(WEEK1, "published"), (WEEK2, "draft"), (WEEK3, "published")] {
        sqlx::query("INSERT INTO schedule_weeks (week_start_date, status) VALUES ($1::date, $2::week_status)")
            .bind(week)
            .bind(status)
            .execute(&pool)
            .await
            .unwrap();
    }

    // Monday: physics 08:00, algebra 09:00 + 09:55 (a double); Thursday: algebra 11:00.
    let physics_mon = template(&pool, physics_lesson, "mon", "08:00", "08:45").await;
    let mon_1 = template(&pool, algebra_lesson, "mon", "09:00", "09:45").await;
    let mon_2 = template(&pool, algebra_lesson, "mon", "09:55", "10:40").await;
    let thu = template(&pool, algebra_lesson, "thu", "11:00", "11:45").await;
    instances(&pool, physics_mon, 0).await;
    let algebra_mon_1 = instances(&pool, mon_1, 0).await;
    let algebra_mon_2 = instances(&pool, mon_2, 0).await;
    let algebra_thu = instances(&pool, thu, 3).await;

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
        Arc::new(ClassRepositoryPg::new(pool.clone())),
    );
    World { service, pool, class_id, algebra_mon_1, algebra_mon_2, algebra_thu }
}

fn ids(lessons: &[application::use_cases::schedule::schedule::NumberedLesson]) -> Vec<Uuid> {
    lessons.iter().map(|l| l.lesson.instance.id).collect()
}

#[sqlx::test(migrations = "../../migrations")]
async fn next_after_either_half_of_a_double_skips_the_other_half(pool: PgPool) {
    let w = seed(pool).await;

    let from_first = w.service.next_course_lessons(w.algebra_mon_1[0]).await.unwrap();
    let from_second = w.service.next_course_lessons(w.algebra_mon_2[0]).await.unwrap();

    assert_eq!(ids(&from_first), vec![w.algebra_thu[0]], "Thursday, not the second half");
    assert_eq!(ids(&from_second), vec![w.algebra_thu[0]]);
    // Thursday has only algebra: lesson 1 of that day.
    assert_eq!(from_first[0].number, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn draft_weeks_are_skipped_and_a_double_comes_whole(pool: PgPool) {
    let w = seed(pool).await;

    // After Thursday of week 1: week 2 is a draft, so Monday of week 3.
    let next = w.service.next_course_lessons(w.algebra_thu[0]).await.unwrap();

    assert_eq!(ids(&next), vec![w.algebra_mon_1[2], w.algebra_mon_2[2]], "both halves, in time order");
    // Physics at 08:00 is lesson 1 of that Monday.
    assert_eq!(next.iter().map(|l| l.number).collect::<Vec<_>>(), vec![2, 3]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancelled_lessons_are_skipped(pool: PgPool) {
    let w = seed(pool).await;
    exec(&w.pool, "UPDATE lesson_instances SET status = 'cancelled' WHERE instance_id = $1", &[w.algebra_thu[0]]).await;

    let next = w.service.next_course_lessons(w.algebra_mon_1[0]).await.unwrap();

    assert_eq!(ids(&next), vec![w.algebra_mon_1[2], w.algebra_mon_2[2]]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn nothing_after_the_last_published_lesson(pool: PgPool) {
    let w = seed(pool).await;

    let next = w.service.next_course_lessons(w.algebra_thu[2]).await.unwrap();

    assert!(next.is_empty());
}

/// Half of the class has algebra, the other half probability, at the same times: the
/// day reads "algebra, probability, algebra, probability" and algebra is still a pair.
#[sqlx::test(migrations = "../../migrations")]
async fn a_double_is_found_between_parallel_lessons_of_the_class(pool: PgPool) {
    let w = seed(pool).await;
    let probability = Uuid::new_v4();
    exec(&w.pool, "INSERT INTO subjects (subject_id, name) VALUES ($1, 'Теория вероятностей')", &[probability]).await;
    let course = Uuid::new_v4();
    exec(&w.pool, "INSERT INTO lessons (lesson_id, class_id, subject_id) VALUES ($1, $2, $3)", &[course, w.class_id, probability]).await;
    let first = template(&w.pool, course, "mon", "09:00", "09:45").await;
    let second = template(&w.pool, course, "mon", "09:55", "10:40").await;
    instances(&w.pool, first, 0).await;
    instances(&w.pool, second, 0).await;

    let partners = w.service.double_lesson_partners(w.algebra_mon_1[0]).await.unwrap();
    let partner_ids: Vec<Uuid> = partners.iter().map(|l| l.instance.id).collect();
    assert_eq!(partner_ids, vec![w.algebra_mon_2[0]], "the other algebra half, not probability");

    let next = w.service.next_course_lessons(w.algebra_mon_1[0]).await.unwrap();
    assert_eq!(ids(&next), vec![w.algebra_thu[0]], "skips the second half of the pair");

    let from_thursday = w.service.next_course_lessons(w.algebra_thu[0]).await.unwrap();
    assert_eq!(ids(&from_thursday), vec![w.algebra_mon_1[2], w.algebra_mon_2[2]]);
    // Parallel lessons share a number: physics 1, then the two slots 2 and 3.
    assert_eq!(from_thursday.iter().map(|l| l.number).collect::<Vec<_>>(), vec![2, 3]);
}
