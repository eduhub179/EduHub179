mod common;

use common::{date, service_with};
use domain::errors::DomainError;

#[tokio::test]
async fn publishing_missing_week_returns_not_found() {
    let service = service_with(Vec::new());
    let result = service.publish(date(2026, 8, 31)).await;

    assert_eq!(result, Err(DomainError::ScheduleWeekNotFound));
}
