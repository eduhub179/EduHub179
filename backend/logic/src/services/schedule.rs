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

// ============================================================================
// UNIT TESTS
// Запуск: `cargo test -p logic schedule`
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::NaiveDate;
    use std::sync::Mutex;

    struct MockScheduleWeekRepository {
        weeks: Mutex<Vec<ScheduleWeek>>,
    }

    impl MockScheduleWeekRepository {
        fn new(weeks: Vec<ScheduleWeek>) -> Self {
            Self {
                weeks: Mutex::new(weeks),
            }
        }
    }

    struct UnusedScheduleRepository;

    #[async_trait]
    impl LessonInstanceRepository for UnusedScheduleRepository {
        async fn get_by_id(&self, _: uuid::Uuid) -> Result<LessonInstance, DomainError> {
            unimplemented!()
        }
        async fn get_by_week(&self, _: NaiveDate) -> Result<Vec<LessonInstance>, DomainError> {
            unimplemented!()
        }
        async fn get_by_date(&self, _: NaiveDate) -> Result<Vec<LessonInstance>, DomainError> {
            unimplemented!()
        }
        async fn get_by_template(&self, _: uuid::Uuid) -> Result<Vec<LessonInstance>, DomainError> {
            unimplemented!()
        }
        async fn save(&self, _: LessonInstance) -> Result<LessonInstance, DomainError> {
            unimplemented!()
        }
    }

    #[async_trait]
    impl LessonTemplateRepository for UnusedScheduleRepository {
        async fn get_by_id(
            &self,
            _: uuid::Uuid,
        ) -> Result<domain::entities::lesson_template::LessonTemplate, DomainError> {
            unimplemented!()
        }
        async fn get_by_lesson(
            &self,
            _: uuid::Uuid,
        ) -> Result<Vec<domain::entities::lesson_template::LessonTemplate>, DomainError> {
            unimplemented!()
        }
        async fn get_active_for_day(
            &self,
            _: domain::value_objects::day_of_week::DayOfWeek,
        ) -> Result<Vec<domain::entities::lesson_template::LessonTemplate>, DomainError> {
            unimplemented!()
        }
        async fn get_all_active(
            &self,
        ) -> Result<Vec<domain::entities::lesson_template::LessonTemplate>, DomainError> {
            unimplemented!()
        }
        async fn save(
            &self,
            _: domain::entities::lesson_template::LessonTemplate,
        ) -> Result<domain::entities::lesson_template::LessonTemplate, DomainError> {
            unimplemented!()
        }
    }

    #[async_trait]
    impl LessonRepository for UnusedScheduleRepository {
        async fn get_by_id(&self, _: uuid::Uuid) -> Result<Lesson, DomainError> {
            unimplemented!()
        }
        async fn save(&self, _: Lesson) -> Result<Lesson, DomainError> {
            unimplemented!()
        }
        async fn get_by_class(&self, _: uuid::Uuid) -> Result<Vec<Lesson>, DomainError> {
            unimplemented!()
        }
        async fn get_by_group(&self, _: uuid::Uuid) -> Result<Vec<Lesson>, DomainError> {
            unimplemented!()
        }
        async fn get_by_teacher(&self, _: uuid::Uuid) -> Result<Vec<Lesson>, DomainError> {
            unimplemented!()
        }
        async fn assign_teacher(&self, _: uuid::Uuid, _: uuid::Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn unassign_teacher(&self, _: uuid::Uuid, _: uuid::Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn get_teacher_ids(&self, _: uuid::Uuid) -> Result<Vec<uuid::Uuid>, DomainError> {
            unimplemented!()
        }
    }

    #[async_trait]
    impl SubjectRepository for UnusedScheduleRepository {
        async fn get_by_id(&self, _: uuid::Uuid) -> Result<Subject, DomainError> {
            unimplemented!()
        }
        async fn get_all(&self) -> Result<Vec<Subject>, DomainError> {
            unimplemented!()
        }
        async fn save(&self, _: Subject) -> Result<Subject, DomainError> {
            unimplemented!()
        }
    }

    #[async_trait]
    impl UserRepository for UnusedScheduleRepository {
        async fn get_by_id(&self, _: uuid::Uuid) -> Result<User, DomainError> {
            unimplemented!()
        }
        async fn get_by_email(&self, _: &str) -> Result<User, DomainError> {
            unimplemented!()
        }
        async fn get_active_students_by_class(
            &self,
            _: uuid::Uuid,
        ) -> Result<Vec<User>, DomainError> {
            unimplemented!()
        }
        async fn save(&self, _: User) -> Result<User, DomainError> {
            unimplemented!()
        }
    }

    #[async_trait]
    impl CabinetRepository for UnusedScheduleRepository {
        async fn get_by_id(&self, _: uuid::Uuid) -> Result<Cabinet, DomainError> {
            unimplemented!()
        }
        async fn get_by_number(&self, _: i32) -> Result<Cabinet, DomainError> {
            unimplemented!()
        }
        async fn get_all(&self) -> Result<Vec<Cabinet>, DomainError> {
            unimplemented!()
        }
        async fn get_by_floor(&self, _: i32) -> Result<Vec<Cabinet>, DomainError> {
            unimplemented!()
        }
        async fn save(&self, _: Cabinet) -> Result<Cabinet, DomainError> {
            unimplemented!()
        }
    }

    #[async_trait]
    impl HomeworkRepository for UnusedScheduleRepository {
        async fn get_by_id(&self, _: uuid::Uuid) -> Result<Homework, DomainError> {
            unimplemented!()
        }
        async fn get_by_lesson_instance(&self, _: uuid::Uuid) -> Result<Homework, DomainError> {
            unimplemented!()
        }
        async fn get_files(&self, _: uuid::Uuid) -> Result<Vec<HomeworkFile>, DomainError> {
            unimplemented!()
        }
        async fn save(&self, _: Homework) -> Result<Homework, DomainError> {
            unimplemented!()
        }
        async fn add_file(&self, _: HomeworkFile) -> Result<HomeworkFile, DomainError> {
            unimplemented!()
        }
        async fn remove_file(&self, _: uuid::Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn delete(&self, _: uuid::Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn create_with_files(
            &self,
            _: Homework,
            _: Vec<HomeworkFile>,
        ) -> Result<Homework, DomainError> {
            unimplemented!()
        }
    }

    #[async_trait]
    impl ScheduleWeekRepository for MockScheduleWeekRepository {
        async fn get_by_id(&self, week_start_date: NaiveDate) -> Result<ScheduleWeek, DomainError> {
            self.weeks
                .lock()
                .map_err(|_| DomainError::ScheduleWeekNotFound)?
                .iter()
                .find(|week| week.week_start_date == week_start_date)
                .cloned()
                .ok_or(DomainError::ScheduleWeekNotFound)
        }

        async fn get_all(&self) -> Result<Vec<ScheduleWeek>, DomainError> {
            self.weeks
                .lock()
                .map_err(|_| DomainError::ScheduleWeekNotFound)
                .map(|weeks| weeks.clone())
        }

        async fn save(&self, week: ScheduleWeek) -> Result<ScheduleWeek, DomainError> {
            let mut weeks = self
                .weeks
                .lock()
                .map_err(|_| DomainError::ScheduleWeekNotFound)?;
            if let Some(existing) = weeks
                .iter_mut()
                .find(|existing| existing.week_start_date == week.week_start_date)
            {
                *existing = week.clone();
            } else {
                weeks.push(week.clone());
            }
            Ok(week)
        }
    }

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).expect("test date must be valid")
    }

    fn service_with(weeks: Vec<ScheduleWeek>) -> ScheduleService {
        let unused = Arc::new(UnusedScheduleRepository);
        ScheduleService::new(
            Arc::new(MockScheduleWeekRepository::new(weeks)),
            unused.clone(),
            unused.clone(),
            unused.clone(),
            unused.clone(),
            unused.clone(),
            unused.clone(),
            unused,
        )
    }

    #[tokio::test]
    async fn create_draft_saves_week() {
        let service = service_with(Vec::new());

        let created = service
            .create_draft(date(2026, 8, 31), None)
            .await
            .expect("draft creation should succeed");

        assert_eq!(created.week_start_date, date(2026, 8, 31));
        assert!(created.is_draft());
        assert_eq!(service.list().await.expect("list should succeed").len(), 1);
    }

    #[tokio::test]
    async fn publish_changes_existing_week_status() {
        let service = service_with(vec![ScheduleWeek::new(
            date(2026, 8, 31),
            WeekStatus::Draft,
            None,
        )]);

        let published = service
            .publish(date(2026, 8, 31))
            .await
            .expect("publishing should succeed");

        assert!(published.is_published());
        assert!(service
            .list()
            .await
            .expect("list should succeed")
            .first()
            .expect("saved week should exist")
            .is_published());
    }

    #[tokio::test]
    async fn publish_missing_week_returns_domain_error() {
        let service = service_with(Vec::new());

        let result = service.publish(date(2026, 8, 31)).await;

        assert_eq!(result, Err(DomainError::ScheduleWeekNotFound));
    }

    #[tokio::test]
    async fn day_schedule_rejects_draft_week_before_loading_lesson_dependencies() {
        let service = service_with(vec![ScheduleWeek::new(
            date(2026, 8, 31),
            WeekStatus::Draft,
            None,
        )]);

        let result = service.day_schedule(date(2026, 9, 2)).await;

        assert_eq!(result, Err(DomainError::ScheduleWeekNotFound));
    }
}
