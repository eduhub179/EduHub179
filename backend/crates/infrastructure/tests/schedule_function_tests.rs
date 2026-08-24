//! Integration tests for the `get_student_schedule_for_date` SQL function.
//!
//! Coverage of the schedule functions:
//! - a 'scheduled' instance appears in the student's schedule;
//! - a 'cancelled' instance appears WITH its status (greyed client-side,
//!   nothing auto-shadows);
//! - an event that overlaps a lesson is shown ALONGSIDE it (both rows,
//!   no auto-shadowing);
//! - instances of un-published weeks are not visible.

use sqlx::PgPool;
use uuid::Uuid;

// ============================================================================
// HELPERS
// ============================================================================

struct Seed {
    student_id: Uuid,
    #[allow(dead_code)]
    instance_id: Uuid,
}

/// Seeds a student with a class and one Math lesson on Monday 2026-09-07
/// (10:00–10:45, every week) whose instance has the given status.
///
/// Raw SQL only: the schedule layer has no repository yet (it is the next
/// build item). Each test runs in its own isolated database (`sqlx::test`),
/// so fixed names/dates do not collide across tests.
async fn seed_student_with_lesson(pool: &PgPool, instance_status: &str) -> Seed {
    let class_id = Uuid::new_v4();
    let student_id = Uuid::new_v4();
    let subject_id = Uuid::new_v4();
    let lesson_id = Uuid::new_v4();
    let template_id = Uuid::new_v4();
    let instance_id = Uuid::new_v4();

    // 1. Class
    sqlx::query(
        "INSERT INTO classes (class_id, graduation_year, letter)
         VALUES ($1, 2027, 'б'::class_letter)",
    )
    .bind(class_id)
    .execute(pool)
    .await
    .expect("Insert class should succeed");

    // 2. Student (belongs to the class via users.class_id)
    sqlx::query(
        "INSERT INTO users (user_id, email, role, last_name, first_name, class_id)
         VALUES ($1, $2, 'student'::user_role, $3, $4, $5)",
    )
    .bind(student_id)
    .bind(format!("student.{}@example.com", student_id))
    .bind("Ivanov")
    .bind("Student")
    .bind(class_id)
    .execute(pool)
    .await
    .expect("Insert student should succeed");

    // 3. Subject
    sqlx::query("INSERT INTO subjects (subject_id, name) VALUES ($1, 'Алгебра')")
        .bind(subject_id)
        .execute(pool)
        .await
        .expect("Insert subject should succeed");

    // 4. Lesson (class-based)
    sqlx::query(
        "INSERT INTO lessons (lesson_id, class_id, subject_id)
         VALUES ($1, $2, $3)",
    )
    .bind(lesson_id)
    .bind(class_id)
    .bind(subject_id)
    .execute(pool)
    .await
    .expect("Insert lesson should succeed");

    // 5. Template: Monday 10:00-10:45, every week
    sqlx::query(
        "INSERT INTO lesson_templates (template_id, lesson_id, day, start_time, end_time, parity)
         VALUES ($1, $2, 'mon'::day_of_week, '10:00'::TIME, '10:45'::TIME, 'every'::week_parity)",
    )
    .bind(template_id)
    .bind(lesson_id)
    .execute(pool)
    .await
    .expect("Insert lesson template should succeed");

    // 6. Schedule week 2026-09-07 must exist BEFORE its instances (FK);
    //    published so the instance is visible to the student.
    sqlx::query(
        "INSERT INTO schedule_weeks (week_start_date, status)
         VALUES ('2026-09-07'::DATE, 'published')",
    )
    .execute(pool)
    .await
    .expect("Insert schedule week should succeed");

    // 7. Instance on 2026-09-07 (week starts 2026-09-07)
    sqlx::query(
        "INSERT INTO lesson_instances (instance_id, template_id, week_start_date, lesson_date, status)
         VALUES ($1, $2, '2026-09-07'::DATE, '2026-09-07'::DATE, $3::VARCHAR)",
    )
    .bind(instance_id)
    .bind(template_id)
    .bind(instance_status)
    .execute(pool)
    .await
    .expect("Insert lesson instance should succeed");

    Seed {
        student_id,
        instance_id,
    }
}

/// Calls `get_student_schedule_for_date` and returns the raw rows:
/// (start_time, end_time, title, is_event, status, cabinet_id).
async fn fetch_schedule(
    pool: &PgPool,
    student_id: Uuid,
    date: &str,
) -> Vec<(String, String, String, bool, Option<String>, Option<Uuid>)> {
    sqlx::query_as::<_, (String, String, String, bool, Option<String>, Option<Uuid>)>(
        "SELECT start_time::TEXT, end_time::TEXT, title, is_event, status, cabinet_id
         FROM get_student_schedule_for_date($1, $2::DATE)",
    )
    .bind(student_id)
    .bind(date)
    .fetch_all(pool)
    .await
    .expect("Fetch schedule should succeed")
}

// ============================================================================
// TESTS
// ============================================================================

/// A scheduled lesson must appear in the student's schedule.
#[sqlx::test(migrations = "../../migrations")]
async fn test_scheduled_lesson_appears(pool: PgPool) {
    let seed = seed_student_with_lesson(&pool, "scheduled").await;

    let rows = fetch_schedule(&pool, seed.student_id, "2026-09-07").await;

    assert_eq!(rows.len(), 1, "expected exactly one schedule entry");
    let (start, end, title, is_event, status, _cabinet) = &rows[0];
    assert_eq!(title, "Алгебра");
    assert_eq!(start, "10:00:00");
    assert_eq!(end, "10:45:00");
    assert!(!is_event);
    assert_eq!(status.as_deref(), Some("scheduled"));
}

/// A cancelled instance appears WITH its status — the client renders it greyed
/// (nothing auto-shadows, not even cancellations).
#[sqlx::test(migrations = "../../migrations")]
async fn test_cancelled_lesson_appears_with_status(pool: PgPool) {
    let seed = seed_student_with_lesson(&pool, "cancelled").await;

    let rows = fetch_schedule(&pool, seed.student_id, "2026-09-07").await;

    assert_eq!(rows.len(), 1, "expected exactly one schedule entry");
    let (_, _, title, is_event, status, _cabinet) = &rows[0];
    assert_eq!(title, "Алгебра");
    assert!(!is_event);
    assert_eq!(status.as_deref(), Some("cancelled"));
}

/// An event that overlaps a lesson is shown ALONGSIDE it — nothing auto-shadows.
/// The student sees both rows and decides.
#[sqlx::test(migrations = "../../migrations")]
async fn test_event_and_lesson_are_both_shown(pool: PgPool) {
    let seed = seed_student_with_lesson(&pool, "scheduled").await;

    // Organizer (teacher) + event overlapping 10:00-10:45 on 2026-09-07
    let teacher_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (user_id, email, role, last_name, first_name)
         VALUES ($1, $2, 'teacher'::user_role, $3, $4)",
    )
    .bind(teacher_id)
    .bind(format!("teacher.{}@example.com", teacher_id))
    .bind("Petrov")
    .bind("Teacher")
    .execute(&pool)
    .await
    .expect("Insert teacher should succeed");

    let event_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO events (event_id, title, start_time, end_time, organizer_id)
         VALUES ($1, 'Лекция по физике', '2026-09-07 10:00'::TIMESTAMPTZ, '2026-09-07 11:00'::TIMESTAMPTZ, $2)",
    )
    .bind(event_id)
    .bind(teacher_id)
    .execute(&pool)
    .await
    .expect("Insert event should succeed");

    sqlx::query(
        "INSERT INTO event_attendees (event_id, student_id)
         VALUES ($1, $2)",
    )
    .bind(event_id)
    .bind(seed.student_id)
    .execute(&pool)
    .await
    .expect("Insert event attendee should succeed");

    let rows = fetch_schedule(&pool, seed.student_id, "2026-09-07").await;

    assert_eq!(rows.len(), 2, "event and lesson are shown together");
    let titles: Vec<&String> = rows.iter().map(|r| &r.2).collect();
    assert!(titles.contains(&&"Лекция по физике".to_string()));
    assert!(titles.contains(&&"Алгебра".to_string()));
    let event_row = rows.iter().find(|r| r.3).expect("event row present");
    assert_eq!(event_row.4.as_deref(), None, "events carry no lesson status");
}

/// A lesson on a different date must not leak into the queried date.
#[sqlx::test(migrations = "../../migrations")]
async fn test_other_date_is_empty(pool: PgPool) {
    let seed = seed_student_with_lesson(&pool, "scheduled").await;

    let rows = fetch_schedule(&pool, seed.student_id, "2026-09-08").await;

    assert!(rows.is_empty(), "no lessons expected on 2026-09-08");
}

/// Instances of an un-published (draft) week are invisible to students
/// (docs/SCHEDULE.en.md §4: students see only published weeks).
#[sqlx::test(migrations = "../../migrations")]
async fn test_draft_week_is_invisible(pool: PgPool) {
    let seed = seed_student_with_lesson(&pool, "scheduled").await;

    // Flip the week to draft — the visibility gate hides it.
    sqlx::query(
        "UPDATE schedule_weeks SET status = 'draft'
         WHERE week_start_date = '2026-09-07'::DATE",
    )
    .execute(&pool)
    .await
    .expect("Update week status should succeed");

    let rows = fetch_schedule(&pool, seed.student_id, "2026-09-07").await;

    assert!(
        rows.is_empty(),
        "draft week must not be visible to students: {:?}",
        rows
    );
}
