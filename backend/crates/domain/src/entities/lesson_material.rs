//! Course materials: topics of a course and files / links in them.
//!
//! A course is a `Lesson` (subject + class/group). Materials are grouped by optional
//! topics; a material without a topic is shown under "Без темы".
//!
//! Invariants (mirrored by DB checks in 0011_create_lesson_materials.sql):
//! - topic name and material title: non-empty after trim, ≤ 255 chars;
//! - description: ≤ 5000 chars, empty after trim becomes `None`;
//! - a material is either a file (S3 metadata) or an http(s) link, never both;
//! - link URL: `http://` or `https://`, ≤ 2000 chars, no whitespace — so a stored
//!   link can never be a `javascript:` URL rendered into a page.
//!
//! Dependencies: `crate::errors::DomainError`, `chrono`, `uuid`.
//! Guarantees: entities are built only through `try_new`, which validates; no panics.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::errors::DomainError;

const MAX_NAME_CHARS: usize = 255;
const MAX_DESCRIPTION_CHARS: usize = 5000;
const MAX_URL_CHARS: usize = 2000;
const MAX_STORAGE_KEY_CHARS: usize = 500;
const MAX_FILE_NAME_CHARS: usize = 255;
const MAX_MIME_TYPE_CHARS: usize = 100;

fn trimmed_name(raw: &str, error: DomainError) -> Result<String, DomainError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_NAME_CHARS {
        return Err(error);
    }
    Ok(trimmed.to_string())
}

/// A topic of a course, e.g. "Пределы".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LessonMaterialTopic {
    pub id: Uuid,
    /// Course the topic belongs to (immutable).
    pub lesson_id: Uuid,
    /// 1-255 chars, unique in the course case-insensitively (DB index).
    pub name: String,
    pub sort_order: i32,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

impl LessonMaterialTopic {
    /// Returns `InvalidMaterialTopicName` for an empty or too long name; stores it trimmed.
    pub fn try_new(
        id: Uuid,
        lesson_id: Uuid,
        name: &str,
        sort_order: i32,
        created_by: Uuid,
        created_at: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id,
            lesson_id,
            name: trimmed_name(name, DomainError::InvalidMaterialTopicName)?,
            sort_order,
            created_by,
            created_at,
        })
    }
}

/// What a material is: an uploaded file or an external link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterialContent {
    /// Object in file storage; the DB keeps only metadata.
    File {
        storage_key: String,
        file_name: String,
        mime_type: String,
        size_bytes: i64,
    },
    /// http(s) URL.
    Link { url: String },
}

impl MaterialContent {
    /// Validated file content (same limits as `homework_files`).
    ///
    /// # Fail-safe поведение
    /// `InvalidHomeworkFileFormat` for empty or too long key/name/type,
    /// `InvalidHomeworkFileSize` for a negative size.
    pub fn file(
        storage_key: &str,
        file_name: &str,
        mime_type: &str,
        size_bytes: i64,
    ) -> Result<Self, DomainError> {
        let (key, name, mime) = (storage_key.trim(), file_name.trim(), mime_type.trim());
        let too_long = key.chars().count() > MAX_STORAGE_KEY_CHARS
            || name.chars().count() > MAX_FILE_NAME_CHARS
            || mime.chars().count() > MAX_MIME_TYPE_CHARS;
        if key.is_empty() || name.is_empty() || mime.is_empty() || too_long {
            return Err(DomainError::InvalidHomeworkFileFormat);
        }
        if size_bytes < 0 {
            return Err(DomainError::InvalidHomeworkFileSize);
        }
        Ok(Self::File {
            storage_key: key.to_string(),
            file_name: name.to_string(),
            mime_type: mime.to_string(),
            size_bytes,
        })
    }

    /// Validated link: http(s) scheme, a host after it, no whitespace, ≤ 2000 chars.
    ///
    /// # Fail-safe поведение
    /// `InvalidMaterialUrl` otherwise (rejects `javascript:`, `data:`, `ftp:` ...).
    pub fn link(url: &str) -> Result<Self, DomainError> {
        let url = url.trim();
        let lower = url.to_ascii_lowercase();
        let rest = lower
            .strip_prefix("https://")
            .or_else(|| lower.strip_prefix("http://"))
            .ok_or(DomainError::InvalidMaterialUrl)?;
        let has_host = rest.split(['/', '?', '#']).next().is_some_and(|host| !host.is_empty());
        if !has_host || url.chars().count() > MAX_URL_CHARS || url.chars().any(char::is_whitespace) {
            return Err(DomainError::InvalidMaterialUrl);
        }
        Ok(Self::Link {
            url: url.to_string(),
        })
    }
}

/// A material of a course: a file or a link, optionally inside a topic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LessonMaterial {
    pub id: Uuid,
    /// Course of the material (immutable).
    pub lesson_id: Uuid,
    /// `None` = "Без темы". The topic must belong to the same course (DB FK).
    pub topic_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub content: MaterialContent,
    pub sort_order: i32,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

impl LessonMaterial {
    /// Validates and normalizes title and description without building a material:
    /// trimmed title, empty description → `None`. Lets callers check the text before
    /// doing expensive work (uploading a file).
    ///
    /// # Fail-safe поведение
    /// `InvalidMaterialTitle` for an empty or too long title, or a too long description.
    pub fn normalized_text(
        title: &str,
        description: Option<&str>,
    ) -> Result<(String, Option<String>), DomainError> {
        let description = description
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string);
        if description
            .as_ref()
            .is_some_and(|text| text.chars().count() > MAX_DESCRIPTION_CHARS)
        {
            return Err(DomainError::InvalidMaterialTitle);
        }
        Ok((trimmed_name(title, DomainError::InvalidMaterialTitle)?, description))
    }

    /// Returns `InvalidMaterialTitle` for an empty or too long title, or a too long
    /// description. Title is trimmed; an empty description becomes `None`.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        id: Uuid,
        lesson_id: Uuid,
        topic_id: Option<Uuid>,
        title: &str,
        description: Option<&str>,
        content: MaterialContent,
        sort_order: i32,
        created_by: Uuid,
        created_at: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        let (title, description) = Self::normalized_text(title, description)?;
        Ok(Self {
            id,
            lesson_id,
            topic_id,
            title,
            description,
            content,
            sort_order,
            created_by,
            created_at,
        })
    }

    /// Storage key if the material is a file.
    pub fn storage_key(&self) -> Option<&str> {
        match &self.content {
            MaterialContent::File { storage_key, .. } => Some(storage_key),
            MaterialContent::Link { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_accepts_only_http_urls_with_host() {
        assert!(MaterialContent::link("https://youtu.be/abc").is_ok());
        assert!(MaterialContent::link("  http://example.com/a?b=1  ").is_ok());
        for bad in [
            "javascript:alert(1)",
            "JAVASCRIPT:alert(1)",
            "data:text/html,x",
            "ftp://example.com",
            "https://",
            "https:///path",
            "https://exa mple.com",
            "example.com",
        ] {
            assert_eq!(MaterialContent::link(bad), Err(DomainError::InvalidMaterialUrl), "{bad}");
        }
        assert_eq!(
            MaterialContent::link(&format!("https://e.com/{}", "a".repeat(2000))),
            Err(DomainError::InvalidMaterialUrl)
        );
    }

    #[test]
    fn file_content_is_validated() {
        assert!(MaterialContent::file("materials/x_a.pdf", "a.pdf", "application/pdf", 10).is_ok());
        assert_eq!(
            MaterialContent::file("", "a.pdf", "application/pdf", 10),
            Err(DomainError::InvalidHomeworkFileFormat)
        );
        assert_eq!(
            MaterialContent::file("k", "a.pdf", "application/pdf", -1),
            Err(DomainError::InvalidHomeworkFileSize)
        );
    }

    #[test]
    fn titles_and_descriptions_are_normalized() {
        let content = MaterialContent::link("https://e.com").unwrap();
        let material = LessonMaterial::try_new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            None,
            "  Конспект  ",
            Some("   "),
            content.clone(),
            0,
            Uuid::new_v4(),
            Utc::now(),
        )
        .unwrap();
        assert_eq!(material.title, "Конспект");
        assert_eq!(material.description, None);

        let empty = LessonMaterial::try_new(
            Uuid::new_v4(), Uuid::new_v4(), None, "  ", None, content, 0, Uuid::new_v4(), Utc::now(),
        );
        assert_eq!(empty, Err(DomainError::InvalidMaterialTitle));
        assert_eq!(
            LessonMaterialTopic::try_new(Uuid::new_v4(), Uuid::new_v4(), "", 0, Uuid::new_v4(), Utc::now()),
            Err(DomainError::InvalidMaterialTopicName)
        );
    }
}
