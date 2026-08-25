//! Integration tests for `EventRepositoryPg`.
//!
//! These tests verify the public API of the infrastructure crate
//! with a real PostgreSQL database using `sqlx::test` for automatic
//! transaction management and rollback.
//!
//! Coverage:
//! - Event CRUD: `get_by_id`, `save` (create/update/handover/errors), `delete` (cascade).
//! - Attendees: `add_attendee` (idempotent, FK errors), `remove_attendee`,
//!   `get_attendees` (sorting, empty cases).
//! - Queries: `get_by_date_range` (half-open, sorting), `get_by_organizer` (sorting),
//!   `get_by_user` (attendance, sorting).
//! - Audit: `created_by` immutable, `organizer_id` mutable (handover).
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

/// Helper: fixed UTC timestamp — deterministic tests, no clock drift.
fn dt(y: i32, m: u32, d: u32, h: u32, min: u32) -> chrono::DateTime<chrono::Utc> {
    chrono::NaiveDate::from_ymd_opt(y, m, d)
        .expect("valid date")
        .and_hms_opt(h, min, 0)
        .expect("valid time")
        .and_utc()
}

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

/// Helper: creates a test event with fixed times.
fn create_test_event(
    organizer_id: Uuid,
    created_by: Uuid,
    cabinet_id: Option<Uuid>,
    start: chrono::DateTime<chrono::Utc>,
) -> Event {
    Event::try_new(
        Uuid::new_v4(),
        "Олимпиада по математике".to_string(),
        Some("Школьный тур".to_string()),
        start,
        start + chrono::Duration::hours(2),
        cabinet_id,
        organizer_id,
        created_by,
        chrono::Utc::now(),
    )
    .expect("Test event should satisfy domain invariants")
}

/// Helper: creates a test attendee row with a random ID and explicit created_at.
fn create_test_attendee_at(
    event_id: Uuid,
    user_id: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
) -> EventAttendee {
    EventAttendee::try_new(Uuid::new_v4(), event_id, user_id, created_at)
        .expect("Test attendee data should be valid")
}

/// Helper: creates a test attendee row with a random ID (now timestamp).
fn create_test_attendee(event_id: Uuid, user_id: Uuid) -> EventAttendee {
    create_test_attendee_at(event_id, user_id, chrono::Utc::now())
}

/// Seeds the base FK chain for events via the real repositories:
/// one teacher (who is BOTH organizer and creator) and a cabinet.
///
/// Returns `(event_id, teacher_id, cabinet_id)`.
async fn seed_event(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
    let user_repo = UserRepositoryPg::new(pool.clone());
    let teacher = create_test_teacher();
    user_repo
        .save(teacher.clone())
        .await
        .expect("Save teacher should succeed");

    let cabinet = create_test_cabinet(301);
    CabinetRepositoryPg::new(pool.clone())
        .save(cabinet.clone())
        .await
        .expect("Save cabinet should succeed");

    let event = create_test_event(teacher.id, teacher.id, Some(cabinet.id), dt(2026, 9, 1, 10, 0));
    EventRepositoryPg::new(pool.clone())
        .save(event.clone())
        .await
        .expect("Save event should succeed");

    (event.id, teacher.id, cabinet.id)
}

/// Seeds an additional user (any role) via the repository.
async fn seed_user(pool: &PgPool, user: User) -> Uuid {
    UserRepositoryPg::new(pool.clone())
        .save(user)
        .await
        .expect("Save user should succeed")
        .id
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
        dt(2026, 9, 1, 10, 0),
        dt(2026, 9, 1, 11, 0),
        None,
        Uuid::new_v4(),
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
        dt(2026, 9, 1, 10, 0),
        dt(2026, 9, 1, 11, 0),
        None,
        Uuid::new_v4(),
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
        dt(2026, 9, 1, 10, 0),
        dt(2026, 9, 1, 11, 0),
        None,
        Uuid::new_v4(),
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
        dt(2026, 9, 1, 10, 0),
        dt(2026, 9, 1, 11, 0),
        None,
        Uuid::new_v4(),
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
        dt(2026, 9, 1, 10, 0),
        dt(2026, 9, 1, 9, 0),
        None,
        Uuid::new_v4(),
        Uuid::new_v4(),
        chrono::Utc::now(),
    )
    .unwrap_err();
    assert_eq!(err, DomainError::InvalidEventTime);
}

#[test]
fn test_event_try_new_rejects_end_equal_start() {
    let start = dt(2026, 9, 1, 10, 0);
    let err = Event::try_new(
        Uuid::new_v4(),
        "Событие".to_string(),
        None,
        start,
        start,
        None,
        Uuid::new_v4(),
        Uuid::new_v4(),
        chrono::Utc::now(),
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
        dt(2026, 9, 1, 10, 0),
        dt(2026, 9, 1, 11, 0),
        None,
        Uuid::new_v4(),
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
        dt(2026, 9, 1, 10, 0),
        dt(2026, 9, 1, 11, 0),
        None,
        Uuid::new_v4(),
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
        dt(2026, 9, 1, 10, 0),
        dt(2026, 9, 1, 11, 0),
        None,
        Uuid::new_v4(),
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
    let (event_id, teacher_id, cabinet_id) = seed_event(&pool).await;
    let repo = EventRepositoryPg::new(pool);

    let event = repo
        .get_by_id(event_id)
        .await
        .expect("Event should be found");
    assert_eq!(event.id, event_id);
    assert_eq!(event.title, "Олимпиада по математике");
    assert_eq!(event.description.as_deref(), Some("Школьный тур"));
    assert_eq!(event.organizer_id, teacher_id);
    assert_eq!(event.created_by, teacher_id);
    assert_eq!(event.cabinet_id, Some(cabinet_id));
    assert!(event.end_time > event.start_time);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_no_cabinet_roundtrips(pool: PgPool) {
    let teacher_id = seed_user(&pool, create_test_teacher()).await;

    let event = create_test_event(teacher_id, teacher_id, None, dt(2026, 9, 1, 10, 0));
    let repo = EventRepositoryPg::new(pool.clone());
    repo.save(event.clone()).await.expect("Save should succeed");

    let fetched = repo
        .get_by_id(event.id)
        .await
        .expect("Event should be found");
    assert_eq!(fetched.cabinet_id, None);
    assert_eq!(fetched.description, event.description);
    assert_eq!(fetched.created_by, teacher_id);
}

// ============================================================================
// SAVE (CREATE / UPDATE / HANDOVER / ERRORS)
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_creates_event(pool: PgPool) {
    let (event_id, _teacher_id, _cabinet_id) = seed_event(&pool).await;
    let repo = EventRepositoryPg::new(pool);
    let fetched = repo
        .get_by_id(event_id)
        .await
        .expect("Event should be found");
    assert_eq!(fetched.id, event_id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_updates_event_preserves_audit(pool: PgPool) {
    let (event_id, _teacher_id, _cabinet_id) = seed_event(&pool).await;
    let repo = EventRepositoryPg::new(pool);

    let original = repo
        .get_by_id(event_id)
        .await
        .expect("Event should be found");
    let original_created_by = original.created_by;
    let original_created_at = original.created_at;

    // Update mutable fields: title, description, times, cabinet (detach).
    let mut updated = original.clone();
    updated.title = "Обновлённое событие".to_string();
    updated.description = None;
    updated.start_time = dt(2026, 9, 4, 12, 0);
    updated.end_time = dt(2026, 9, 4, 13, 0);
    updated.cabinet_id = None;

    repo.save(updated.clone()).await.expect("Update should succeed");

    let fetched = repo
        .get_by_id(event_id)
        .await
        .expect("Event should be found");
    assert_eq!(fetched.title, "Обновлённое событие");
    assert_eq!(fetched.description, None);
    assert_eq!(fetched.cabinet_id, None);
    assert_eq!(fetched.start_time, dt(2026, 9, 4, 12, 0));
    assert_eq!(fetched.end_time, dt(2026, 9, 4, 13, 0));
    // Audit immutables: created_by and created_at untouched by upsert.
    assert_eq!(fetched.organizer_id, original.organizer_id);
    assert_eq!(fetched.created_by, original_created_by);
    assert_eq!(
        fetched.created_at.timestamp_micros(),
        original_created_at.timestamp_micros()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_organizer_handover_updates_organizer_preserves_creator(pool: PgPool) {
    let (event_id, _teacher_id, _cabinet_id) = seed_event(&pool).await;
    // New teacher takes over as organizer.
    let new_organizer_id = seed_user(&pool, create_test_teacher()).await;
    let repo = EventRepositoryPg::new(pool.clone());

    let original = repo
        .get_by_id(event_id)
        .await
        .expect("Event should be found");
    assert_eq!(original.organizer_id, original.created_by, "initially organizer == creator");

    let mut handed_over = original.clone();
    handed_over.organizer_id = new_organizer_id;
    repo.save(handed_over).await.expect("Handover should succeed");

    let fetched = repo
        .get_by_id(event_id)
        .await
        .expect("Event should be found");
    assert_eq!(fetched.organizer_id, new_organizer_id, "organizer is mutable");
    assert_eq!(
        fetched.created_by,
        original.created_by,
        "creator (audit) survives the handover"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_created_by_is_immutable_on_update(pool: PgPool) {
    let (event_id, teacher_id, _cabinet_id) = seed_event(&pool).await;
    // A different user who did NOT create the event.
    let other_user_id = seed_user(&pool, create_test_teacher()).await;
    let repo = EventRepositoryPg::new(pool.clone());

    let original = repo
        .get_by_id(event_id)
        .await
        .expect("Event should be found");
    assert_eq!(original.created_by, teacher_id);

    // Even if the caller passes a different created_by, the DB must keep the
    // original (created_by is deliberately excluded from the UPDATE list).
    let mut tampered = original.clone();
    tampered.created_by = other_user_id;
    repo.save(tampered).await.expect("Save should succeed");

    let fetched = repo
        .get_by_id(event_id)
        .await
        .expect("Event should be found");
    assert_eq!(fetched.created_by, teacher_id, "created_by must not change on update");
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_with_non_existent_organizer_returns_user_not_found(pool: PgPool) {
    let teacher_id = seed_user(&pool, create_test_teacher()).await;
    let event = create_test_event(Uuid::new_v4(), teacher_id, None, dt(2026, 9, 1, 10, 0));
    let repo = EventRepositoryPg::new(pool);
    let err = repo.save(event).await.unwrap_err();
    assert_eq!(err, DomainError::UserNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_with_non_existent_created_by_returns_user_not_found(pool: PgPool) {
    let teacher_id = seed_user(&pool, create_test_teacher()).await;
    let event = create_test_event(teacher_id, Uuid::new_v4(), None, dt(2026, 9, 1, 10, 0));
    let repo = EventRepositoryPg::new(pool);
    let err = repo.save(event).await.unwrap_err();
    assert_eq!(err, DomainError::UserNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_save_with_non_existent_cabinet_returns_cabinet_not_found(pool: PgPool) {
    let teacher_id = seed_user(&pool, create_test_teacher()).await;
    let event = create_test_event(teacher_id, teacher_id, Some(Uuid::new_v4()), dt(2026, 9, 1, 10, 0));
    let repo = EventRepositoryPg::new(pool);
    let err = repo.save(event).await.unwrap_err();
    assert_eq!(err, DomainError::CabinetNotFound);
}

// ============================================================================
// DELETE
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_delete_success_cascades_attendees(pool: PgPool) {
    let (event_id, _teacher_id, _cabinet_id) = seed_event(&pool).await;
    let student_id = seed_user(&pool, create_test_student()).await;
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
    let (event_id, _teacher_id, _cabinet_id) = seed_event(&pool).await;
    // A TEACHER as attendee — attendance is role-agnostic, not student-only.
    let teacher_attendee_id = seed_user(&pool, create_test_teacher()).await;
    let repo = EventRepositoryPg::new(pool.clone());

    repo.add_attendee(create_test_attendee(event_id, teacher_attendee_id))
        .await
        .expect("Add attendee should succeed");

    let attendees = repo
        .get_attendees(event_id)
        .await
        .expect("List should succeed");
    assert_eq!(attendees.len(), 1);
    assert_eq!(attendees[0].event_id, event_id);
    assert_eq!(attendees[0].user_id, teacher_attendee_id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_add_attendee_idempotent(pool: PgPool) {
    let (event_id, _teacher_id, _cabinet_id) = seed_event(&pool).await;
    let student_id = seed_user(&pool, create_test_student()).await;
    let repo = EventRepositoryPg::new(pool.clone());

    let attendee = create_test_attendee(event_id, student_id);
    repo.add_attendee(attendee.clone())
        .await
        .expect("First add should succeed");
    // Same (event, user) pair again — silent no-op, not an error.
    repo.add_attendee(attendee.clone())
        .await
        .expect("Second add should be a no-op");

    let attendees = repo
        .get_attendees(event_id)
        .await
        .expect("List should succeed");
    assert_eq!(attendees.len(), 1, "User must not attend twice");
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_add_attendee_missing_event_returns_event_not_found(pool: PgPool) {
    let student_id = seed_user(&pool, create_test_student()).await;
    let repo = EventRepositoryPg::new(pool);
    let err = repo
        .add_attendee(create_test_attendee(Uuid::new_v4(), student_id))
        .await
        .unwrap_err();
    assert_eq!(err, DomainError::EventNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_add_attendee_missing_user_returns_user_not_found(pool: PgPool) {
    let (event_id, _teacher_id, _cabinet_id) = seed_event(&pool).await;
    let repo = EventRepositoryPg::new(pool);
    let err = repo
        .add_attendee(create_test_attendee(event_id, Uuid::new_v4()))
        .await
        .unwrap_err();
    assert_eq!(err, DomainError::UserNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_remove_attendee_success(pool: PgPool) {
    let (event_id, _teacher_id, _cabinet_id) = seed_event(&pool).await;
    let student_id = seed_user(&pool, create_test_student()).await;
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
    let (event_id, _teacher_id, _cabinet_id) = seed_event(&pool).await;
    let student_id = seed_user(&pool, create_test_student()).await;
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

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_attendees_sorted_by_created_at(pool: PgPool) {
    let (event_id, _teacher_id, _cabinet_id) = seed_event(&pool).await;
    let user_a = seed_user(&pool, create_test_student()).await;
    let user_b = seed_user(&pool, create_test_student()).await;
    let user_c = seed_user(&pool, create_test_student()).await;
    let repo = EventRepositoryPg::new(pool.clone());

    // Insert in NON-sorted order: C (12:00), A (10:00), B (11:00).
    repo.add_attendee(create_test_attendee_at(event_id, user_c, dt(2026, 9, 1, 12, 0)))
        .await
        .expect("Add C should succeed");
    repo.add_attendee(create_test_attendee_at(event_id, user_a, dt(2026, 9, 1, 10, 0)))
        .await
        .expect("Add A should succeed");
    repo.add_attendee(create_test_attendee_at(event_id, user_b, dt(2026, 9, 1, 11, 0)))
        .await
        .expect("Add B should succeed");

    let attendees = repo
        .get_attendees(event_id)
        .await
        .expect("List should succeed");
    let ids: Vec<Uuid> = attendees.iter().map(|a| a.user_id).collect();
    assert_eq!(ids, vec![user_a, user_b, user_c], "sorted by created_at");
}

// ============================================================================
// QUERIES
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_date_range_filters_half_open(pool: PgPool) {
    let (event_id, _teacher_id, _cabinet_id) = seed_event(&pool).await;
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
async fn test_get_by_date_range_sorted_by_start_time(pool: PgPool) {
    let teacher_id = seed_user(&pool, create_test_teacher()).await;
    let repo = EventRepositoryPg::new(pool.clone());

    // Insert in NON-sorted order: 14:00, 09:00, 11:00.
    for start in [dt(2026, 9, 2, 14, 0), dt(2026, 9, 2, 9, 0), dt(2026, 9, 2, 11, 0)] {
        repo.save(create_test_event(teacher_id, teacher_id, None, start))
            .await
            .expect("Save should succeed");
    }

    let events = repo
        .get_by_date_range(dt(2026, 9, 2, 0, 0), dt(2026, 9, 3, 0, 0))
        .await
        .expect("Query should succeed");
    let starts: Vec<chrono::DateTime<chrono::Utc>> = events.iter().map(|e| e.start_time).collect();
    assert_eq!(
        starts,
        vec![dt(2026, 9, 2, 9, 0), dt(2026, 9, 2, 11, 0), dt(2026, 9, 2, 14, 0)],
        "sorted by start_time"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_organizer_sorted(pool: PgPool) {
    let teacher_id = seed_user(&pool, create_test_teacher()).await;
    let repo = EventRepositoryPg::new(pool.clone());

    for start in [dt(2026, 9, 2, 14, 0), dt(2026, 9, 2, 9, 0), dt(2026, 9, 2, 11, 0)] {
        repo.save(create_test_event(teacher_id, teacher_id, None, start))
            .await
            .expect("Save should succeed");
    }

    let events = repo
        .get_by_organizer(teacher_id)
        .await
        .expect("Query should succeed");
    assert_eq!(events.len(), 3);
    let starts: Vec<chrono::DateTime<chrono::Utc>> = events.iter().map(|e| e.start_time).collect();
    assert_eq!(
        starts,
        vec![dt(2026, 9, 2, 9, 0), dt(2026, 9, 2, 11, 0), dt(2026, 9, 2, 14, 0)],
        "sorted by start_time"
    );

    // Unknown organizer → empty, not an error.
    let empty = repo
        .get_by_organizer(Uuid::new_v4())
        .await
        .expect("Query should succeed");
    assert!(empty.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_user(pool: PgPool) {
    let (event_id, _teacher_id, _cabinet_id) = seed_event(&pool).await;
    let student_id = seed_user(&pool, create_test_student()).await;
    let repo = EventRepositoryPg::new(pool.clone());

    // Not attending yet → empty.
    let before = repo
        .get_by_user(student_id)
        .await
        .expect("Query should succeed");
    assert!(before.is_empty());

    repo.add_attendee(create_test_attendee(event_id, student_id))
        .await
        .expect("Add attendee should succeed");

    let events = repo
        .get_by_user(student_id)
        .await
        .expect("Query should succeed");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id, event_id);
    assert_eq!(events[0].title, "Олимпиада по математике");
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_user_sorted_by_start_time(pool: PgPool) {
    let teacher_id = seed_user(&pool, create_test_teacher()).await;
    let student_id = seed_user(&pool, create_test_student()).await;
    let repo = EventRepositoryPg::new(pool.clone());

    // Three events on different times; the student attends all three.
    let mut event_ids = Vec::new();
    for start in [dt(2026, 9, 2, 14, 0), dt(2026, 9, 2, 9, 0), dt(2026, 9, 2, 11, 0)] {
        let event = create_test_event(teacher_id, teacher_id, None, start);
        repo.save(event.clone()).await.expect("Save should succeed");
        event_ids.push(event.id);
    }
    for event_id in &event_ids {
        repo.add_attendee(create_test_attendee(*event_id, student_id))
            .await
            .expect("Add attendee should succeed");
    }

    let events = repo
        .get_by_user(student_id)
        .await
        .expect("Query should succeed");
    let starts: Vec<chrono::DateTime<chrono::Utc>> = events.iter().map(|e| e.start_time).collect();
    assert_eq!(
        starts,
        vec![dt(2026, 9, 2, 9, 0), dt(2026, 9, 2, 11, 0), dt(2026, 9, 2, 14, 0)],
        "sorted by start_time"
    );
}
