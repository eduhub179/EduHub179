//! Repository trait for event persistence.
//!
//! Dependencies: Only types from `crate::entities::event` and `crate::errors`.
//! Guarantees: All methods return `Result`. No panics are allowed.
//! Implementation of this trait is located in the `infrastructure` crate.

use crate::entities::event::{Event, EventAttendee};
use crate::errors::DomainError;
use uuid::Uuid;

/// Interface for interacting with the event storage.
///
/// An event is a one-off occurrence (no recurrence in the MVP) with an
/// optional cabinet, a MUTABLE organizer (who leads now / contact — metadata,
/// not attendance) and an IMMUTABLE creator (audit). Participants — any user,
/// one flat list — are `event_attendees` rows managed as an aggregate of the
/// event (like `homework_files` for homeworks). Attendance is explicit:
/// nobody is auto-added, not even the organizer/creator.
///
/// Using a trait allows mocking the database in use-case unit tests
/// without spinning up a real PostgreSQL instance.
#[async_trait::async_trait]
pub trait EventRepository: Send + Sync {
    /// Fetches an event by its unique identifier.
    ///
    /// Fail-safe: Returns `EventNotFound` if the record doesn't exist,
    /// rather than `None` (forcing the caller to handle this case).
    async fn get_by_id(&self, event_id: Uuid) -> Result<Event, DomainError>;

    /// Fetches all events starting within the half-open range `[start, end)`.
    ///
    /// Uses `WHERE start_time >= $1 AND start_time < $2`, so a single calendar
    /// day is `(day_start, day_start + 1 day)`. Sorted by `start_time`.
    /// Performance: relies on `idx_events_date`.
    async fn get_by_date_range(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Event>, DomainError>;

    /// Fetches all events organized by the given user, sorted by `start_time`.
    ///
    /// Performance: relies on `idx_events_organizer`.
    /// Returns empty vec for an unknown organizer (list-method precedent).
    async fn get_by_organizer(&self, organizer_id: Uuid) -> Result<Vec<Event>, DomainError>;

    /// Fetches all events a user attends, sorted by `start_time` — the
    /// role-agnostic schedule query for events.
    ///
    /// Performance: relies on `idx_event_attendees_user`.
    /// Returns empty vec for an unknown user (list-method precedent).
    async fn get_by_user(&self, user_id: Uuid) -> Result<Vec<Event>, DomainError>;

    /// Fetches all attendees of an event, sorted by `created_at` then `attendee_id`.
    ///
    /// Performance: relies on `idx_event_attendees_event`.
    /// Returns empty vec for a missing event (list-method precedent,
    /// like `get_member_ids`).
    async fn get_attendees(&self, event_id: Uuid) -> Result<Vec<EventAttendee>, DomainError>;

    /// Saves or updates an event (atomic upsert on `event_id`).
    ///
    /// Uses PostgreSQL `INSERT ... ON CONFLICT (event_id) DO UPDATE`.
    /// Mutable fields (updated on conflict): title, description, start_time,
    /// end_time, cabinet_id, organizer_id (handover is supported).
    /// IMMUTABLE (deliberately excluded from UPDATE list): `created_by`
    /// and `created_at`; `updated_at` is maintained by the trigger.
    ///
    /// FK violations are mapped by constraint name: missing organizer or
    /// creator → `UserNotFound`, missing cabinet → `CabinetNotFound`.
    async fn save(&self, event: Event) -> Result<Event, DomainError>;

    /// Deletes an event by its ID.
    ///
    /// Attendees cascade via FK `ON DELETE CASCADE` (no manual cleanup needed).
    /// Fail-safe: Returns `EventNotFound` if no row was affected.
    async fn delete(&self, event_id: Uuid) -> Result<(), DomainError>;

    /// Adds a user to an event (idempotent).
    ///
    /// Uses `INSERT ... ON CONFLICT (event_id, user_id) DO NOTHING`
    /// (UNIQUE index `idx_event_attendees_unique`) — attending twice
    /// is a silent no-op.
    ///
    /// Fail-safe: missing event → `EventNotFound`, missing user →
    /// `UserNotFound` (FK violations mapped by constraint name).
    async fn add_attendee(&self, attendee: EventAttendee) -> Result<EventAttendee, DomainError>;

    /// Removes a user from an event by the (event, user) pair.
    ///
    /// Fail-safe: Returns `EventAttendeeNotFound` if no row was affected
    /// (explicit contract — the caller learns whether the user was there).
    async fn remove_attendee(&self, event_id: Uuid, user_id: Uuid) -> Result<(), DomainError>;
}
