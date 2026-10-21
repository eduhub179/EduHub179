//! Course materials use cases: topics of a course and files / links inside them.
//!
//! Access (see `LessonAccess`):
//! - view: staff (teachers, admins) — any course; students — courses they attend;
//! - manage (topics, upload, edit, delete): admin or a teacher of the course.
//!
//! Materials are visible right after they are added (no drafts).
//!
//! Files go to `FileStorage` under `materials/`; the DB keeps metadata. Upload is
//! compensated: if saving the record fails, the uploaded object is deleted.
//!
//! Dependencies: `domain` (repositories, ports), `LessonAccess`.
//! Guarantees: all methods return `Result`, no panics.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use domain::{
    entities::{
        lesson::Lesson,
        lesson_material::{LessonMaterial, LessonMaterialTopic, MaterialContent},
        user::User,
    },
    errors::DomainError,
    ports::file_storage::{FileStorage, StorageFolder, UploadFileRequest},
    repositories::lesson_material_repository::{LessonMaterialRepository, MaterialTopicFilter},
};
use uuid::Uuid;

use crate::use_cases::homework::upload_file::UploadHomeworkFileCommand;
use crate::use_cases::lesson_access::LessonAccess;

/// A course as shown on the materials page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseSummary {
    pub lesson_id: Uuid,
    pub subject_name: String,
    pub target_name: String,
    /// Whether the current user may add / edit / delete materials (UI shows the buttons).
    pub can_manage: bool,
}

/// A topic with the number of its materials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicWithCount {
    pub topic: LessonMaterialTopic,
    pub material_count: i64,
}

/// The topic sidebar of a course, loaded in two queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicsOverview {
    pub topics: Vec<TopicWithCount>,
    /// Materials with no topic ("Без темы").
    pub without_topic_count: i64,
    pub total_count: i64,
}

/// Input for adding a link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddLinkCommand {
    pub lesson_id: Uuid,
    pub topic_id: Option<Uuid>,
    pub title: String,
    pub url: String,
    pub description: Option<String>,
}

/// Input for uploading a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadMaterialCommand {
    pub lesson_id: Uuid,
    pub topic_id: Option<Uuid>,
    /// Defaults to the file name.
    pub title: Option<String>,
    pub description: Option<String>,
    pub file: UploadHomeworkFileCommand,
}

/// Partial update: `None` keeps the value; `Some(None)` clears description / topic.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UpdateMaterialCommand {
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub topic_id: Option<Option<Uuid>>,
    pub sort_order: Option<i32>,
}

/// Result of a delete; the caller logs `orphaned_storage_key` for manual cleanup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedMaterial {
    pub material: LessonMaterial,
    pub orphaned_storage_key: Option<String>,
}

/// A downloaded file: metadata for the response headers and the bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedMaterial {
    pub material: LessonMaterial,
    pub file_name: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

pub struct MaterialsService {
    materials: Arc<dyn LessonMaterialRepository>,
    access: LessonAccess,
    /// `None` if S3 is not configured: links work, files answer `FileStorageUnavailable`.
    storage: Option<Arc<dyn FileStorage>>,
}

impl MaterialsService {
    pub fn new(
        materials: Arc<dyn LessonMaterialRepository>,
        access: LessonAccess,
        storage: Option<Arc<dyn FileStorage>>,
    ) -> Self {
        Self {
            materials,
            access,
            storage,
        }
    }

    // ========================================================================
    // Courses
    // ========================================================================

    /// Courses of a class for the materials page, sorted by subject.
    /// Staff get every course of the class (with `can_manage`); a student only the
    /// courses they attend.
    ///
    /// # Fail-safe поведение
    /// `ClassNotFound` for an unknown class.
    pub async fn class_courses(&self, user: &User, class_id: Uuid) -> Result<Vec<CourseSummary>, DomainError> {
        let mut courses = Vec::new();
        for lesson in self.access.class_lessons(class_id).await? {
            if !lesson.is_active {
                continue;
            }
            if user.role.is_student() && !self.access.is_in_audience(user.id, &lesson).await? {
                continue;
            }
            courses.push(self.summary(user, &lesson).await?);
        }
        courses.sort_by(|a, b| (&a.subject_name, &a.target_name).cmp(&(&b.subject_name, &b.target_name)));
        Ok(courses)
    }

    /// One course (page header). `InsufficientPermissions` if the user may not view it.
    pub async fn course(&self, user: &User, lesson_id: Uuid) -> Result<CourseSummary, DomainError> {
        let lesson = self.require_viewer(user, lesson_id).await?;
        self.summary(user, &lesson).await
    }

    // ========================================================================
    // Topics
    // ========================================================================

    /// Topics with material counts: one query for topics, one `GROUP BY` for counts.
    pub async fn topics(&self, user: &User, lesson_id: Uuid) -> Result<TopicsOverview, DomainError> {
        self.require_viewer(user, lesson_id).await?;
        let counts: HashMap<Option<Uuid>, i64> = self
            .materials
            .count_materials_by_topic(lesson_id)
            .await?
            .into_iter()
            .collect();
        let topics = self
            .materials
            .get_topics(lesson_id)
            .await?
            .into_iter()
            .map(|topic| TopicWithCount {
                material_count: counts.get(&Some(topic.id)).copied().unwrap_or(0),
                topic,
            })
            .collect();
        Ok(TopicsOverview {
            topics,
            without_topic_count: counts.get(&None).copied().unwrap_or(0),
            total_count: counts.values().sum(),
        })
    }

    /// Appends a topic to the course.
    /// `MaterialTopicAlreadyExists` if the name is taken (case-insensitive).
    pub async fn create_topic(&self, user: &User, lesson_id: Uuid, name: &str) -> Result<LessonMaterialTopic, DomainError> {
        self.access.get_lesson(lesson_id).await?;
        self.access.require_manager(user, lesson_id).await?;
        let sort_order = next_order(self.materials.get_topics(lesson_id).await?.iter().map(|t| t.sort_order));
        let topic = LessonMaterialTopic::try_new(Uuid::new_v4(), lesson_id, name, sort_order, user.id, Utc::now())?;
        self.materials.save_topic(topic).await
    }

    /// Renames and/or moves a topic.
    pub async fn update_topic(
        &self,
        user: &User,
        topic_id: Uuid,
        name: Option<&str>,
        sort_order: Option<i32>,
    ) -> Result<LessonMaterialTopic, DomainError> {
        let topic = self.materials.get_topic(topic_id).await?;
        self.access.require_manager(user, topic.lesson_id).await?;
        let updated = LessonMaterialTopic::try_new(
            topic.id,
            topic.lesson_id,
            name.unwrap_or(&topic.name),
            sort_order.unwrap_or(topic.sort_order),
            topic.created_by,
            topic.created_at,
        )?;
        self.materials.save_topic(updated).await
    }

    /// Deletes a topic; its materials move to "Без темы" and are not lost.
    pub async fn delete_topic(&self, user: &User, topic_id: Uuid) -> Result<(), DomainError> {
        let topic = self.materials.get_topic(topic_id).await?;
        self.access.require_manager(user, topic.lesson_id).await?;
        self.materials.delete_topic(topic_id).await
    }

    // ========================================================================
    // Materials
    // ========================================================================

    /// Materials of a course by topic filter.
    /// `MaterialTopicNotFound` if the topic belongs to another course.
    pub async fn materials(
        &self,
        user: &User,
        lesson_id: Uuid,
        filter: MaterialTopicFilter,
    ) -> Result<Vec<LessonMaterial>, DomainError> {
        self.require_viewer(user, lesson_id).await?;
        if let MaterialTopicFilter::Topic(topic_id) = filter {
            self.require_topic_of(topic_id, lesson_id).await?;
        }
        self.materials.get_materials(lesson_id, filter).await
    }

    /// Adds an http(s) link to the end of its topic.
    pub async fn add_link(&self, user: &User, cmd: AddLinkCommand) -> Result<LessonMaterial, DomainError> {
        self.prepare_add(user, cmd.lesson_id, cmd.topic_id).await?;
        let content = MaterialContent::link(&cmd.url)?;
        let material = self
            .new_material(user, cmd.lesson_id, cmd.topic_id, &cmd.title, cmd.description.as_deref(), content)
            .await?;
        self.materials.save_material(material).await
    }

    /// Uploads a file and adds it to the end of its topic. Title defaults to the file name.
    ///
    /// # Fail-safe поведение
    /// The file is validated (name, type, size ≤ 100 MB) before anything is uploaded;
    /// if the record cannot be saved, the uploaded object is deleted.
    /// `FileStorageUnavailable` if S3 is not configured.
    pub async fn upload_file(&self, user: &User, cmd: UploadMaterialCommand) -> Result<LessonMaterial, DomainError> {
        self.prepare_add(user, cmd.lesson_id, cmd.topic_id).await?;
        let storage = self.storage.as_ref().ok_or(DomainError::FileStorageUnavailable)?;
        cmd.file.validate()?;
        let title = cmd
            .title
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| cmd.file.file_name.trim().to_string());
        // Check title/description before uploading, so a bad title leaves no object behind.
        LessonMaterial::normalized_text(&title, cmd.description.as_deref())?;

        let stored = storage
            .upload(UploadFileRequest {
                folder: StorageFolder::Materials,
                file_name: cmd.file.file_name.trim().to_string(),
                mime_type: cmd.file.mime_type.trim().to_string(),
                bytes: cmd.file.bytes,
            })
            .await?;
        let saved = async {
            let content =
                MaterialContent::file(&stored.storage_key, &stored.file_name, &stored.mime_type, stored.size_bytes)?;
            let material = self
                .new_material(user, cmd.lesson_id, cmd.topic_id, &title, cmd.description.as_deref(), content)
                .await?;
            self.materials.save_material(material).await
        }
        .await;
        if saved.is_err() {
            // Best effort: a failed cleanup leaves an orphaned object, not a broken record.
            let _ = storage.delete(&stored.storage_key).await;
        }
        saved
    }

    /// Renames, re-describes, moves to another topic (or to "Без темы"), reorders.
    pub async fn update_material(
        &self,
        user: &User,
        material_id: Uuid,
        cmd: UpdateMaterialCommand,
    ) -> Result<LessonMaterial, DomainError> {
        let material = self.materials.get_material(material_id).await?;
        self.access.require_manager(user, material.lesson_id).await?;
        let topic_id = cmd.topic_id.unwrap_or(material.topic_id);
        if let Some(topic_id) = topic_id {
            self.require_topic_of(topic_id, material.lesson_id).await?;
        }
        let description = match cmd.description {
            Some(new) => new,
            None => material.description.clone(),
        };
        let updated = LessonMaterial::try_new(
            material.id,
            material.lesson_id,
            topic_id,
            cmd.title.as_deref().unwrap_or(&material.title),
            description.as_deref(),
            material.content.clone(),
            cmd.sort_order.unwrap_or(material.sort_order),
            material.created_by,
            material.created_at,
        )?;
        self.materials.save_material(updated).await
    }

    /// Deletes a material; a file object is removed from storage after the record.
    pub async fn delete_material(&self, user: &User, material_id: Uuid) -> Result<DeletedMaterial, DomainError> {
        let material = self.materials.get_material(material_id).await?;
        self.access.require_manager(user, material.lesson_id).await?;
        let material = self.materials.delete_material(material_id).await?;

        let orphaned_storage_key = match (material.storage_key(), &self.storage) {
            (None, _) => None,
            (Some(key), Some(storage)) => storage.delete(key).await.err().map(|_| key.to_string()),
            (Some(key), None) => Some(key.to_string()),
        };
        Ok(DeletedMaterial {
            material,
            orphaned_storage_key,
        })
    }

    /// File contents of a material.
    /// `MaterialIsNotAFile` for links; `MaterialNotFound` if the object is gone.
    pub async fn download(&self, user: &User, material_id: Uuid) -> Result<DownloadedMaterial, DomainError> {
        let material = self.materials.get_material(material_id).await?;
        self.require_viewer(user, material.lesson_id).await?;
        let MaterialContent::File {
            storage_key,
            file_name,
            mime_type,
            ..
        } = material.content.clone()
        else {
            return Err(DomainError::MaterialIsNotAFile);
        };
        let storage = self.storage.as_ref().ok_or(DomainError::FileStorageUnavailable)?;
        let bytes = match storage.download(&storage_key).await {
            Err(DomainError::HomeworkFileNotFound) => return Err(DomainError::MaterialNotFound),
            other => other?,
        };
        Ok(DownloadedMaterial {
            material,
            file_name,
            mime_type,
            bytes,
        })
    }

    // ========================================================================
    // Helpers
    // ========================================================================

    /// Staff may view any course; a student only a course they attend.
    async fn require_viewer(&self, user: &User, lesson_id: Uuid) -> Result<Lesson, DomainError> {
        let lesson = self.access.get_lesson(lesson_id).await?;
        let allowed = !user.role.is_student() || self.access.is_in_audience(user.id, &lesson).await?;
        if allowed {
            Ok(lesson)
        } else {
            Err(DomainError::InsufficientPermissions)
        }
    }

    async fn require_topic_of(&self, topic_id: Uuid, lesson_id: Uuid) -> Result<(), DomainError> {
        match self.materials.get_topic(topic_id).await {
            Ok(topic) if topic.lesson_id == lesson_id => Ok(()),
            Ok(_) => Err(DomainError::MaterialTopicNotFound),
            Err(error) => Err(error),
        }
    }

    async fn prepare_add(&self, user: &User, lesson_id: Uuid, topic_id: Option<Uuid>) -> Result<(), DomainError> {
        self.access.get_lesson(lesson_id).await?;
        self.access.require_manager(user, lesson_id).await?;
        if let Some(topic_id) = topic_id {
            self.require_topic_of(topic_id, lesson_id).await?;
        }
        Ok(())
    }

    /// A new material at the end of its topic.
    async fn new_material(
        &self,
        user: &User,
        lesson_id: Uuid,
        topic_id: Option<Uuid>,
        title: &str,
        description: Option<&str>,
        content: MaterialContent,
    ) -> Result<LessonMaterial, DomainError> {
        let filter = topic_id.map_or(MaterialTopicFilter::WithoutTopic, MaterialTopicFilter::Topic);
        let sort_order = next_order(
            self.materials
                .get_materials(lesson_id, filter)
                .await?
                .iter()
                .map(|material| material.sort_order),
        );
        LessonMaterial::try_new(
            Uuid::new_v4(),
            lesson_id,
            topic_id,
            title,
            description,
            content,
            sort_order,
            user.id,
            Utc::now(),
        )
    }

    async fn summary(&self, user: &User, lesson: &Lesson) -> Result<CourseSummary, DomainError> {
        let names = self.access.names(lesson).await?;
        Ok(CourseSummary {
            lesson_id: lesson.id,
            subject_name: names.subject_name,
            target_name: names.target_name,
            can_manage: self.access.can_manage(user, lesson.id).await?,
        })
    }
}

/// Position after the last one (0 for an empty list).
fn next_order(orders: impl Iterator<Item = i32>) -> i32 {
    orders.max().map_or(0, |last| last.saturating_add(1))
}
