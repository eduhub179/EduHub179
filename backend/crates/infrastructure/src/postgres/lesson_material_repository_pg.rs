//! PostgreSQL implementation of `LessonMaterialRepository` (course topics and materials).
//!
//! Dependencies: `sqlx`, tables from 0011_create_lesson_materials.sql.
//! Guarantees: DB errors are mapped to `DomainError`, no panics; rows that violate
//! domain invariants (corrupted data) surface as errors, not as invalid entities.

use chrono::{DateTime, Utc};
use domain::entities::lesson_material::{LessonMaterial, LessonMaterialTopic, MaterialContent};
use domain::errors::DomainError;
use domain::repositories::lesson_material_repository::{
    LessonMaterialRepository, MaterialTopicFilter,
};
use sqlx::PgPool;
use uuid::Uuid;

pub struct LessonMaterialRepositoryPg {
    pool: PgPool,
}

impl LessonMaterialRepositoryPg {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Maps constraint violations to domain errors; `not_found` is used for a missing row.
    fn map_err(err: sqlx::Error, not_found: DomainError) -> DomainError {
        match err {
            sqlx::Error::RowNotFound => not_found,
            sqlx::Error::Database(db_err) => match (db_err.code().as_deref(), db_err.constraint()) {
                // 23505 unique_violation: topic name already used in the course.
                (Some("23505"), Some("idx_lesson_material_topics_name")) => {
                    DomainError::MaterialTopicAlreadyExists
                }
                // 23503 foreign_key_violation on INSERT/UPDATE.
                (Some("23503"), Some("fk_lesson_materials_topic")) => {
                    DomainError::MaterialTopicNotFound
                }
                (Some("23503"), Some(name)) if name.ends_with("_lesson_id_fkey") => {
                    DomainError::LessonNotFound
                }
                (Some("23503"), Some(name)) if name.ends_with("_created_by_fkey") => {
                    DomainError::UserNotFound
                }
                // 23514 check_violation: data that passed the domain but not the DB.
                (Some("23514"), Some("chk_lesson_materials_one_content")) => {
                    DomainError::InvalidMaterialUrl
                }
                (Some("23514"), _) => DomainError::InvalidMaterialTitle,
                _ => DomainError::InternalError,
            },
            _ => DomainError::InternalError,
        }
    }
}

const TOPIC_COLUMNS: &str = "topic_id, lesson_id, name, sort_order, created_by, created_at";
const MATERIAL_COLUMNS: &str = "material_id, lesson_id, topic_id, title, description, storage_key, \
     file_name, mime_type, size_bytes, url, sort_order, created_by, created_at";

#[derive(sqlx::FromRow)]
struct TopicRow {
    topic_id: Uuid,
    lesson_id: Uuid,
    name: String,
    sort_order: i32,
    created_by: Uuid,
    created_at: DateTime<Utc>,
}

impl TopicRow {
    fn into_domain(self) -> Result<LessonMaterialTopic, DomainError> {
        LessonMaterialTopic::try_new(
            self.topic_id,
            self.lesson_id,
            &self.name,
            self.sort_order,
            self.created_by,
            self.created_at,
        )
    }
}

#[derive(sqlx::FromRow)]
struct MaterialRow {
    material_id: Uuid,
    lesson_id: Uuid,
    topic_id: Option<Uuid>,
    title: String,
    description: Option<String>,
    storage_key: Option<String>,
    file_name: Option<String>,
    mime_type: Option<String>,
    size_bytes: Option<i64>,
    url: Option<String>,
    sort_order: i32,
    created_by: Uuid,
    created_at: DateTime<Utc>,
}

impl MaterialRow {
    fn into_domain(self) -> Result<LessonMaterial, DomainError> {
        let content = match (self.url, self.storage_key, self.file_name, self.mime_type, self.size_bytes) {
            (Some(url), None, None, None, None) => MaterialContent::link(&url)?,
            (None, Some(key), Some(name), Some(mime), Some(size)) => {
                MaterialContent::file(&key, &name, &mime, size)?
            }
            // Excluded by chk_lesson_materials_one_content; treat as corrupted data.
            _ => return Err(DomainError::InternalError),
        };
        LessonMaterial::try_new(
            self.material_id,
            self.lesson_id,
            self.topic_id,
            &self.title,
            self.description.as_deref(),
            content,
            self.sort_order,
            self.created_by,
            self.created_at,
        )
    }
}

#[async_trait::async_trait]
impl LessonMaterialRepository for LessonMaterialRepositoryPg {
    async fn get_topics(&self, lesson_id: Uuid) -> Result<Vec<LessonMaterialTopic>, DomainError> {
        let rows = sqlx::query_as::<_, TopicRow>(&format!(
            "SELECT {TOPIC_COLUMNS} FROM lesson_material_topics
             WHERE lesson_id = $1 ORDER BY sort_order, lower(name)"
        ))
        .bind(lesson_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| Self::map_err(err, DomainError::MaterialTopicNotFound))?;
        rows.into_iter().map(TopicRow::into_domain).collect()
    }

    async fn get_topic(&self, topic_id: Uuid) -> Result<LessonMaterialTopic, DomainError> {
        sqlx::query_as::<_, TopicRow>(&format!(
            "SELECT {TOPIC_COLUMNS} FROM lesson_material_topics WHERE topic_id = $1"
        ))
        .bind(topic_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| Self::map_err(err, DomainError::MaterialTopicNotFound))?
        .into_domain()
    }

    async fn save_topic(&self, topic: LessonMaterialTopic) -> Result<LessonMaterialTopic, DomainError> {
        sqlx::query_as::<_, TopicRow>(&format!(
            "INSERT INTO lesson_material_topics ({TOPIC_COLUMNS})
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (topic_id) DO UPDATE
                 SET name = EXCLUDED.name, sort_order = EXCLUDED.sort_order
             RETURNING {TOPIC_COLUMNS}"
        ))
        .bind(topic.id)
        .bind(topic.lesson_id)
        .bind(&topic.name)
        .bind(topic.sort_order)
        .bind(topic.created_by)
        .bind(topic.created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| Self::map_err(err, DomainError::MaterialTopicNotFound))?
        .into_domain()
    }

    async fn delete_topic(&self, topic_id: Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM lesson_material_topics WHERE topic_id = $1")
            .bind(topic_id)
            .execute(&self.pool)
            .await
            .map_err(|err| Self::map_err(err, DomainError::MaterialTopicNotFound))?;
        if result.rows_affected() == 0 {
            return Err(DomainError::MaterialTopicNotFound);
        }
        Ok(())
    }

    async fn count_materials_by_topic(
        &self,
        lesson_id: Uuid,
    ) -> Result<Vec<(Option<Uuid>, i64)>, DomainError> {
        sqlx::query_as::<_, (Option<Uuid>, i64)>(
            "SELECT topic_id, COUNT(*) FROM lesson_materials WHERE lesson_id = $1 GROUP BY topic_id",
        )
        .bind(lesson_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| Self::map_err(err, DomainError::MaterialNotFound))
    }

    async fn get_materials(
        &self,
        lesson_id: Uuid,
        filter: MaterialTopicFilter,
    ) -> Result<Vec<LessonMaterial>, DomainError> {
        let condition = match filter {
            MaterialTopicFilter::All => "",
            MaterialTopicFilter::WithoutTopic => "AND topic_id IS NULL",
            MaterialTopicFilter::Topic(_) => "AND topic_id = $2",
        };
        let sql = format!(
            "SELECT {MATERIAL_COLUMNS} FROM lesson_materials
             WHERE lesson_id = $1 {condition} ORDER BY sort_order, created_at"
        );
        let mut query = sqlx::query_as::<_, MaterialRow>(&sql).bind(lesson_id);
        if let MaterialTopicFilter::Topic(topic_id) = filter {
            query = query.bind(topic_id);
        }
        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|err| Self::map_err(err, DomainError::MaterialNotFound))?;
        rows.into_iter().map(MaterialRow::into_domain).collect()
    }

    async fn get_material(&self, material_id: Uuid) -> Result<LessonMaterial, DomainError> {
        sqlx::query_as::<_, MaterialRow>(&format!(
            "SELECT {MATERIAL_COLUMNS} FROM lesson_materials WHERE material_id = $1"
        ))
        .bind(material_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| Self::map_err(err, DomainError::MaterialNotFound))?
        .into_domain()
    }

    async fn save_material(&self, material: LessonMaterial) -> Result<LessonMaterial, DomainError> {
        let (storage_key, file_name, mime_type, size_bytes, url) = match &material.content {
            MaterialContent::File {
                storage_key,
                file_name,
                mime_type,
                size_bytes,
            } => (
                Some(storage_key.as_str()),
                Some(file_name.as_str()),
                Some(mime_type.as_str()),
                Some(*size_bytes),
                None,
            ),
            MaterialContent::Link { url } => (None, None, None, None, Some(url.as_str())),
        };
        sqlx::query_as::<_, MaterialRow>(&format!(
            "INSERT INTO lesson_materials ({MATERIAL_COLUMNS})
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
             ON CONFLICT (material_id) DO UPDATE
                 SET topic_id = EXCLUDED.topic_id, title = EXCLUDED.title,
                     description = EXCLUDED.description, sort_order = EXCLUDED.sort_order
             RETURNING {MATERIAL_COLUMNS}"
        ))
        .bind(material.id)
        .bind(material.lesson_id)
        .bind(material.topic_id)
        .bind(&material.title)
        .bind(material.description.as_deref())
        .bind(storage_key)
        .bind(file_name)
        .bind(mime_type)
        .bind(size_bytes)
        .bind(url)
        .bind(material.sort_order)
        .bind(material.created_by)
        .bind(material.created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| Self::map_err(err, DomainError::MaterialNotFound))?
        .into_domain()
    }

    async fn delete_material(&self, material_id: Uuid) -> Result<LessonMaterial, DomainError> {
        sqlx::query_as::<_, MaterialRow>(&format!(
            "DELETE FROM lesson_materials WHERE material_id = $1 RETURNING {MATERIAL_COLUMNS}"
        ))
        .bind(material_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| Self::map_err(err, DomainError::MaterialNotFound))?
        .ok_or(DomainError::MaterialNotFound)?
        .into_domain()
    }
}
