mod common;

use common::{date, service_with};
use domain::entities::schedule_week::ScheduleWeek;
use domain::value_objects::week_status::WeekStatus;

#[tokio::test]
async fn creates_and_lists_draft_week() {
    let service = service_with(Vec::new());
    let created = service.create_draft(date(2026, 8, 31), None).await.unwrap();
    let weeks = service.list().await.unwrap();

    assert_eq!(created.status, WeekStatus::Draft);
    assert_eq!(weeks, vec![created]);
}

#[tokio::test]
async fn publishes_existing_week() {
    let service = service_with(vec![ScheduleWeek::new(
        date(2026, 8, 31),
        WeekStatus::Draft,
        None,
    )]);

    let published = service.publish(date(2026, 8, 31)).await.unwrap();

    assert!(published.is_published());
    assert!(service.list().await.unwrap()[0].is_published());
}
