//! PostgreSQL infrastructure module.
//!
//! Contains repository implementations and connection management.
//! All types here are internal to the infrastructure layer;
//! the domain layer interacts only via traits.

pub mod user_repository_pg;
pub mod class_repository_pg;
pub mod subject_repository_pg;

// Re-export for convenience in the `bin` crate (DI composition root).
pub use user_repository_pg::UserRepositoryPg;
pub use class_repository_pg::ClassRepositoryPg;
pub use subject_repository_pg::SubjectRepositoryPg;