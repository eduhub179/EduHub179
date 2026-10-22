//! Use case: tell the students of a lesson that its homework appeared or changed.
//!
//! Recipients: active students of the lesson's class or group
//! (`ResolveHomeworkRecipientsUseCase`), except the author of the change.
//! Text: "Новое ДЗ · Алгебра" / "ДЗ изменено · Алгебра" + date and a snippet or file name.
//! All notifications of one homework share a tag, so a later change replaces the earlier
//! notification on the device instead of piling up.
//!
//! Dependencies: repositories, `NotificationSender` (Web Push in production).
//! Guarantees: returns `Result`, no panics; a failure for one recipient does not stop
//! the others — it is counted in the summary.

use std::collections::HashMap;
use std::sync::Arc;

use domain::errors::DomainError;
use domain::ports::notification_sender::{NotificationPayload, NotificationSender};
use domain::repositories::homework_repository::HomeworkRepository;
use domain::repositories::lesson_instance_repository::LessonInstanceRepository;
use domain::repositories::lesson_repository::LessonRepository;
use domain::repositories::lesson_template_repository::LessonTemplateRepository;
use domain::repositories::subject_repository::SubjectRepository;
use uuid::Uuid;

use super::resolve_homework_recipients::ResolveHomeworkRecipientsUseCase;

/// Body snippet length: enough for "§12, № 5–7, 9*" style homework, short on a lock screen.
const SNIPPET_CHARS: usize = 120;

/// What happened to the homework.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HomeworkChange {
    Created,
    TextUpdated,
    FileAdded { file_name: String },
    FileRemoved { file_name: String },
    /// The homework was deleted (explicitly, or with its last file when it had no text).
    Deleted,
}

/// How many students were notified and how many sends failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NotifySummary {
    pub recipients: usize,
    pub failed: usize,
}

pub struct NotifyHomeworkChangedUseCase {
    homeworks: Arc<dyn HomeworkRepository>,
    instances: Arc<dyn LessonInstanceRepository>,
    templates: Arc<dyn LessonTemplateRepository>,
    lessons: Arc<dyn LessonRepository>,
    subjects: Arc<dyn SubjectRepository>,
    recipients: ResolveHomeworkRecipientsUseCase,
    sender: Arc<dyn NotificationSender>,
}

impl NotifyHomeworkChangedUseCase {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        homeworks: Arc<dyn HomeworkRepository>,
        instances: Arc<dyn LessonInstanceRepository>,
        templates: Arc<dyn LessonTemplateRepository>,
        lessons: Arc<dyn LessonRepository>,
        subjects: Arc<dyn SubjectRepository>,
        recipients: ResolveHomeworkRecipientsUseCase,
        sender: Arc<dyn NotificationSender>,
    ) -> Self {
        Self {
            homeworks,
            instances,
            templates,
            lessons,
            subjects,
            recipients,
            sender,
        }
    }

    /// Notifies about a change of an existing homework (its text goes into the body).
    ///
    /// # Fail-safe поведение
    /// `Err` only if the homework / lesson data cannot be loaded; per-recipient send
    /// failures are counted in `NotifySummary::failed`.
    pub async fn execute(
        &self,
        homework_id: Uuid,
        change: HomeworkChange,
        author_id: Option<Uuid>,
    ) -> Result<NotifySummary, DomainError> {
        let homework = self.homeworks.get_by_id(homework_id).await?;
        self.notify(
            homework.lesson_instance_id,
            homework_id,
            homework.text_content.as_deref(),
            change,
            author_id,
        )
        .await
    }

    /// Notifies about a change after which the homework may no longer exist (a file was
    /// removed, the homework was deleted): everything is resolved from the lesson.
    pub async fn execute_for_lesson(
        &self,
        lesson_instance_id: Uuid,
        homework_id: Uuid,
        change: HomeworkChange,
        author_id: Option<Uuid>,
    ) -> Result<NotifySummary, DomainError> {
        self.notify(lesson_instance_id, homework_id, None, change, author_id).await
    }

    async fn notify(
        &self,
        lesson_instance_id: Uuid,
        homework_id: Uuid,
        text: Option<&str>,
        change: HomeworkChange,
        author_id: Option<Uuid>,
    ) -> Result<NotifySummary, DomainError> {
        let instance = self.instances.get_by_id(lesson_instance_id).await?;
        let template = self.templates.get_by_id(instance.template_id).await?;
        let lesson = self.lessons.get_by_id(template.lesson_id).await?;
        let subject = self.subjects.get_by_id(lesson.subject_id).await?.name;

        let date = instance.lesson_date.format("%d.%m");
        let (title, body) = match &change {
            HomeworkChange::Created => (format!("Новое ДЗ · {subject}"), with_text(&date.to_string(), text)),
            HomeworkChange::TextUpdated => (format!("ДЗ изменено · {subject}"), with_text(&date.to_string(), text)),
            HomeworkChange::FileAdded { file_name } => (
                format!("ДЗ изменено · {subject}"),
                format!("К уроку {date}: добавлен файл «{file_name}»"),
            ),
            HomeworkChange::FileRemoved { file_name } => (
                format!("ДЗ изменено · {subject}"),
                format!("К уроку {date}: удалён файл «{file_name}»"),
            ),
            HomeworkChange::Deleted => (
                format!("ДЗ удалено · {subject}"),
                format!("К уроку {date} задание отменено"),
            ),
        };
        let data = HashMap::from([
            ("url".to_string(), "/main".to_string()),
            // One tag per homework: a later change replaces the earlier notification.
            ("tag".to_string(), format!("homework-{homework_id}")),
            ("homework_id".to_string(), homework_id.to_string()),
            ("lesson_instance_id".to_string(), instance.id.to_string()),
        ]);

        let mut summary = NotifySummary::default();
        for user_id in self.recipients.for_lesson_instance(lesson_instance_id).await? {
            if Some(user_id) == author_id {
                continue;
            }
            summary.recipients += 1;
            let payload = NotificationPayload {
                user_id,
                title: title.clone(),
                body: body.clone(),
                data: data.clone(),
            };
            if self.sender.send_to_user(user_id, &payload).await.is_err() {
                summary.failed += 1;
            }
        }
        Ok(summary)
    }
}

/// "К уроку 21.10: §12, № 5–7", or just "К уроку 21.10" for file-only homework.
fn with_text(date: &str, text: Option<&str>) -> String {
    match text {
        Some(text) => format!("К уроку {date}: {}", snippet(text)),
        None => format!("К уроку {date}"),
    }
}

/// First line-ish of the homework text, cut on a char boundary with an ellipsis.
fn snippet(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= SNIPPET_CHARS {
        flat
    } else {
        let cut: String = flat.chars().take(SNIPPET_CHARS - 1).collect();
        format!("{}…", cut.trim_end())
    }
}

#[cfg(test)]
mod tests {
    use super::snippet;

    #[test]
    fn snippet_flattens_and_cuts() {
        assert_eq!(snippet("§12,\n  № 5–7"), "§12, № 5–7");
        let long = snippet(&"слово ".repeat(100));
        assert_eq!(long.chars().count(), 120);
        assert!(long.ends_with('…'));
    }
}
