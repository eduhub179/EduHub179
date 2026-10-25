//! Repository trait for course materials (topics and materials).
//!
//! Dependencies: `crate::entities::lesson_material`, `crate::errors`.
//! Guarantees: all methods return `Result`, no panics.
//! Implementation: `infrastructure::postgres::LessonMaterialRepositoryPg`.

use uuid::Uuid;

use crate::entities::lesson_material::{LessonMaterial, LessonMaterialTopic};
use crate::errors::DomainError;

/// Which materials of a course to load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialTopicFilter {
    /// Every material of the course.
    All,
    /// Materials without a topic ("Без темы").
    WithoutTopic,
    /// Materials of one topic.
    Topic(Uuid),
}

#[async_trait::async_trait]
pub trait LessonMaterialRepository: Send + Sync {
    // ========================================================================
    // Topics
    // ========================================================================

    /// Topics of a course ordered by `sort_order`, then name.
    async fn get_topics(&self, lesson_id: Uuid) -> Result<Vec<LessonMaterialTopic>, DomainError>;

    /// Fail-safe: `MaterialTopicNotFound` if there is no such topic.
    async fn get_topic(&self, topic_id: Uuid) -> Result<LessonMaterialTopic, DomainError>;

    /// Upsert on `topic_id` (`lesson_id`, `created_by` are immutable).
    /// Errors: `MaterialTopicAlreadyExists` (same name in the course, case-insensitive),
    /// `LessonNotFound` (FK).
    async fn save_topic(&self, topic: LessonMaterialTopic) -> Result<LessonMaterialTopic, DomainError>;

    /// Deletes a topic; its materials move to "Без темы" (FK `ON DELETE SET NULL (topic_id)`).
    /// Fail-safe: `MaterialTopicNotFound` if no row was deleted.
    async fn delete_topic(&self, topic_id: Uuid) -> Result<(), DomainError>;

    /// Number of materials per topic of a course in one query; the `None` key counts
    /// materials without a topic. Topics without materials are absent.
    async fn count_materials_by_topic(
        &self,
        lesson_id: Uuid,
    ) -> Result<Vec<(Option<Uuid>, i64)>, DomainError>;

    // ========================================================================
    // Materials
    // ========================================================================

    /// Materials of a course by filter, ordered by `sort_order`, then creation time.
    async fn get_materials(
        &self,
        lesson_id: Uuid,
        filter: MaterialTopicFilter,
    ) -> Result<Vec<LessonMaterial>, DomainError>;

    /// Fail-safe: `MaterialNotFound` if there is no such material.
    async fn get_material(&self, material_id: Uuid) -> Result<LessonMaterial, DomainError>;

    /// Upsert on `material_id` (`lesson_id`, content and `created_by` are immutable;
    /// title, description, topic and order can change).
    /// Errors: `MaterialTopicNotFound` if the topic is missing or belongs to another
    /// course (composite FK), `LessonNotFound` (FK).
    async fn save_material(&self, material: LessonMaterial) -> Result<LessonMaterial, DomainError>;

    /// Deletes a material and returns the deleted record, so the caller removes the
    /// file object by the key from the DB. Fail-safe: `MaterialNotFound` if absent.
    async fn delete_material(&self, material_id: Uuid) -> Result<LessonMaterial, DomainError>;
}
