//! Integration tests for `AdminService` against PostgreSQL: users, lesson changes
//! (cancel, cabinet, substitute, note), weeks, announcements.
//!
//! Run from `backend/` with `DATABASE_URL` set (role needs CREATEDB):
//!     cargo test -p application --test admin

use std::sync::Arc;

use application::use_cases::admin::{
    AdminService, AnnouncementInput, LessonChange, NewUser, UserChange, WeekSource,
};
use application::use_cases::schedule::ScheduleService;
use chrono::NaiveDate;
use domain::entities::user::User;
use domain::errors::DomainError;
use domain::repositories::admin_repository::UserFilter;
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::{
    announcement_priority::AnnouncementPriority, announcement_status::AnnouncementStatus,
    lesson_instance_status::LessonInstanceStatus, role::UserRole,
};
use infrastructure::postgres::{
    AdminRepositoryPg, AnnouncementRepositoryPg, CabinetRepositoryPg, ClassRepositoryPg,
    HomeworkRepositoryPg, LessonInstanceRepositoryPg, LessonRepositoryPg,
    LessonSubstitutionRepositoryPg, LessonTemplateRepositoryPg, ScheduleWeekRepositoryPg,
    StudentGroupRepositoryPg, SubjectRepositoryPg, UserRepositoryPg,
};
use sqlx::PgPool;
use uuid::Uuid;

const MONDAY: &str = "2026-10-19";

struct World {
    admin_service: AdminService,
    schedule: Arc<ScheduleService>,
    users: UserRepositoryPg,
    admin: User,
    teacher: User,
    substitute: Uuid,
    student: User,
    class_id: Uuid,
    instance: Uuid,
    cabinet: Uuid,
}

fn day(raw: &str) -> NaiveDate {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d").unwrap()
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

/// Class 2027б with one algebra lesson on Monday in a published week; an admin,
/// the usual teacher, a substitute teacher, a student and cabinet 204.
async fn seed(pool: PgPool) -> World {
    let class_id = Uuid::new_v4();
    sqlx::query("INSERT INTO classes (class_id, graduation_year, letter) VALUES ($1, 2027, 'б')")
        .bind(class_id)
        .execute(&pool)
        .await
        .unwrap();
    let subject = Uuid::new_v4();
    sqlx::query("INSERT INTO subjects (subject_id, name) VALUES ($1, 'Алгебра')")
        .bind(subject)
        .execute(&pool)
        .await
        .unwrap();
    let admin = insert_user(&pool, "admin", None).await;
    let teacher = insert_user(&pool, "teacher", None).await;
    let substitute = insert_user(&pool, "teacher", None).await;
    let student = insert_user(&pool, "student", Some(class_id)).await;

    let lesson = Uuid::new_v4();
    sqlx::query("INSERT INTO lessons (lesson_id, class_id, subject_id) VALUES ($1, $2, $3)")
        .bind(lesson)
        .bind(class_id)
        .bind(subject)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO lesson_teachers (lesson_id, teacher_id) VALUES ($1, $2)")
        .bind(lesson)
        .bind(teacher)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO schedule_weeks (week_start_date, status) VALUES ($1::date, 'published')")
        .bind(MONDAY)
        .execute(&pool)
        .await
        .unwrap();
    let template = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO lesson_templates (template_id, lesson_id, day, start_time, end_time)
         VALUES ($1, $2, 'mon', '09:00', '09:45')",
    )
    .bind(template)
    .bind(lesson)
    .execute(&pool)
    .await
    .unwrap();
    let instance = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO lesson_instances (instance_id, template_id, week_start_date, lesson_date)
         VALUES ($1, $2, $3::date, $3::date)",
    )
    .bind(instance)
    .bind(template)
    .bind(MONDAY)
    .execute(&pool)
    .await
    .unwrap();
    let cabinet = Uuid::new_v4();
    sqlx::query("INSERT INTO cabinets (cabinet_id, number) VALUES ($1, 204)")
        .bind(cabinet)
        .execute(&pool)
        .await
        .unwrap();

    let substitutions = Arc::new(LessonSubstitutionRepositoryPg::new(pool.clone()));
    let schedule = Arc::new(
        ScheduleService::new(
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
        )
        .with_substitutions(substitutions.clone()),
    );
    let admin_service = AdminService::new(
        Arc::new(AdminRepositoryPg::new(pool.clone())),
        Arc::new(UserRepositoryPg::new(pool.clone())),
        Arc::new(ClassRepositoryPg::new(pool.clone())),
        Arc::new(CabinetRepositoryPg::new(pool.clone())),
        Arc::new(LessonInstanceRepositoryPg::new(pool.clone())),
        substitutions,
        Arc::new(AnnouncementRepositoryPg::new(pool.clone())),
        schedule.clone(),
    );
    let users = UserRepositoryPg::new(pool);
    World {
        admin_service,
        schedule,
        admin: users.get_by_id(admin).await.unwrap(),
        teacher: users.get_by_id(teacher).await.unwrap(),
        substitute,
        student: users.get_by_id(student).await.unwrap(),
        users,
        class_id,
        instance,
        cabinet,
    }
}

fn announcement(title: &str, status: AnnouncementStatus, class_ids: Vec<Uuid>) -> AnnouncementInput {
    AnnouncementInput {
        title: Some(title.to_string()),
        content: None,
        status,
        priority: AnnouncementPriority::Normal,
        expires_at: None,
        class_ids,
        group_ids: vec![],
        user_ids: vec![],
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_admins_may_use_the_service(pool: PgPool) {
    let w = seed(pool).await;

    let as_teacher = w.admin_service.list_users(&w.teacher, UserFilter::default()).await;
    let as_student = w.admin_service.lessons_for_date(&w.student, day(MONDAY)).await;

    assert_eq!(as_teacher.unwrap_err(), DomainError::InsufficientPermissions);
    assert_eq!(as_student.unwrap_err(), DomainError::InsufficientPermissions);
}

#[sqlx::test(migrations = "../../migrations")]
async fn users_are_listed_with_class_and_filtered(pool: PgPool) {
    let w = seed(pool).await;

    let students = w
        .admin_service
        .list_users(&w.admin, UserFilter { role: Some(UserRole::Student), ..Default::default() })
        .await
        .unwrap();
    let by_login = w
        .admin_service
        .list_users(&w.admin, UserFilter { query: Some(w.teacher.login.to_uppercase()), ..Default::default() })
        .await
        .unwrap();

    assert_eq!(students.len(), 1);
    assert_eq!(students[0].class_name.as_deref(), Some("2027б"));
    assert_eq!(by_login.len(), 1);
    assert_eq!(by_login[0].user.id, w.teacher.id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn created_user_has_normalized_login_and_staff_has_no_class(pool: PgPool) {
    let w = seed(pool).await;

    let user = w
        .admin_service
        .create_user(
            &w.admin,
            NewUser {
                login: " Ivan.Petrov ".into(),
                role: UserRole::Teacher,
                last_name: "Петров".into(),
                first_name: "Иван".into(),
                middle_name: Some("  ".into()),
                class_id: Some(w.class_id),
            },
        )
        .await
        .unwrap();
    let duplicate = w
        .admin_service
        .create_user(
            &w.admin,
            NewUser {
                login: "ivan.petrov".into(),
                role: UserRole::Student,
                last_name: "П".into(),
                first_name: "И".into(),
                middle_name: None,
                class_id: None,
            },
        )
        .await;

    assert_eq!(user.login, "ivan.petrov");
    assert_eq!(user.class_id, None);
    assert_eq!(user.middle_name, None);
    assert_eq!(duplicate.unwrap_err(), DomainError::LoginAlreadyExists);
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_cannot_demote_or_deactivate_themselves(pool: PgPool) {
    let w = seed(pool).await;

    let demote = w
        .admin_service
        .update_user(&w.admin, w.admin.id, UserChange { role: Some(UserRole::Teacher), ..Default::default() })
        .await;
    let deactivate = w
        .admin_service
        .update_user(&w.admin, w.admin.id, UserChange { is_active: Some(false), ..Default::default() })
        .await;
    let rename = w
        .admin_service
        .update_user(&w.admin, w.admin.id, UserChange { first_name: Some("Новое".into()), ..Default::default() })
        .await;

    assert_eq!(demote.unwrap_err(), DomainError::InsufficientPermissions);
    assert_eq!(deactivate.unwrap_err(), DomainError::InsufficientPermissions);
    assert_eq!(rename.unwrap().first_name, "Новое");
}

#[sqlx::test(migrations = "../../migrations")]
async fn student_promoted_to_teacher_loses_class(pool: PgPool) {
    let w = seed(pool).await;

    let user = w
        .admin_service
        .update_user(&w.admin, w.student.id, UserChange { role: Some(UserRole::Teacher), ..Default::default() })
        .await
        .unwrap();

    assert_eq!(user.class_id, None);
    assert_eq!(w.users.get_by_id(w.student.id).await.unwrap().class_id, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn substitute_shows_in_student_and_substitute_schedules(pool: PgPool) {
    let w = seed(pool).await;

    let lesson = w
        .admin_service
        .update_lesson(
            &w.admin,
            w.instance,
            LessonChange {
                cabinet_id: Some(Some(w.cabinet)),
                substitute_teacher_id: Some(Some(w.substitute)),
                note: Some(Some("в актовом зале".into())),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let student_day = w.schedule.day_schedule(w.student.id, day(MONDAY)).await.unwrap();
    let substitute_day = w.schedule.teacher_day_schedule(w.substitute, day(MONDAY)).await.unwrap();

    assert_eq!(lesson.cabinet.map(|c| c.number), Some(204));
    assert_eq!(lesson.substitute_teacher.map(|t| t.id), Some(w.substitute));
    let shown = &student_day.lessons[0];
    assert_eq!(shown.substitution.as_ref().and_then(|s| s.note.as_deref()), Some("в актовом зале"));
    assert_eq!(substitute_day.lessons.len(), 1, "the substitute sees the lesson");
}

#[sqlx::test(migrations = "../../migrations")]
async fn clearing_teacher_keeps_note_and_clearing_both_removes_substitution(pool: PgPool) {
    let w = seed(pool).await;
    let set = LessonChange {
        substitute_teacher_id: Some(Some(w.substitute)),
        note: Some(Some("заметка".into())),
        ..Default::default()
    };
    w.admin_service.update_lesson(&w.admin, w.instance, set).await.unwrap();

    let without_teacher = w
        .admin_service
        .update_lesson(
            &w.admin,
            w.instance,
            LessonChange {
                substitute_teacher_id: Some(None),
                status: Some(LessonInstanceStatus::Cancelled),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let cleared = w
        .admin_service
        .update_lesson(&w.admin, w.instance, LessonChange { note: Some(None), ..Default::default() })
        .await
        .unwrap();

    assert!(without_teacher.instance.status.is_cancelled());
    assert_eq!(without_teacher.substitute_teacher, None);
    assert_eq!(without_teacher.substitution.and_then(|s| s.note).as_deref(), Some("заметка"));
    assert_eq!(cleared.substitution, None);
    assert!(cleared.instance.status.is_cancelled(), "status is kept when not sent");
}

#[sqlx::test(migrations = "../../migrations")]
async fn student_cannot_substitute(pool: PgPool) {
    let w = seed(pool).await;

    let result = w
        .admin_service
        .update_lesson(
            &w.admin,
            w.instance,
            LessonChange { substitute_teacher_id: Some(Some(w.student.id)), ..Default::default() },
        )
        .await;

    assert_eq!(result.unwrap_err(), DomainError::UserNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn weeks_start_on_monday_and_are_never_reset(pool: PgPool) {
    let w = seed(pool).await;

    let tuesday = w.admin_service.create_week(&w.admin, day("2026-10-27"), WeekSource::Empty).await;
    let existing = w.admin_service.create_week(&w.admin, day(MONDAY), WeekSource::Empty).await;
    let copy = w
        .admin_service
        .create_week(&w.admin, day("2026-10-26"), WeekSource::CopyFrom(day(MONDAY)))
        .await
        .unwrap();
    let copied_lessons = w.admin_service.lessons_for_date(&w.admin, day("2026-10-26")).await.unwrap();

    assert_eq!(tuesday.unwrap_err(), DomainError::InvalidLessonInstanceDate);
    assert_eq!(existing.unwrap_err(), DomainError::ScheduleWeekAlreadyExists);
    assert!(copy.is_draft());
    assert_eq!(copied_lessons.lessons.len(), 1, "admins see draft weeks");
    let published = w.admin_service.set_week_published(&w.admin, day("2026-10-26"), true).await.unwrap();
    assert!(published.is_published());
}

#[sqlx::test(migrations = "../../migrations")]
async fn announcements_reach_only_their_classes(pool: PgPool) {
    let w = seed(pool).await;

    w.admin_service
        .create_announcement(&w.admin, announcement("Для 11Б", AnnouncementStatus::Published, vec![w.class_id]))
        .await
        .unwrap();
    w.admin_service
        .create_announcement(&w.admin, announcement("Черновик", AnnouncementStatus::Draft, vec![]))
        .await
        .unwrap();
    let everyone = w
        .admin_service
        .create_announcement(&w.admin, announcement("Всем", AnnouncementStatus::Published, vec![]))
        .await
        .unwrap();
    let empty = w
        .admin_service
        .create_announcement(&w.admin, AnnouncementInput { title: Some(" ".into()), ..announcement("", AnnouncementStatus::Draft, vec![]) })
        .await;

    let titles = |list: Vec<domain::entities::announcement::Announcement>| {
        let mut titles: Vec<String> = list.into_iter().filter_map(|a| a.title).collect();
        titles.sort();
        titles
    };
    let student_sees = titles(w.admin_service.visible_announcements(&w.student).await.unwrap());
    let teacher_sees = titles(w.admin_service.visible_announcements(&w.teacher).await.unwrap());
    let all = w.admin_service.list_announcements(&w.admin).await.unwrap();

    assert_eq!(student_sees, vec!["Всем", "Для 11Б"]);
    assert_eq!(teacher_sees, vec!["Всем"]);
    assert_eq!(all.len(), 3);
    assert_eq!(empty.unwrap_err(), DomainError::InvalidAnnouncementContent);

    w.admin_service.delete_announcement(&w.admin, everyone.id).await.unwrap();
    assert_eq!(w.admin_service.list_announcements(&w.admin).await.unwrap().len(), 2);
}
