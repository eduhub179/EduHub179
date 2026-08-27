mod common;

use common::{block_on, date, service_with};
use domain::entities::schedule_week::ScheduleWeek;
use domain::value_objects::week_status::WeekStatus;

#[test]
fn creates_and_lists_draft_week() {
    let service = service_with(Vec::new());
    let created = block_on(service.create_draft(date(2026, 8, 31), None)).unwrap();
    let weeks = block_on(service.list()).unwrap();

    assert_eq!(created.status, WeekStatus::Draft);
    assert_eq!(weeks, vec![created]);
}

#[test]
fn publishes_existing_week() {
    let service = service_with(vec![ScheduleWeek::new(
        date(2026, 8, 31),
        WeekStatus::Draft,
        None,
    )]);

    let published = block_on(service.publish(date(2026, 8, 31))).unwrap();

    assert!(published.is_published());
    assert!(block_on(service.list()).unwrap()[0].is_published());
}
