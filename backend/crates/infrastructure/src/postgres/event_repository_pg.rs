//! PostgreSQL implementation of `EventRepository`.
//!
//! Dependencies: `sqlx` (PostgreSQL driver), `domain` crate.
//! Guarantees:
//! - All methods return `Result`. No panics, no `unwrap()`.
//! - Database errors are mapped to `DomainError` for clean business logic.
//! - Uses indexes defined in migrations for optimal performance.
//!
//! Performance notes:
//! - `get_by_id` relies on the primary key.
//! - `get_by_date_range` / `get_by_organizer` rely on `idx_events_date` / `idx_events_organizer`.
//! - `get_by_user` relies on `idx_event_attendees_user` (JOIN).
//! - `get_attendees` relies on `idx_event_attendees_event`.

use domain::entities::event::{Event, EventAttendee};
use domain::errors::DomainError;
use domain::repositories::event_repository::EventRepository;
use sqlx::PgPool;
use uuid::Uuid;

/// Internal structure for mapping rows from PostgreSQL (`events`).
/// Kept private to isolate database schema from domain model.
#[derive(Debug, sqlx::FromRow)]
struct EventRow {
    event_id: Uuid,
    title: String,
    description: Option<String>,
    start_time: chrono::DateTime<chrono::Utc>,
    end_time: chrono::DateTime<chrono::Utc>,
    cabinet_id: Option<Uuid>,
    organizer_id: Uuid,
    created_by: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl EventRow {
    /// Converts the database row into a domain `Event` entity.
    /// Returns `Err` if the data violates domain invariants (data corruption in DB).
    fn into_domain(self) -> Result<Event, DomainError> {
        Event::try_new(
            self.event_id,
            self.title,
            self.description,
            self.start_time,
            self.end_time,
            self.cabinet_id,
            self.organizer_id,
            self.created_by,
            self.created_at,
        )
    }
}

/// Internal structure for mapping rows from PostgreSQL (`event_attendees`).
/// Kept private to isolate database schema from domain model.
#[derive(Debug, sqlx::FromRow)]
struct EventAttendeeRow {
    attendee_id: Uuid,
    event_id: Uuid,
    user_id: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl EventAttendeeRow {
    /// Converts the database row into a domain `EventAttendee` entity.
    fn into_domain(self) -> Result<EventAttendee, DomainError> {
        EventAttendee::try_new(self.attendee_id, self.event_id, self.user_id, self.created_at)
    }
}

/// PostgreSQL-backed implementation of `EventRepository`.
///
/// Uses a connection pool (`PgPool`) for efficient connection reuse.
/// All queries use runtime type checking (no compile-time `query!` macro).
pub struct EventRepositoryPg {
    pool: PgPool,
}

impl EventRepositoryPg {
    /// Creates a new repository instance.
    /// Fail-safe: Does not validate the pool connection here;
    /// connection issues will surface on the first query.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Maps low-level `sqlx::Error` to domain-level `DomainError`.
    /// This is the single point of error translation, ensuring
    /// business logic never sees database-specific errors.
    ///
    /// Constraint violations (FK 23503) are mapped by constraint NAME so each
    /// missing parent entity gets its own error:
    /// - `events.organizer_id` -> the organizer user is gone
    /// - `events.created_by` -> the creator user is gone
    /// - `events.cabinet_id` -> the cabinet is gone
    /// - `event_attendees.event_id` -> the owning event is gone
    /// - `event_attendees.user_id` -> the attending user is gone
    fn map_db_error(err: sqlx::Error) -> DomainError {
        match err {
            // Read/update/delete paths: no row for the requested event.
            sqlx::Error::RowNotFound => DomainError::EventNotFound,
            sqlx::Error::Database(db_err) => match db_err.code().as_deref() {
                // 23503 = foreign_key_violation. Constraint name tells us
                // WHICH parent record is missing.
                Some("23503") => match db_err.constraint() {
                    Some("events_organizer_id_fkey") | Some("events_created_by_fkey") => {
                        DomainError::UserNotFound
                    }
                    Some("events_cabinet_id_fkey") => DomainError::CabinetNotFound,
                    Some("event_attendees_event_id_fkey") => DomainError::EventNotFound,
                    Some("event_attendees_user_id_fkey") => DomainError::UserNotFound,
                    // Unknown constraint: keep the catch-all.
                    _ => DomainError::EventNotFound,
                },
                // 23505 (unique_violation) is not expected: `save` upserts on the
                // PK and `add_attendee` uses ON CONFLICT DO NOTHING.
                _ => DomainError::EventNotFound,
            },
            _ => DomainError::EventNotFound,
        }
    }
}

#[async_trait::async_trait]
impl EventRepository for EventRepositoryPg {
    /// Fetches an event by ID.
    /// Performance: Uses primary key index (O(log n)).
    async fn get_by_id(&self, event_id: Uuid) -> Result<Event, DomainError> {
        let row = sqlx::query_as::<_, EventRow>(
            r#"
            SELECT event_id, title, description, start_time, end_time,
                   cabinet_id, organizer_id, created_by, created_at
            FROM events
            WHERE event_id = $1
            "#,
        )
        .bind(event_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        row.into_domain()
    }

    /// Fetches all events starting within the half-open range `[start, end)`.
    /// Performance: Uses `idx_events_date`.
    async fn get_by_date_range(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Event>, DomainError> {
        let rows = sqlx::query_as::<_, EventRow>(
            r#"
            SELECT event_id, title, description, start_time, end_time,
                   cabinet_id, organizer_id, created_by, created_at
            FROM events
            WHERE start_time >= $1 AND start_time < $2
            ORDER BY start_time
            "#,
        )
        .bind(start)
        .bind(end)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        rows.into_iter().map(EventRow::into_domain).collect()
    }

    /// Fetches all events organized by the given user, sorted by start_time.
    /// Performance: Uses `idx_events_organizer`.
    async fn get_by_organizer(&self, organizer_id: Uuid) -> Result<Vec<Event>, DomainError> {
        let rows = sqlx::query_as::<_, EventRow>(
            r#"
            SELECT event_id, title, description, start_time, end_time,
                   cabinet_id, organizer_id, created_by, created_at
            FROM events
            WHERE organizer_id = $1
            ORDER BY start_time
            "#,
        )
        .bind(organizer_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        rows.into_iter().map(EventRow::into_domain).collect()
    }

    /// Fetches all events a user (student or teacher) attends, sorted by start_time.
    /// Performance: Uses `idx_event_attendees_user` (JOIN).
    async fn get_by_user(&self, user_id: Uuid) -> Result<Vec<Event>, DomainError> {
        let rows = sqlx::query_as::<_, EventRow>(
            r#"
            SELECT e.event_id, e.title, e.description, e.start_time, e.end_time,
                   e.cabinet_id, e.organizer_id, e.created_by, e.created_at
            FROM events e
                     JOIN event_attendees ea ON ea.event_id = e.event_id
            WHERE ea.user_id = $1
            ORDER BY e.start_time
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        rows.into_iter().map(EventRow::into_domain).collect()
    }

    /// Fetches all attendees of an event, sorted by created_at then attendee_id.
    /// Performance: Uses `idx_event_attendees_event`.
    async fn get_attendees(&self, event_id: Uuid) -> Result<Vec<EventAttendee>, DomainError> {
        let rows = sqlx::query_as::<_, EventAttendeeRow>(
            r#"
            SELECT attendee_id, event_id, user_id, created_at
            FROM event_attendees
            WHERE event_id = $1
            ORDER BY created_at, attendee_id
            "#,
        )
        .bind(event_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        rows.into_iter()
            .map(EventAttendeeRow::into_domain)
            .collect()
    }

    /// Saves or updates an event.
    ///
    /// Uses PostgreSQL `INSERT ... ON CONFLICT` for atomic upsert.
    /// If an event with the same `event_id` exists, it updates the MUTABLE
    /// fields: title, description, start_time, end_time, cabinet_id and
    /// organizer_id (handover is supported).
    ///
    /// Note: `created_by` and `created_at` are immutable after creation
    /// (deliberately omitted from the UPDATE list; `created_at` is written on
    /// INSERT only). `updated_at` is maintained by the trigger.
    async fn save(&self, event: Event) -> Result<Event, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO events (event_id, title, description, start_time, end_time,
                                cabinet_id, organizer_id, created_by, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (event_id) DO UPDATE SET
                title = EXCLUDED.title,
                description = EXCLUDED.description,
                start_time = EXCLUDED.start_time,
                end_time = EXCLUDED.end_time,
                cabinet_id = EXCLUDED.cabinet_id,
                organizer_id = EXCLUDED.organizer_id,
                updated_at = NOW()
            "#,
        )
        .bind(event.id)
        .bind(&event.title)
        .bind(event.description.as_deref())
        .bind(event.start_time)
        .bind(event.end_time)
        .bind(event.cabinet_id)
        .bind(event.organizer_id)
        .bind(event.created_by)
        .bind(event.created_at)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        Ok(event)
    }

    /// Deletes an event by ID.
    /// Attendees are deleted via `ON DELETE CASCADE` FK.
    /// Returns `EventNotFound` if no row was affected.
    async fn delete(&self, event_id: Uuid) -> Result<(), DomainError> {
        let result = sqlx::query(
            r#"
            DELETE FROM events WHERE event_id = $1
            "#,
        )
        .bind(event_id)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        if result.rows_affected() == 0 {
            return Err(DomainError::EventNotFound);
        }

        Ok(())
    }

    /// Adds a user (student or teacher) to an event.
    ///
    /// Uses `INSERT ... ON CONFLICT (event_id, user_id) DO NOTHING` —
    /// attending the same event twice is a silent no-op
    /// (UNIQUE index `idx_event_attendees_unique`).
    ///
    /// FK violations still raise (23503) and are mapped by constraint name:
    /// missing event → `EventNotFound`, missing user → `UserNotFound`.
    async fn add_attendee(&self, attendee: EventAttendee) -> Result<EventAttendee, DomainError> {
        sqlx::query(
            r#"
            INSERT INTO event_attendees (attendee_id, event_id, user_id, created_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (event_id, user_id) DO NOTHING
            "#,
        )
        .bind(attendee.id)
        .bind(attendee.event_id)
        .bind(attendee.user_id)
        .bind(attendee.created_at)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        Ok(attendee)
    }

    /// Removes a user from an event by the (event, user) pair.
    /// Returns `EventAttendeeNotFound` if no row was affected (explicit contract).
    async fn remove_attendee(&self, event_id: Uuid, user_id: Uuid) -> Result<(), DomainError> {
        let result = sqlx::query(
            r#"
            DELETE FROM event_attendees WHERE event_id = $1 AND user_id = $2
            "#,
        )
        .bind(event_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(Self::map_db_error)?;

        if result.rows_affected() == 0 {
            return Err(DomainError::EventAttendeeNotFound);
        }

        Ok(())
    }
}
