mod common;

use common::{block_on, date, service_with};
use domain::entities::schedule_week::ScheduleWeek;
use domain::errors::DomainError;
use domain::value_objects::week_status::WeekStatus;
use uuid::Uuid;

#[test]
fn student_cannot_read_draft_day_schedule() {
    let service = service_with(vec![ScheduleWeek::new(
        date(2026, 8, 31),
        WeekStatus::Draft,
        None,
    )]);
    let result = block_on(service.day_schedule(Uuid::nil(), date(2026, 9, 2)));

    assert_eq!(result, Err(DomainError::ScheduleWeekNotFound));
}
