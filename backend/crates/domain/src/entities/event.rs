//! Event entity and related types.
//!
//! Invariants:
//! - `title` must be non-empty after trimming and at most 255 chars (DB `VARCHAR(255)`).
//! - `description` is optional; if provided, whitespace-only is normalized to `None`
//!   (DB `TEXT` has no length limit, so only trimming is applied).
//! - `end_time` must be strictly after `start_time` (DB CHECK `chk_event_time`).
//! - `cabinet_id` is optional; the DB FK is `ON DELETE SET NULL` — deleting a
//!   cabinet detaches it from events instead of blocking deletion.
//! - `organizer_id` is MUTABLE: who leads the event now / who to contact
//!   (metadata, NOT attendance). Can be handed over (DB FK `ON DELETE RESTRICT`).
//! - `created_by` is IMMUTABLE after creation: who created the event (audit for
//!   the archive). Set at creation, never touched by upsert (DB FK `ON DELETE RESTRICT`).
//!   On creation `created_by == organizer_id` (the creator is the initial organizer).
//! - `created_at` is set at creation time and is immutable (audit trail).
//!   `updated_at` is maintained by the `trigger_events_updated_at` trigger,
//!   so the entity does not carry it.
//!
//! Attendance is EXPLICIT: `EventAttendee` rows hold PARTICIPANTS — any user
//! (students AND teachers), one flat list. The organizer/creator is never
//! auto-added; the admin creates most events and attends none of them.
//!
//! Dependencies: Only `crate::errors::DomainError` and `uuid::Uuid`.
//! Guarantees: Instances can only be created via `try_new`, which validates
//! the invariants. This prevents invalid entities from reaching the repository.

use crate::errors::DomainError;
use uuid::Uuid;

/// Representation of a school event in the domain model.
///
/// Events are one-off (no recurrence in the MVP — see docs §8.4) and
/// reference an optional cabinet, a mutable organizer (contact/lead) and an
/// immutable creator (audit). Participants — students and teachers — are
/// `EventAttendee` rows in `event_attendees`.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// Unique event identifier (UUID v4) — corresponds to `event_id` in DB.
    pub id: Uuid,
    /// Event title (e.g. "Олимпиада по математике"). Trimmed, 1..=255 chars.
    pub title: String,
    /// Optional description. Whitespace-only is normalized to `None`.
    pub description: Option<String>,
    /// Start time (concrete date + time, UTC).
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// End time (UTC). Must be strictly after `start_time`.
    pub end_time: chrono::DateTime<chrono::Utc>,
    /// Optional cabinet where the event takes place.
    /// DB FK `ON DELETE SET NULL` — the reference may be detached by cabinet deletion.
    pub cabinet_id: Option<Uuid>,
    /// Who leads the event now / who to contact. MUTABLE — handing the event
    /// over to another user is a supported operation (metadata, not attendance).
    pub organizer_id: Uuid,
    /// Who created the event. IMMUTABLE after creation (audit for the archive);
    /// on creation equals `organizer_id`.
    pub created_by: Uuid,
    /// Creation timestamp (UTC). Provided by the caller (typically `Utc::now()`
    /// at the use-case layer); immutable after creation.
    /// Corresponds to `created_at TIMESTAMPTZ DEFAULT NOW()` in the DB.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Event {
    /// Constructor with invariant validation (Fail-safe).
    ///
    /// Returns `Err(DomainError::InvalidEventTitle)` if `title` is empty or
    /// whitespace-only after trimming (the trimmed value is stored), or longer
    /// than 255 chars (counting `chars()` like `StudentGroup::try_new`).
    ///
    /// Returns `Err(DomainError::InvalidEventTime)` if `end_time <= start_time`.
    ///
    /// `description` is trimmed; whitespace-only is normalized to `None`
    /// (mirrors `Cabinet::try_new`). No validation on UUIDs or `created_at`
    /// (caller-provided timestamp).
    pub fn try_new(
        id: Uuid,
        title: String,
        description: Option<String>,
        start_time: chrono::DateTime<chrono::Utc>,
        end_time: chrono::DateTime<chrono::Utc>,
        cabinet_id: Option<Uuid>,
        organizer_id: Uuid,
        created_by: Uuid,
        created_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Self, DomainError> {
        // Title: trim, reject empty/whitespace-only, enforce DB VARCHAR(255).
        let trimmed_title = title.trim();
        if trimmed_title.is_empty() {
            return Err(DomainError::InvalidEventTitle);
        }
        if trimmed_title.chars().count() > 255 {
            return Err(DomainError::InvalidEventTitle);
        }

        // Time ordering: end must be strictly after start (DB CHECK chk_event_time).
        if end_time <= start_time {
            return Err(DomainError::InvalidEventTime);
        }

        // Description: trim; whitespace-only → None (no length limit in DB).
        let validated_description = match description {
            Some(text) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            None => None,
        };

        Ok(Self {
            id,
            title: trimmed_title.to_string(),
            description: validated_description,
            start_time,
            end_time,
            cabinet_id,
            organizer_id,
            created_by,
            created_at,
        })
    }
}

/// Representation of a user's participation in an event.
///
/// Attendees are PARTICIPANTS — any user (student or teacher), one flat list.
/// The organizer/creator is metadata and is NOT an attendee by default:
/// attendance is explicit.
///
/// DB: `event_attendees` table with a UNIQUE index on `(event_id, user_id)`
/// — a user cannot attend the same event twice (idempotent by design).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventAttendee {
    /// Unique attendee identifier (UUID v4) — corresponds to `attendee_id` in DB.
    pub id: Uuid,
    /// The event being attended (FK `ON DELETE CASCADE` — deleting an event
    /// removes its attendees).
    pub event_id: Uuid,
    /// The attending user — student or teacher (FK `ON DELETE CASCADE`).
    pub user_id: Uuid,
    /// When the attendance was recorded. Caller-provided (typically `Utc::now()`);
    /// immutable after creation.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl EventAttendee {
    /// Constructor with invariant validation (Fail-safe).
    ///
    /// No validation beyond the closed set of UUIDs — the DB enforces
    /// referential integrity (FKs) and the "one user per event once"
    /// rule (UNIQUE index). Presence checks live at the repository layer.
    pub fn try_new(
        id: Uuid,
        event_id: Uuid,
        user_id: Uuid,
        created_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id,
            event_id,
            user_id,
            created_at,
        })
    }
}
