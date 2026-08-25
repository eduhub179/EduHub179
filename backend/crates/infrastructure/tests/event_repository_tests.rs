//! Integration tests for `EventRepositoryPg`.
//!
//! These tests verify the public API of the infrastructure crate
//! with a real PostgreSQL database using `sqlx::test` for automatic
//! transaction management and rollback.
//!
//! Coverage:
//! - Event CRUD: `get_by_id`, `save` (create/update/errors), `delete` (cascade).
//! - Attendees: `add_attendee` (idempotent, FK errors), `remove_attendee`,
//!   `get_attendees` (sorting, empty cases).
//! - Queries: `get_by_date_range`, `get_by_organizer`, `get_by_student`.
//! - Domain invariant validation (pure unit tests, no DB).
//!
//! DB fixture note: `events` references `users` and `cabinets`, both of which
//! have repositories — fixtures are seeded through the real repositories.

use domain::entities::cabinet::Cabinet;
use domain::entities::event::{Event, EventAttendee};
use domain::entities::user::User;
use domain::errors::DomainError;
use domain::repositories::cabinet_repository::CabinetRepository;
use domain::repositories::event_repository::EventRepository;
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::role::UserRole;
use infrastructure::postgres::{CabinetRepositoryPg, EventRepositoryPg, UserRepositoryPg};
use sqlx::PgPool;
use uuid::Uuid;

// ============================================================================
// HELPERS
// ============================================================================

/// Helper: creates a test teacher with a random ID and unique email.
fn create_test_teacher() -> User {
    User::try_new(
        Uuid::new_v4(),
        format!("teacher.{}@example.com", Uuid::new_v4()),
        UserRole::Teacher,
        "Petrov".to_string(),
        "Teacher".to_string(),
        None,
        None,
    )
    .expect("Test teacher data should be valid")
}

/// Helper: creates a test student with a random ID and unique email.
fn create_test_student() -> User {
    User::try_new(
        Uuid::new_v4(),
        format!("student.{}@example.com", Uuid::new_v4()),
        UserRole::Student,
        "Ivanov".to_string(),
        "Student".to_string(),
        None,
        None,
    )
    .expect("Test student data should be valid")
}

/// Helper: creates a test cabinet (3-digit number) with a random ID.
fn create_test_cabinet(number: i32) -> Cabinet {
    Cabinet::try_new(Uuid::new_v4(), number, None, None)
        .expect("Test cabinet data should be valid")
}

/// Helper: creates a test event with random ID, times relative to now.
fn create_test_event(
    organizer_id: Uuid,
    cabinet_id: Option<Uuid>,
    start_offset_hours: i64,
) -> Event {
    let start = chrono::Utc::now() + chrono::Duration::hours(start_offset_hours);
    let end = start + chrono::Duration::hours(2);
    Event::try_new(
        Uuid::new_v4(),
        "Олимпиада по математике".to_string(),
        Some("Школьный тур".to_string()),
        start,
        end,
        cabinet_id,
        organizer_id,
        chrono::Utc::now(),
    )
    .expect("Test event should satisfy domain invariants")
}

/// Helper: creates a test attendee row with a random ID.
fn create_test_attendee(event_id: Uuid, student_id: Uuid) -> EventAttendee {
    EventAttendee::try_new(
        Uuid::new_v4(),
        event_id,
        student_id,
        chrono::Utc::now(),
    )
    .expect("Test attendee data should be valid")
}

/// Seeds the FK chain required by `events` via the real repositories:
/// teacher (organizer), student (attendee), cabinet.
///
/// Returns `(event_id, organizer_id, student_id, cabinet_id)`.
async fn seed_event(pool: &PgPool) -> (Uuid, Uuid, Uuid, Uuid) {
    let user_repo = UserRepositoryPg::new(pool.clone());
    let teacher = create_test_teacher();
    let student = create_test_student();
    user_repo
        .save(teacher.clone())
        .await
        .expect("Save teacher should succeed");
    user_repo
        .save(student.clone())
        .await
        .expect("Save student should succeed");

    let cabinet = create_test_cabinet(301);
    CabinetRepositoryPg::new(pool.clone())
        .save(cabinet.clone())
        .await
        .expect("Save cabinet should succeed");

    let event = create_test_event(teacher.id, Some(cabinet.id), 24);
    EventRepositoryPg::new(pool.clone())
        .save(event.clone())
        .await
        .expect("Save event should succeed");

    (event.id, teacher.id, student.id, cabinet.id)
}

// ============================================================================
// DOMAIN INVARIANTS (pure unit tests, no DB)
// ============================================================================

#[test]
fn test_event_try_new_rejects_empty_title() {
    let err = Event::try_new(
        Uuid::new_v4(),
        "   ".to_string(),
        None,
        chrono::Utc::now(),
        chrono::Utc::now() + chrono::Duration::hours(1),
        None,
        Uuid::new_v4(),
        chrono::Utc::now(),
    )
    .unwrap_err();
    assert_eq!(err, DomainError::InvalidEventTitle);
}

#[test]
fn test_event_try_new_trims_title() {
    let event = Event::try_new(
        Uuid::new_v4(),
        "  Концерт  ".to_string(),
        None,
        chrono::Utc::now(),
        chrono::Utc::now() + chrono::Duration::hours(1),
        None,
        Uuid::new_v4(),
        chrono::Utc::now(),
    )
    .expect("Valid title should pass");
    assert_eq!(event.title, "Концерт");
}

#[test]
fn test_event_try_new_rejects_overlong_title() {
    let err = Event::try_new(
        Uuid::new_v4(),
        "а".repeat(256),
        None,
        chrono::Utc::now(),
        chrono::Utc::now() + chrono::Duration::hours(1),
        None,
        Uuid::new_v4(),
        chrono::Utc::now(),
    )
    .unwrap_err();
    assert_eq!(err, DomainError::InvalidEventTitle);
}

#[test]
fn test_event_try_new_allows_max_title() {
    let event = Event::try_new(
        Uuid::new_v4(),
        "а".repeat(255),
        None,
        chrono::Utc::now(),
        chrono::Utc::now() + chrono::Duration::hours(1),
        None,
        Uuid::new_v4(),
        chrono::Utc::now(),
    )
    .expect("255 chars should be valid");
    assert_eq!(event.title.chars().count(), 255);
}

#[test]
fn test_event_try_new_rejects_end_before_start() {
    let err = Event::try_new(
        Uuid::new_v4(),
        "Событие".to_string(),
        None,
        chrono::Utc::now(),
        chrono::Utc::now() - chrono::Duration::hours(1),
        None,
        Uuid::new_v4(),
        chrono::Utc::now(),
    )
    .unwrap_err();
    assert_eq!(err, DomainError::InvalidEventTime);
}

#[test]
fn test_event_try_new_rejects_end_equal_start() {
    let now = chrono::Utc::now();
    let err = Event::try_new(
        Uuid::new_v4(),
        "Событие".to_string(),
        None,
        now,
        now,
        None,
        Uuid::new_v4(),
        now,
    )
    .unwrap_err();
    assert_eq!(err, DomainError::InvalidEventTime);
}

#[test]
fn test_event_try_new_normalizes_whitespace_description() {
    let event = Event::try_new(
        Uuid::new_v4(),
        "Событие".to_string(),
        Some("   ".to_string()),
        chrono::Utc::now(),
        chrono::Utc::now() + chrono::Duration::hours(1),
        None,
        Uuid::new_v4(),
        chrono::Utc::now(),
    )
    .expect("Whitespace description should normalize to None");
    assert_eq!(event.description, None);
}

#[test]
fn test_event_try_new_trims_description() {
    let event = Event::try_new(
        Uuid::new_v4(),
        "Событие".to_string(),
        Some("  подробности  ".to_string()),
        chrono::Utc::now(),
        chrono::Utc::now() + chrono::Duration::hours(1),
        None,
        Uuid::new_v4(),
        chrono::Utc::now(),
    )
    .expect("Description should be trimmed");
    assert_eq!(event.description.as_deref(), Some("подробности"));
}

#[test]
fn test_event_try_new_allows_none_description_and_cabinet() {
    let event = Event::try_new(
        Uuid::new_v4(),
        "Событие".to_string(),
        None,
        chrono::Utc::now(),
        chrono::Utc::now() + chrono::Duration::hours(1),
        None,
        Uuid::new_v4(),
        chrono::Utc::now(),
    )
    .expect("None description and cabinet should be valid");
    assert_eq!(event.description, None);
    assert_eq!(event.cabinet_id, None);
}

// ============================================================================
// GET BY ID
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_not_found(pool: PgPool) {
    let repo = EventRepositoryPg::new(pool);
    let err = repo.get_by_id(Uuid::new_v4()).await.unwrap_err();
    assert_eq!(err, DomainError::EventNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_success(pool: PgPool) {
    let (event_id, organizer_id, _student_id, cabinet_id) = seed_event(&pool).await;
    let repo = EventRepositoryPg::new(pool);

    let event = repo
        .get_by_id(event_id)
        .await
        .expect("Event should be found");
    assert_eq!(event.id, event_id);
    assert_eq!(event.title, "Олимпиада по математике");
    assert_eq!(event.description.as_deref(), Some("Школьный тур"));
    assert_eq!(event.organizer_id, organizer_id);
    assert_eq!(event.cabinet_id, Some(cabinet_id));
    assert!(event.end_time > event.start_time);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_no_cabinet_roundtrips(pool: PgPool) {
    let user_repo = UserRepositoryPg::new(pool.clone());
    let teacher = create_test_teacher();
    user_repo
        .save(teacher.clone())
        .await
        .expect("Save teacher should succeed");

    let event = create_test_event(teacher.id, None, 24);
    let repo = EventRepositoryPg::new(pool.clone());
    repo.save(event.clone()).await.expect("Save should succeed");

    let fetched = repo
        .get_by_id(event.id)
        .await
        .expect("Event should be found");
    assert_eq!(fetched.cabinet_id, None);
    assert_eq!(fetched.description, event.description);
}

// ============================================================================
// SAVE
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_creates_event(pool: PgPool) {
    let (event_id, _organizer_id, _student_id, _cabinet_id) = seed_event(&pool).await;
    let repo = EventRepositoryPg::new(pool);
    let fetched = repo
        .get_by_id(event_id)
        .await
        .expect("Event should be found");
    assert_eq!(fetched.id, event_id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_updates_event_preserves_immutables(pool: PgPool) {
    let (event_id, organizer_id, _student_id, _cabinet_id) = seed_event(&pool).await;
    let repo = EventRepositoryPg::new(pool);

    let original = repo
        .get_by_id(event_id)
        .await
        .expect("Event should be found");
    let original_created_at = original.created_at;

    // Update mutable fields: title, description, times, cabinet (detach).
    let mut updated = original.clone();
    updated.title = "Обновлённое событие".to_string();
    updated.description = None;
    updated.start_time = original.start_time + chrono::Duration::days(3);
    updated.end_time = updated.start_time + chrono::Duration::hours(1);
    updated.cabinet_id = None;

    repo.save(updated.clone()).await.expect("Update should succeed");

    let fetched = repo
        .get_by_id(event_id)
        .await
        .expect("Event should be found");
    assert_eq!(fetched.title, "Обновлённое событие");
    assert_eq!(fetched.description, None);
    assert_eq!(fetched.cabinet_id, None);
    assert_eq!(
        fetched.start_time.timestamp_micros(),
        updated.start_time.timestamp_micros()
    );
    assert_eq!(
        fetched.end_time.timestamp_micros(),
        updated.end_time.timestamp_micros()
    );
    // Immutables: organizer and created_at untouched by upsert.
    assert_eq!(fetched.organizer_id, organizer_id);
    assert_eq!(
        fetched.created_at.timestamp_micros(),
        original_created_at.timestamp_micros()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_with_non_existent_organizer_returns_user_not_found(pool: PgPool) {
    let event = create_test_event(Uuid::new_v4(), None, 24);
    let repo = EventRepositoryPg::new(pool);
    let err = repo.save(event).await.unwrap_err();
    assert_eq!(err, DomainError::UserNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_with_non_existent_cabinet_returns_cabinet_not_found(pool: PgPool) {
    let user_repo = UserRepositoryPg::new(pool.clone());
    let teacher = create_test_teacher();
    user_repo
        .save(teacher.clone())
        .await
        .expect("Save teacher should succeed");

    let event = create_test_event(teacher.id, Some(Uuid::new_v4()), 24);
    let repo = EventRepositoryPg::new(pool);
    let err = repo.save(event).await.unwrap_err();
    assert_eq!(err, DomainError::CabinetNotFound);
}

// ============================================================================
// DELETE
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_delete_success_cascades_attendees(pool: PgPool) {
    let (event_id, _organizer_id, student_id, _cabinet_id) = seed_event(&pool).await;
    let repo = EventRepositoryPg::new(pool.clone());

    repo.add_attendee(create_test_attendee(event_id, student_id))
        .await
        .expect("Add attendee should succeed");

    repo.delete(event_id).await.expect("Delete should succeed");

    // Event is gone...
    let err = repo.get_by_id(event_id).await.unwrap_err();
    assert_eq!(err, DomainError::EventNotFound);
    // ...and attendees cascaded with it.
    let attendees = repo.get_attendees(event_id).await.expect("List should succeed");
    assert!(attendees.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_delete_not_found(pool: PgPool) {
    let repo = EventRepositoryPg::new(pool);
    let err = repo.delete(Uuid::new_v4()).await.unwrap_err();
    assert_eq!(err, DomainError::EventNotFound);
}

// ============================================================================
// ATTENDEES
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_add_attendee_success_and_get_attendees(pool: PgPool) {
    let (event_id, _organizer_id, student_id, _cabinet_id) = seed_event(&pool).await;
    let repo = EventRepositoryPg::new(pool.clone());

    repo.add_attendee(create_test_attendee(event_id, student_id))
        .await
        .expect("Add attendee should succeed");

    let attendees = repo
        .get_attendees(event_id)
        .await
        .expect("List should succeed");
    assert_eq!(attendees.len(), 1);
    assert_eq!(attendees[0].event_id, event_id);
    assert_eq!(attendees[0].student_id, student_id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_add_attendee_idempotent(pool: PgPool) {
    let (event_id, _organizer_id, student_id, _cabinet_id) = seed_event(&pool).await;
    let repo = EventRepositoryPg::new(pool.clone());

    let attendee = create_test_attendee(event_id, student_id);
    repo.add_attendee(attendee.clone())
        .await
        .expect("First add should succeed");
    // Same (event, student) pair again — silent no-op, not an error.
    repo.add_attendee(attendee.clone())
        .await
        .expect("Second add should be a no-op");

    let attendees = repo
        .get_attendees(event_id)
        .await
        .expect("List should succeed");
    assert_eq!(attendees.len(), 1, "Student must not attend twice");
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_add_attendee_missing_event_returns_event_not_found(pool: PgPool) {
    let user_repo = UserRepositoryPg::new(pool.clone());
    let student = create_test_student();
    user_repo
        .save(student.clone())
        .await
        .expect("Save student should succeed");

    let repo = EventRepositoryPg::new(pool);
    let err = repo
        .add_attendee(create_test_attendee(Uuid::new_v4(), student.id))
        .await
        .unwrap_err();
    assert_eq!(err, DomainError::EventNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_add_attendee_missing_student_returns_user_not_found(pool: PgPool) {
    let (event_id, _organizer_id, _student_id, _cabinet_id) = seed_event(&pool).await;
    let repo = EventRepositoryPg::new(pool);
    let err = repo
        .add_attendee(create_test_attendee(event_id, Uuid::new_v4()))
        .await
        .unwrap_err();
    assert_eq!(err, DomainError::UserNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_remove_attendee_success(pool: PgPool) {
    let (event_id, _organizer_id, student_id, _cabinet_id) = seed_event(&pool).await;
    let repo = EventRepositoryPg::new(pool.clone());

    repo.add_attendee(create_test_attendee(event_id, student_id))
        .await
        .expect("Add attendee should succeed");
    repo.remove_attendee(event_id, student_id)
        .await
        .expect("Remove should succeed");

    let attendees = repo
        .get_attendees(event_id)
        .await
        .expect("List should succeed");
    assert!(attendees.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_remove_attendee_not_found(pool: PgPool) {
    let (event_id, _organizer_id, student_id, _cabinet_id) = seed_event(&pool).await;
    let repo = EventRepositoryPg::new(pool);
    let err = repo
        .remove_attendee(event_id, student_id)
        .await
        .unwrap_err();
    assert_eq!(err, DomainError::EventAttendeeNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_attendees_missing_event_returns_empty(pool: PgPool) {
    let repo = EventRepositoryPg::new(pool);
    let attendees = repo
        .get_attendees(Uuid::new_v4())
        .await
        .expect("List should succeed");
    assert!(attendees.is_empty());
}

// ============================================================================
// QUERIES
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_date_range_filters_half_open(pool: PgPool) {
    let (event_id, organizer_id, _student_id, _cabinet_id) = seed_event(&pool).await;
    let repo = EventRepositoryPg::new(pool.clone());

    let fetched = repo
        .get_by_id(event_id)
        .await
        .expect("Event should be found");

    // Exact half-open window around the event start.
    let window = repo
        .get_by_date_range(
            fetched.start_time - chrono::Duration::minutes(1),
            fetched.start_time + chrono::Duration::minutes(1),
        )
        .await
        .expect("Query should succeed");
    assert_eq!(window.len(), 1);
    assert_eq!(window[0].id, event_id);

    // Window that ends before the event starts → excluded.
    let before = repo
        .get_by_date_range(
            fetched.start_time - chrono::Duration::hours(2),
            fetched.start_time - chrono::Duration::minutes(1),
        )
        .await
        .expect("Query should succeed");
    assert!(before.is_empty());

    // Window that starts exactly at event start → included (half-open [start, end)).
    let from_start = repo
        .get_by_date_range(fetched.start_time, fetched.start_time + chrono::Duration::hours(1))
        .await
        .expect("Query should succeed");
    assert_eq!(from_start.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_organizer(pool: PgPool) {
    let (event_id, organizer_id, _student_id, _cabinet_id) = seed_event(&pool).await;
    let repo = EventRepositoryPg::new(pool.clone());

    let events = repo
        .get_by_organizer(organizer_id)
        .await
        .expect("Query should succeed");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id, event_id);

    // Unknown organizer → empty, not an error.
    let empty = repo
        .get_by_organizer(Uuid::new_v4())
        .await
        .expect("Query should succeed");
    assert!(empty.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_student(pool: PgPool) {
    let (event_id, _organizer_id, student_id, _cabinet_id) = seed_event(&pool).await;
    let repo = EventRepositoryPg::new(pool.clone());

    // Not attending yet → empty.
    let before = repo
        .get_by_student(student_id)
        .await
        .expect("Query should succeed");
    assert!(before.is_empty());

    repo.add_attendee(create_test_attendee(event_id, student_id))
        .await
        .expect("Add attendee should succeed");

    let events = repo
        .get_by_student(student_id)
        .await
        .expect("Query should succeed");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id, event_id);
    assert_eq!(events[0].title, "Олимпиада по математике");
}

