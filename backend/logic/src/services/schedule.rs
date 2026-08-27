//! Schedule-building use cases.
//!
//! Dependencies: `domain::entities::schedule_week` and
//! `domain::repositories::schedule_week_repository`.
//! Guarantees: persistence is accessed through a domain trait, so this module
//! remains independent of the PostgreSQL adapter.

use std::sync::Arc;

use chrono::{Datelike, NaiveDate, NaiveTime};
use domain::entities::homework::{Homework, HomeworkFile};
use domain::entities::lesson::Lesson;
use domain::entities::schedule_week::ScheduleWeek;
use domain::entities::{
    cabinet::Cabinet, lesson_instance::LessonInstance, subject::Subject, user::User,
};
use domain::errors::DomainError;
use domain::repositories::cabinet_repository::CabinetRepository;
use domain::repositories::homework_repository::HomeworkRepository;
use domain::repositories::lesson_instance_repository::LessonInstanceRepository;
use domain::repositories::lesson_repository::LessonRepository;
use domain::repositories::lesson_template_repository::LessonTemplateRepository;
use domain::repositories::schedule_week_repository::ScheduleWeekRepository;
use domain::repositories::subject_repository::SubjectRepository;
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::week_status::WeekStatus;

/// Coordinates schedule-week operations for presentation handlers.
///
/// The concrete repository is injected by the composition root. This keeps
/// business rules testable with an in-memory implementation.
pub struct ScheduleService {
    schedule_weeks: Arc<dyn ScheduleWeekRepository>,
    instances: Arc<dyn LessonInstanceRepository>,
    templates: Arc<dyn LessonTemplateRepository>,
    lessons: Arc<dyn LessonRepository>,
    subjects: Arc<dyn SubjectRepository>,
    users: Arc<dyn UserRepository>,
    cabinets: Arc<dyn CabinetRepository>,
    homeworks: Arc<dyn HomeworkRepository>,
}

/// A complete schedule response for the mobile/web schedule screen.
///
/// `days` always contains Monday through Sunday in chronological order.
/// Sunday is intentionally included as an empty lesson day because the domain
/// reserves it for events; this keeps the frontend calendar stable.
#[derive(Debug, Clone, PartialEq)]
pub struct WeeklySchedule {
    /// Monday that identifies this schedule week.
    pub week_start_date: NaiveDate,
    /// Calendar days in Monday-to-Sunday order.
    pub days: Vec<ScheduleDay>,
}

/// Lessons displayed for one calendar date.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleDay {
    /// Concrete calendar date.
    pub date: NaiveDate,
    /// Lessons ordered by their template start time.
    pub lessons: Vec<ScheduleLesson>,
}

/// A frontend-ready lesson with its related display data.
///
/// The original instance is retained, including its status, so cancelled
/// lessons can be rendered greyed as required by the override design.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleLesson {
    /// Concrete schedule instance and its cancellation/completion status.
    pub instance: LessonInstance,
    /// Time interval copied from the current lesson template.
    pub start_time: NaiveTime,
    /// Time interval copied from the current lesson template.
    pub end_time: NaiveTime,
    /// Abstract lesson (class/group + subject).
    pub lesson: Lesson,
    /// Subject shown in the schedule card.
    pub subject: Subject,
    /// All teachers assigned to the lesson.
    pub teachers: Vec<User>,
    /// Cabinet used by this instance, falling back to the template cabinet.
    pub cabinet: Option<Cabinet>,
    /// Homework and attached files; absent means no homework was created.
    pub homework: Option<HomeworkDetails>,
}

/// Homework data needed by a schedule card and its details screen.
#[derive(Debug, Clone, PartialEq)]
pub struct HomeworkDetails {
    /// Homework entity with status and text.
    pub homework: Homework,
    /// Files sorted according to the repository contract.
    pub files: Vec<HomeworkFile>,
}

impl ScheduleService {
    /// Creates a service with all repositories injected once at application startup.
    ///
    /// The same repository objects are reused by every request. The frontend
    /// never receives or passes these dependencies; it passes only query data
    /// such as a date to the public schedule methods.
    pub fn new(
        schedule_weeks: Arc<dyn ScheduleWeekRepository>,
        instances: Arc<dyn LessonInstanceRepository>,
        templates: Arc<dyn LessonTemplateRepository>,
        lessons: Arc<dyn LessonRepository>,
        subjects: Arc<dyn SubjectRepository>,
        users: Arc<dyn UserRepository>,
        cabinets: Arc<dyn CabinetRepository>,
        homeworks: Arc<dyn HomeworkRepository>,
    ) -> Self {
        Self {
            schedule_weeks,
            instances,
            templates,
            lessons,
            subjects,
            users,
            cabinets,
            homeworks,
        }
    }

    /// Creates a draft week, optionally recording the source week.
    ///
    /// Fail-safe: repository errors are returned unchanged and the operation
    /// never panics. Atomic persistence remains the adapter's responsibility.
    pub async fn create_draft(
        &self,
        week_start_date: NaiveDate,
        copied_from: Option<NaiveDate>,
    ) -> Result<ScheduleWeek, DomainError> {
        self.schedule_weeks
            .save(ScheduleWeek::new(
                week_start_date,
                WeekStatus::Draft,
                copied_from,
            ))
            .await
    }

    /// Publishes an existing week after loading its current state.
    ///
    /// Fail-safe: a missing week is reported as `ScheduleWeekNotFound`; the
    /// repository is not called for saving when loading fails.
    pub async fn publish(&self, week_start_date: NaiveDate) -> Result<ScheduleWeek, DomainError> {
        let mut week = self.schedule_weeks.get_by_id(week_start_date).await?;
        week.status = WeekStatus::Published;
        self.schedule_weeks.save(week).await
    }

    /// Returns all weeks in the order provided by the repository.
    pub async fn list(&self) -> Result<Vec<ScheduleWeek>, DomainError> {
        self.schedule_weeks.get_all().await
    }

    /// Loads the schedule for one concrete calendar day.
    ///
    /// Only the published week containing `date` is visible to students. The
    /// day query uses `LessonInstanceRepository::get_by_date`, so it does not
    /// load the other six days of the week. Fail-safe: a missing or draft week
    /// returns `DomainError::ScheduleWeekNotFound`; repository failures are
    /// propagated unchanged.
    pub async fn day_schedule(&self, date: NaiveDate) -> Result<ScheduleDay, DomainError> {
        let week_start_date = monday_of(date);
        let week = self.schedule_weeks.get_by_id(week_start_date).await?;
        if !week.is_published() {
            return Err(DomainError::ScheduleWeekNotFound);
        }

        self.build_day_schedule(date).await
    }

    /// Loads the schedule for the week containing `date`.
    ///
    /// The date may be any day of the week; Monday is calculated internally.
    /// Only published weeks are returned because draft weeks are invisible to
    /// students. Fail-safe: a missing or draft week returns
    /// `DomainError::ScheduleWeekNotFound` and no partial response is emitted.
    pub async fn current_week_schedule(
        &self,
        date: NaiveDate,
    ) -> Result<WeeklySchedule, DomainError> {
        let week_start_date = monday_of(date);
        let week = self.schedule_weeks.get_by_id(week_start_date).await?;
        if !week.is_published() {
            return Err(DomainError::ScheduleWeekNotFound);
        }

        self.build_week_schedule(week_start_date).await
    }

    /// Composes one frontend-ready day from the repositories stored in the service.
    async fn build_day_schedule(&self, date: NaiveDate) -> Result<ScheduleDay, DomainError> {
        let day_instances = self.instances.get_by_date(date).await?;
        let mut lessons_for_day = Vec::with_capacity(day_instances.len());

        for instance in day_instances {
            let template = self.templates.get_by_id(instance.template_id).await?;
            let lesson = self.lessons.get_by_id(template.lesson_id).await?;
            let subject = self.subjects.get_by_id(lesson.subject_id).await?;
            let teacher_ids = self.lessons.get_teacher_ids(lesson.id).await?;
            let mut teacher_list = Vec::with_capacity(teacher_ids.len());
            for teacher_id in teacher_ids {
                teacher_list.push(self.users.get_by_id(teacher_id).await?);
            }

            let cabinet_id = instance.cabinet_id.or(template.cabinet_id);
            let cabinet = match cabinet_id {
                Some(id) => Some(self.cabinets.get_by_id(id).await?),
                None => None,
            };
            let homework = load_homework(&self.homeworks, instance.id).await?;
            lessons_for_day.push(ScheduleLesson {
                instance,
                start_time: template.start_time,
                end_time: template.end_time,
                lesson,
                subject,
                teachers: teacher_list,
                cabinet,
                homework,
            });
        }

        lessons_for_day.sort_by_key(|lesson| lesson.start_time);
        Ok(ScheduleDay {
            date,
            lessons: lessons_for_day,
        })
    }

    /// Composes the complete week from the repositories stored in the service.
    async fn build_week_schedule(
        &self,
        week_start_date: NaiveDate,
    ) -> Result<WeeklySchedule, DomainError> {
        let week_instances = self.instances.get_by_week(week_start_date).await?;
        let mut days = (0..7)
            .map(|offset| ScheduleDay {
                date: week_start_date + chrono::Duration::days(offset),
                lessons: Vec::new(),
            })
            .collect::<Vec<_>>();

        for instance in week_instances {
            let day = self.build_schedule_lesson(instance).await?;
            let day_index = usize::try_from(day.instance.day_of_week())
                .map_err(|_| DomainError::InvalidLessonInstanceDate)?;
            if let Some(schedule_day) = days.get_mut(day_index) {
                schedule_day.lessons.push(day);
            } else {
                return Err(DomainError::InvalidLessonInstanceDate);
            }
        }

        for day in &mut days {
            day.lessons.sort_by_key(|lesson| lesson.start_time);
        }
        Ok(WeeklySchedule {
            week_start_date,
            days,
        })
    }

    /// Loads all related data for one schedule instance.
    async fn build_schedule_lesson(
        &self,
        instance: LessonInstance,
    ) -> Result<ScheduleLesson, DomainError> {
        let template = self.templates.get_by_id(instance.template_id).await?;
        let lesson = self.lessons.get_by_id(template.lesson_id).await?;
        let subject = self.subjects.get_by_id(lesson.subject_id).await?;
        let teacher_ids = self.lessons.get_teacher_ids(lesson.id).await?;
        let mut teacher_list = Vec::with_capacity(teacher_ids.len());
        for teacher_id in teacher_ids {
            teacher_list.push(self.users.get_by_id(teacher_id).await?);
        }

        let cabinet_id = instance.cabinet_id.or(template.cabinet_id);
        let cabinet = match cabinet_id {
            Some(id) => Some(self.cabinets.get_by_id(id).await?),
            None => None,
        };
        let homework = load_homework(&self.homeworks, instance.id).await?;
        Ok(ScheduleLesson {
            instance,
            start_time: template.start_time,
            end_time: template.end_time,
            lesson,
            subject,
            teachers: teacher_list,
            cabinet,
            homework,
        })
    }
}

/// Calculates the Monday identifying the ISO-style school week.
fn monday_of(date: NaiveDate) -> NaiveDate {
    date - chrono::Duration::days(i64::from(date.weekday().num_days_from_monday()))
}

/// Missing homework is normal for a schedule card; other storage failures are not.
async fn load_homework(
    homeworks: &Arc<dyn HomeworkRepository>,
    instance_id: uuid::Uuid,
) -> Result<Option<HomeworkDetails>, DomainError> {
    match homeworks.get_by_lesson_instance(instance_id).await {
        Ok(homework) => {
            let files = homeworks.get_files(homework.id).await?;
            Ok(Some(HomeworkDetails { homework, files }))
        }
        Err(DomainError::HomeworkNotFound) => Ok(None),
        Err(error) => Err(error),
    }
}
