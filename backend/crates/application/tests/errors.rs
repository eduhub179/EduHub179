mod common;

use common::{block_on, date, service_with};
use domain::errors::DomainError;

#[test]
fn publishing_missing_week_returns_not_found() {
    let service = service_with(Vec::new());
    let result = block_on(service.publish(date(2026, 8, 31)));

    assert_eq!(result, Err(DomainError::ScheduleWeekNotFound));
}
