//! PostgreSQL infrastructure module.
//!
//! Contains repository implementations and connection management.
//! All types here are internal to the infrastructure layer;
//! the domain layer interacts only via traits.

pub mod announcement_repository_pg;
pub mod cabinet_repository_pg;
pub mod class_repository_pg;
pub mod connections;
pub mod credetials_store_pg;
pub mod device_token_repository_pg;
pub mod event_repository_pg;
pub mod homework_repository_pg;
pub mod lesson_instance_repository_pg;
pub mod lesson_material_repository_pg;
pub mod lesson_repository_pg;
pub mod lesson_template_repository_pg;
pub mod plusnik_repository_pg;
pub mod schedule_week_repository_pg;
pub mod student_group_repository_pg;
pub mod subject_repository_pg;
pub mod telegram_bindigs_repository_pg;
pub mod user_repository_pg;
pub mod web_session_repository_pg;

// Re-export for convenience in the `bin` crate (DI composition root).
pub use announcement_repository_pg::AnnouncementRepositoryPg;
pub use cabinet_repository_pg::CabinetRepositoryPg;
pub use class_repository_pg::ClassRepositoryPg;
pub use connections::create_pool;
pub use credetials_store_pg::CredentialsStorePg;
pub use device_token_repository_pg::DeviceTokenRepositoryPg;
pub use event_repository_pg::EventRepositoryPg;
pub use homework_repository_pg::HomeworkRepositoryPg;
pub use lesson_instance_repository_pg::LessonInstanceRepositoryPg;
pub use lesson_material_repository_pg::LessonMaterialRepositoryPg;
pub use lesson_repository_pg::LessonRepositoryPg;
pub use lesson_template_repository_pg::LessonTemplateRepositoryPg;
pub use plusnik_repository_pg::PlusnikRepositoryPg;
pub use schedule_week_repository_pg::ScheduleWeekRepositoryPg;
pub use student_group_repository_pg::StudentGroupRepositoryPg;
pub use subject_repository_pg::SubjectRepositoryPg;
pub use telegram_bindigs_repository_pg::TelegramBindingRepositoryPg;
pub use user_repository_pg::UserRepositoryPg;
pub use web_session_repository_pg::WebSessionRepositoryPg;
