//! Repository trait for student group persistence.
//!
//! Dependencies: Only types from `crate::entities` and `crate::errors`.
//! Guarantees: All methods return `Result`. No panics are allowed.
//! Implementation of this trait is located in the `infrastructure` crate.
use crate::entities::student_group::StudentGroup;
use crate::errors::DomainError;
use uuid::Uuid;

/// Interface for interacting with the student group storage.
///
/// A group is an arbitrary subset of students that may span multiple classes.
/// This repository manages both the group catalog and group membership
/// (`group_members`).
///
/// Using a trait allows mocking the database in use-case unit tests
/// without spinning up a real PostgreSQL instance.
#[async_trait::async_trait]
pub trait StudentGroupRepository: Send + Sync {
    /// Fetches a group by its unique identifier.
    /// Fail-safe: Returns `StudentGroupNotFound` if the record doesn't exist,
    /// rather than `None` (forcing the caller to handle this case).
    async fn get_by_id(&self, group_id: Uuid) -> Result<StudentGroup, DomainError>;

    /// Fetches all groups, sorted alphabetically by name.
    ///
    /// Performance: The implementation should rely on the unique index:
    /// `CREATE UNIQUE INDEX idx_student_groups_name ON student_groups (name);`
    async fn get_all(&self) -> Result<Vec<StudentGroup>, DomainError>;

    /// Saves or updates a group.
    ///
    /// Uses PostgreSQL `INSERT ... ON CONFLICT` for atomic upsert.
    /// If a group with the same `group_id` exists, it updates the name.
    /// If a group with the same `name` exists (but different `group_id`),
    /// it raises a unique violation, mapped to `DomainError::StudentGroupAlreadyExists`.
    async fn save(&self, group: StudentGroup) -> Result<StudentGroup, DomainError>;

    /// Adds a student to a group.
    ///
    /// Idempotent: adding a student who is already a member is a no-op
    /// (implemented via `INSERT ... ON CONFLICT DO NOTHING`).
    /// Fail-safe: Returns `StudentGroupNotFound` if the group doesn't exist.
    async fn add_member(&self, group_id: Uuid, student_id: Uuid) -> Result<(), DomainError>;

    /// Adds multiple students to a group in a single query (idempotent).
    ///
    /// Returns `Err(DomainError::StudentGroupNotFound)` if the group does not exist.
    /// Ignores students who are already members of the group.
    /// If an empty slice is provided, returns `Ok(())` without querying the database.
    async fn add_members(&self, group_id: Uuid, student_ids: &[Uuid]) -> Result<(), DomainError>;

    /// Removes a student from a group.
    ///
    /// Idempotent: removing a non-member is a no-op.
    /// Fail-safe: Returns `StudentGroupNotFound` if the group doesn't exist.
    async fn remove_member(&self, group_id: Uuid, student_id: Uuid) -> Result<(), DomainError>;

    /// Fetches the IDs of all students in a group.
    ///
    /// Performance: relies on `idx_group_members_group`.
    /// Returns `Vec<Uuid>` (not `User`) to avoid coupling the group repository
    /// to the user domain model — full objects can be fetched via
    /// `UserRepository` in the use case when needed.
    async fn get_member_ids(&self, group_id: Uuid) -> Result<Vec<Uuid>, DomainError>;

    /// Fetches all groups a student belongs to, sorted by group name.
    ///
    /// Performance: relies on `idx_group_members_student`.
    /// Used, for example, when building a student's schedule.
    async fn get_groups_by_student(
        &self,
        student_id: Uuid,
    ) -> Result<Vec<StudentGroup>, DomainError>;

    /// Checks if a student is a member of a group.
    ///
    /// Returns `true` if the student belongs to the group, `false` otherwise.
    /// Does not raise an error if the group or student does not exist —
    /// simply returns `false` in such cases.
    ///
    /// Performance: relies on `idx_group_members_group` and `idx_group_members_student`.
    async fn has_member(&self, group_id: Uuid, student_id: Uuid) -> Result<bool, DomainError>;
}

