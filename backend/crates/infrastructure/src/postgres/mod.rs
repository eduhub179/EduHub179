//! PostgreSQL infrastructure module.
//!
//! Contains repository implementations and connection management.
//! All types here are internal to the infrastructure layer;
//! the domain layer interacts only via traits.

pub mod cabinet_repository_pg;
pub mod class_repository_pg;
pub mod homework_repository_pg;
pub mod lesson_repository_pg;
pub mod lesson_template_repository_pg;
pub mod student_group_repository_pg;
pub mod subject_repository_pg;
pub mod user_repository_pg;

// Re-export for convenience in the `bin` crate (DI composition root).
pub use cabinet_repository_pg::CabinetRepositoryPg;
pub use class_repository_pg::ClassRepositoryPg;
pub use homework_repository_pg::HomeworkRepositoryPg;
pub use lesson_repository_pg::LessonRepositoryPg;
pub use lesson_template_repository_pg::LessonTemplateRepositoryPg;
pub use student_group_repository_pg::StudentGroupRepositoryPg;
pub use subject_repository_pg::SubjectRepositoryPg;
pub use user_repository_pg::UserRepositoryPg;
