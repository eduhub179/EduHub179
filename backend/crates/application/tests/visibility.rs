mod common;

use common::{date, service_with};
use domain::entities::schedule_week::ScheduleWeek;
use domain::entities::user::User;
use domain::errors::DomainError;
use domain::value_objects::role::UserRole;
use domain::value_objects::week_status::WeekStatus;
use uuid::Uuid;

fn user(id: Uuid, role: UserRole) -> User {
    User::try_new(id, format!("{id}@179.ru"), role, "Фамилия".into(), "Имя".into(), None, None).unwrap()
}

fn draft_week_service() -> application::use_cases::schedule::ScheduleService {
    service_with(vec![ScheduleWeek::new(date(2026, 8, 31), WeekStatus::Draft, None)])
}

#[tokio::test]
async fn student_cannot_read_draft_day_schedule() {
    let service = draft_week_service();
    let me = user(Uuid::nil(), UserRole::Student);
    let result = service.day_schedule(&me, Uuid::nil(), date(2026, 9, 2)).await;

    assert_eq!(result, Err(DomainError::ScheduleWeekNotFound));
}

/// The permission is checked before anything is loaded: another student's schedule is
/// refused even for a week that does not exist.
#[tokio::test]
async fn student_cannot_read_another_students_schedule() {
    let service = draft_week_service();
    let me = user(Uuid::new_v4(), UserRole::Student);
    let other = Uuid::new_v4();

    let day = service.day_schedule(&me, other, date(2026, 9, 2)).await;
    let week = service.current_week_schedule(&me, other, date(2026, 9, 2)).await;

    assert_eq!(day, Err(DomainError::InsufficientPermissions));
    assert_eq!(week.unwrap_err(), DomainError::InsufficientPermissions);
}

#[tokio::test]
async fn teachers_and_admins_read_any_students_schedule() {
    let service = draft_week_service();
    for role in [UserRole::Teacher, UserRole::Admin] {
        let viewer = user(Uuid::new_v4(), role);
        // Passes the permission check; stops at the draft week.
        let day = service.day_schedule(&viewer, Uuid::new_v4(), date(2026, 9, 2)).await;
        assert_eq!(day, Err(DomainError::ScheduleWeekNotFound));
    }
}

#[tokio::test]
async fn inactive_viewer_is_refused() {
    let service = draft_week_service();
    let mut teacher = user(Uuid::new_v4(), UserRole::Teacher);
    teacher.is_active = false;

    let day = service.day_schedule(&teacher, Uuid::new_v4(), date(2026, 9, 2)).await;

    assert_eq!(day, Err(DomainError::UserIsInactive));
}
