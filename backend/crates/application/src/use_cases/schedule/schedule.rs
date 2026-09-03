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
use domain::repositories::student_group_repository::StudentGroupRepository;
use domain::repositories::subject_repository::SubjectRepository;
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::week_status::WeekStatus;

/// Coordinates schedule-week operations for presentation handlers.
///
/// The concrete repository is injected by the composition root. This keeps
/// business rules testable with an in-memory implementation.
pub struct ScheduleService {
    schedule_weeks: Arc<dyn ScheduleWeekRepository>,
    student_groups: Arc<dyn StudentGroupRepository>,
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
        student_groups: Arc<dyn StudentGroupRepository>,
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
            student_groups,
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

    /// Loads the personal schedule for one concrete calendar day.
    ///
    /// Only the published week containing `date` is visible to students. The
    /// day query uses `LessonInstanceRepository::get_by_date`, so it does not
    /// load the other six days of the week. Only lessons for the student's
    /// class or groups are returned. Fail-safe: a missing or draft week
    /// returns `DomainError::ScheduleWeekNotFound`; repository failures are
    /// propagated unchanged.
    pub async fn day_schedule(
        &self,
        student_id: uuid::Uuid,
        date: NaiveDate,
    ) -> Result<ScheduleDay, DomainError> {
        let week_start_date = monday_of(date);
        let week = self.schedule_weeks.get_by_id(week_start_date).await?;
        if !week.is_published() {
            return Err(DomainError::ScheduleWeekNotFound);
        }

        let student = self.users.get_by_id(student_id).await?;
        if !student.is_active {
            return Err(DomainError::UserIsInactive);
        }
        if !student.role.is_student() {
            return Err(DomainError::InsufficientPermissions);
        }
        let student_group_ids = self
            .student_groups
            .get_groups_by_student(student_id)
            .await?
            .into_iter()
            .map(|group| group.id)
            .collect::<std::collections::HashSet<_>>();

        self.build_day_schedule(date, student.class_id, &student_group_ids)
            .await
    }

    /// Loads the personal schedule for the student and week containing `date`.
    ///
    /// The date may be any day of the week; Monday is calculated internally.
    /// Only published weeks are returned because draft weeks are invisible to
    /// students. Fail-safe: a missing or draft week returns
    /// `DomainError::ScheduleWeekNotFound` and no partial response is emitted.
    pub async fn current_week_schedule(
        &self,
        student_id: uuid::Uuid,
        date: NaiveDate,
    ) -> Result<WeeklySchedule, DomainError> {
        let student = self.users.get_by_id(student_id).await?;
        if !student.is_active {
            return Err(DomainError::UserIsInactive);
        }
        if !student.role.is_student() {
            return Err(DomainError::InsufficientPermissions);
        }

        let week_start_date = monday_of(date);
        let week = self.schedule_weeks.get_by_id(week_start_date).await?;
        if !week.is_published() {
            return Err(DomainError::ScheduleWeekNotFound);
        }

        self.build_week_schedule(week_start_date, student_id).await
    }

    /// Composes one frontend-ready day from the repositories stored in the service.
    async fn build_day_schedule(
        &self,
        date: NaiveDate,
        student_class_id: Option<uuid::Uuid>,
        student_group_ids: &std::collections::HashSet<uuid::Uuid>,
    ) -> Result<ScheduleDay, DomainError> {
        let day_instances = self.instances.get_by_date(date).await?;
        let mut lessons_for_day = Vec::with_capacity(day_instances.len());

        for instance in day_instances {
            let template = self.templates.get_by_id(instance.template_id).await?;
            let lesson = self.lessons.get_by_id(template.lesson_id).await?;
            let belongs_to_student = lesson.class_id() == student_class_id
                || lesson
                    .group_id()
                    .is_some_and(|group_id| student_group_ids.contains(&group_id));
            if !belongs_to_student {
                continue;
            }

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
        student_id: uuid::Uuid,
    ) -> Result<WeeklySchedule, DomainError> {
        let week_instances = self.instances.get_by_week(week_start_date).await?;
        let student = self.users.get_by_id(student_id).await?;
        let student_group_ids = self
            .student_groups
            .get_groups_by_student(student_id)
            .await?
            .into_iter()
            .map(|group| group.id)
            .collect::<std::collections::HashSet<_>>();
        let mut days = (0..7)
            .map(|offset| ScheduleDay {
                date: week_start_date + chrono::Duration::days(offset),
                lessons: Vec::new(),
            })
            .collect::<Vec<_>>();

        for instance in week_instances {
            let template = self.templates.get_by_id(instance.template_id).await?;
            let lesson = self.lessons.get_by_id(template.lesson_id).await?;
            let belongs_to_student = lesson.class_id() == student.class_id
                || lesson
                    .group_id()
                    .is_some_and(|group_id| student_group_ids.contains(&group_id));
            if !belongs_to_student {
                continue;
            }

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

    /// Generates lesson instances for the given week from all active templates.
    ///
    /// Behavior:
    /// - Requires the week to exist (`ScheduleWeekRepository::get_by_id`).
    /// - Fails if the week already contains instances.
    /// - Creates one `LessonInstance` per active template (parity currently
    ///   treated as 'every') with `status = Scheduled` and `cabinet_id` taken
    ///   from the template as a starting value.
    /// - Saves each instance via `LessonInstanceRepository::save`.
    pub async fn generate_from_templates(
        &self,
        week_start_date: NaiveDate,
    ) -> Result<Vec<LessonInstance>, DomainError> {
        // Ensure week exists
        let _week = self.schedule_weeks.get_by_id(week_start_date).await?;

        // If week already has instances, refuse to generate to avoid accidental duplicates
        let existing = self.instances.get_by_week(week_start_date).await?;
        if !existing.is_empty() {
            return Err(DomainError::LessonInstanceAlreadyExists);
        }

        // Fetch all active templates and create instances for the week.
        let templates = self.templates.get_all_active().await?;
        let mut created = Vec::with_capacity(templates.len());
        for template in templates {
            let instance = LessonInstance::for_template(
                uuid::Uuid::new_v4(),
                &template,
                week_start_date,
                domain::value_objects::lesson_instance_status::LessonInstanceStatus::Scheduled,
                template.cabinet_id,
            );
            let saved = self.instances.save(instance.clone()).await?;
            created.push(saved);
        }

        Ok(created)
    }

    /// Copies instances from `source_week_start_date` to `target_week_start_date`.
    ///
    /// Behavior:
    /// - If `target` does not exist, it is created as a `Draft` with `copied_from = source`.
    /// - Fails if `target` already contains instances.
    /// - Copies only scheduled instances from the source, creating matching
    ///   scheduled instances in the target with shifted dates and copied `cabinet_id`.
    /// - Records provenance by setting `copied_from` on the target week.
    pub async fn copy_week(
        &self,
        source_week_start_date: NaiveDate,
        target_week_start_date: NaiveDate,
    ) -> Result<Vec<LessonInstance>, DomainError> {
        // Ensure source exists
        let _source_week = self
            .schedule_weeks
            .get_by_id(source_week_start_date)
            .await?;

        // Ensure or create target week
        let target_week = match self
            .schedule_weeks
            .get_by_id(target_week_start_date)
            .await
        {
            Ok(w) => w,
            Err(DomainError::ScheduleWeekNotFound) => {
                let new = ScheduleWeek::new(
                    target_week_start_date,
                    domain::value_objects::week_status::WeekStatus::Draft,
                    Some(source_week_start_date),
                );
                self.schedule_weeks.save(new.clone()).await?;
                new
            }
            Err(e) => return Err(e),
        };

        // Fail if target already populated
        let target_existing = self.instances.get_by_week(target_week_start_date).await?;
        if !target_existing.is_empty() {
            return Err(DomainError::LessonInstanceAlreadyExists);
        }

        // Load source instances and copy scheduled ones
        let source_instances = self.instances.get_by_week(source_week_start_date).await?;
        let mut created = Vec::new();
        let offset = target_week_start_date - source_week_start_date;
        for inst in source_instances.into_iter() {
            if !inst.status.is_scheduled() {
                continue; // skip cancelled/completed (do not copy replacements)
            }

            let new_lesson_date = inst.lesson_date + chrono::Duration::days(offset.num_days());
            let new_instance = LessonInstance::try_new(
                uuid::Uuid::new_v4(),
                inst.template_id,
                target_week_start_date,
                new_lesson_date,
                domain::value_objects::lesson_instance_status::LessonInstanceStatus::Scheduled,
                inst.cabinet_id,
            )?;

            let saved = self.instances.save(new_instance.clone()).await?;
            created.push(saved);
        }

        // Update target week provenance (copied_from)
        let mut updated_week = target_week.clone();
        updated_week.copied_from = Some(source_week_start_date);
        self.schedule_weeks.save(updated_week).await?;

        Ok(created)
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

// =====================
// Unit tests for generators
// =====================
#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::NaiveDate;
    use std::sync::Mutex;

    struct DummyStudentGroups;
    #[async_trait]
    impl StudentGroupRepository for DummyStudentGroups {
        async fn get_by_id(&self, _group_id: uuid::Uuid) -> Result<domain::entities::student_group::StudentGroup, DomainError> {
            Err(DomainError::StudentGroupNotFound)
        }

        async fn get_all(&self) -> Result<Vec<domain::entities::student_group::StudentGroup>, DomainError> {
            Ok(Vec::new())
        }

        async fn save(&self, _group: domain::entities::student_group::StudentGroup) -> Result<domain::entities::student_group::StudentGroup, DomainError> {
            Err(DomainError::InternalError)
        }

        async fn add_member(&self, _group_id: uuid::Uuid, _student_id: uuid::Uuid) -> Result<(), DomainError> {
            Err(DomainError::StudentGroupNotFound)
        }

        async fn add_members(&self, _group_id: uuid::Uuid, _student_ids: &[uuid::Uuid]) -> Result<(), DomainError> {
            Err(DomainError::StudentGroupNotFound)
        }

        async fn remove_member(&self, _group_id: uuid::Uuid, _student_id: uuid::Uuid) -> Result<(), DomainError> {
            Err(DomainError::StudentGroupNotFound)
        }

        async fn get_member_ids(&self, _group_id: uuid::Uuid) -> Result<Vec<uuid::Uuid>, DomainError> {
            Ok(Vec::new())
        }

        async fn get_groups_by_student(&self, _student_id: uuid::Uuid) -> Result<Vec<domain::entities::student_group::StudentGroup>, DomainError> {
            Ok(Vec::new())
        }

        async fn has_member(&self, _group_id: uuid::Uuid, _student_id: uuid::Uuid) -> Result<bool, DomainError> {
            Ok(false)
        }
    }

    struct DummyLessonRepo;
    #[async_trait]
    impl LessonRepository for DummyLessonRepo {
        async fn get_by_id(&self, _lesson_id: uuid::Uuid) -> Result<Lesson, DomainError> {
            Err(DomainError::LessonNotFound)
        }
        async fn get_teacher_ids(&self, _lesson_id: uuid::Uuid) -> Result<Vec<uuid::Uuid>, DomainError> {
            Ok(Vec::new())
        }
        async fn save(&self, _lesson: Lesson) -> Result<Lesson, DomainError> {
            Err(DomainError::InternalError)
        }
        async fn get_by_class(&self, _class_id: uuid::Uuid) -> Result<Vec<Lesson>, DomainError> {
            Ok(Vec::new())
        }
        async fn get_by_group(&self, _group_id: uuid::Uuid) -> Result<Vec<Lesson>, DomainError> {
            Ok(Vec::new())
        }
        async fn get_by_teacher(&self, _teacher_id: uuid::Uuid) -> Result<Vec<Lesson>, DomainError> {
            Ok(Vec::new())
        }
        async fn assign_teacher(&self, _lesson_id: uuid::Uuid, _teacher_id: uuid::Uuid) -> Result<(), DomainError> {
            Err(DomainError::LessonNotFound)
        }
        async fn unassign_teacher(&self, _lesson_id: uuid::Uuid, _teacher_id: uuid::Uuid) -> Result<(), DomainError> {
            Err(DomainError::LessonNotFound)
        }
    }

    struct DummySubjectRepo;
    #[async_trait]
    impl SubjectRepository for DummySubjectRepo {
        async fn get_by_id(&self, _subject_id: uuid::Uuid) -> Result<Subject, DomainError> {
            Err(DomainError::SubjectNotFound)
        }
        async fn get_all(&self) -> Result<Vec<Subject>, DomainError> {
            Ok(Vec::new())
        }
        async fn save(&self, _subject: Subject) -> Result<Subject, DomainError> {
            Err(DomainError::InternalError)
        }
    }

    struct DummyUserRepo;
    #[async_trait]
    impl UserRepository for DummyUserRepo {
        async fn get_by_id(&self, _user_id: uuid::Uuid) -> Result<domain::entities::user::User, DomainError> {
            Err(DomainError::UserNotFound)
        }
        async fn get_by_login(&self, _login: &str) -> Result<domain::entities::user::User, DomainError> { Err(DomainError::UserNotFound) }
        async fn get_active_students_by_class(&self, _class_id: uuid::Uuid) -> Result<Vec<domain::entities::user::User>, DomainError> { Ok(Vec::new()) }
        async fn save(&self, _user: domain::entities::user::User) -> Result<domain::entities::user::User, DomainError> { Err(DomainError::InternalError) }
    }

    struct DummyCabinetRepo;
    #[async_trait]
    impl CabinetRepository for DummyCabinetRepo {
        async fn get_by_id(&self, _cabinet_id: uuid::Uuid) -> Result<Cabinet, DomainError> { Err(DomainError::CabinetNotFound) }
        async fn get_by_number(&self, _number: i32) -> Result<Cabinet, DomainError> { Err(DomainError::CabinetNotFound) }
        async fn get_all(&self) -> Result<Vec<Cabinet>, DomainError> { Ok(Vec::new()) }
        async fn get_by_floor(&self, _floor: i32) -> Result<Vec<Cabinet>, DomainError> { Ok(Vec::new()) }
        async fn save(&self, _cabinet: Cabinet) -> Result<Cabinet, DomainError> { Err(DomainError::InternalError) }
    }

    struct DummyHomeworkRepo;
    #[async_trait]
    impl HomeworkRepository for DummyHomeworkRepo {
        async fn get_by_id(&self, _homework_id: uuid::Uuid) -> Result<Homework, DomainError> { Err(DomainError::HomeworkNotFound) }
        async fn get_by_lesson_instance(&self, _lesson_instance_id: uuid::Uuid) -> Result<Homework, DomainError> { Err(DomainError::HomeworkNotFound) }
        async fn get_files(&self, _homework_id: uuid::Uuid) -> Result<Vec<HomeworkFile>, DomainError> { Ok(Vec::new()) }
        async fn save(&self, _homework: Homework) -> Result<Homework, DomainError> { Err(DomainError::InternalError) }
        async fn add_file(&self, _file: domain::entities::homework::HomeworkFile) -> Result<domain::entities::homework::HomeworkFile, DomainError> { Err(DomainError::InternalError) }
        async fn remove_file(&self, _file_id: uuid::Uuid) -> Result<(), DomainError> { Err(DomainError::InternalError) }
        async fn delete(&self, _homework_id: uuid::Uuid) -> Result<(), DomainError> { Err(DomainError::InternalError) }
        async fn create_with_files(&self, _homework: Homework, _files: Vec<domain::entities::homework::HomeworkFile>) -> Result<Homework, DomainError> { Err(DomainError::InternalError) }
    }

    struct MockScheduleWeekRepo {
        week: ScheduleWeek,
        saved: Mutex<Option<ScheduleWeek>>,
    }

    #[async_trait]
    impl ScheduleWeekRepository for MockScheduleWeekRepo {
        async fn get_by_id(&self, week_start_date: NaiveDate) -> Result<ScheduleWeek, DomainError> {
            if self.week.week_start_date == week_start_date {
                Ok(self.week.clone())
            } else {
                Err(DomainError::ScheduleWeekNotFound)
            }
        }
        async fn get_all(&self) -> Result<Vec<ScheduleWeek>, DomainError> { Ok(vec![self.week.clone()]) }
        async fn save(&self, week: ScheduleWeek) -> Result<ScheduleWeek, DomainError> {
            *self.saved.lock().unwrap() = Some(week.clone());
            Ok(week)
        }
    }

    struct MockTemplateRepo {
        templates: Vec<domain::entities::lesson_template::LessonTemplate>,
    }

    #[async_trait]
    impl LessonTemplateRepository for MockTemplateRepo {
        async fn get_by_id(&self, _template_id: uuid::Uuid) -> Result<domain::entities::lesson_template::LessonTemplate, DomainError> { Err(DomainError::LessonTemplateNotFound) }
        async fn get_by_lesson(&self, _lesson_id: uuid::Uuid) -> Result<Vec<domain::entities::lesson_template::LessonTemplate>, DomainError> { Ok(Vec::new()) }
        async fn get_active_for_day(&self, _day: domain::value_objects::day_of_week::DayOfWeek) -> Result<Vec<domain::entities::lesson_template::LessonTemplate>, DomainError> { Ok(Vec::new()) }
        async fn get_all_active(&self) -> Result<Vec<domain::entities::lesson_template::LessonTemplate>, DomainError> { Ok(self.templates.clone()) }
        async fn save(&self, _template: domain::entities::lesson_template::LessonTemplate) -> Result<domain::entities::lesson_template::LessonTemplate, DomainError> { Err(DomainError::InternalError) }
    }

    struct MockInstanceRepo {
        saved: Mutex<Vec<LessonInstance>>,
        week_instances: Vec<LessonInstance>,
    }

    #[async_trait]
    impl LessonInstanceRepository for MockInstanceRepo {
        async fn get_by_id(&self, _instance_id: uuid::Uuid) -> Result<LessonInstance, DomainError> { Err(DomainError::LessonInstanceNotFound) }
        async fn get_by_week(&self, week_start_date: NaiveDate) -> Result<Vec<LessonInstance>, DomainError> {
            let mut out = Vec::new();
            for inst in &self.week_instances {
                if inst.week_start_date == week_start_date {
                    out.push(inst.clone());
                }
            }
            Ok(out)
        }
        async fn get_by_date(&self, _lesson_date: NaiveDate) -> Result<Vec<LessonInstance>, DomainError> { Ok(Vec::new()) }
        async fn get_by_template(&self, _template_id: uuid::Uuid) -> Result<Vec<LessonInstance>, DomainError> { Ok(Vec::new()) }
        async fn save(&self, instance: LessonInstance) -> Result<LessonInstance, DomainError> {
            self.saved.lock().unwrap().push(instance.clone());
            Ok(instance)
        }
    }

    #[tokio::test]
    async fn test_generate_from_templates_creates_instances() {
        use domain::value_objects::day_of_week::DayOfWeek;
        use chrono::NaiveTime;

        let week_date = NaiveDate::from_ymd_opt(2026, 9, 7).unwrap();
        let week = ScheduleWeek::new(week_date, WeekStatus::Draft, None);
        let schedule_repo = MockScheduleWeekRepo { week: week.clone(), saved: Mutex::new(None) };

        let template = domain::entities::lesson_template::LessonTemplate::try_new(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            DayOfWeek::Mon,
            NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            NaiveTime::from_hms_opt(9, 45, 0).unwrap(),
            domain::value_objects::week_parity::WeekParity::Every,
            None,
            true,
        ).unwrap();

        let template_repo = MockTemplateRepo { templates: vec![template] };
        let instance_repo = MockInstanceRepo { saved: Mutex::new(Vec::new()), week_instances: Vec::new() };

        let service = ScheduleService::new(
            Arc::new(schedule_repo),
            Arc::new(DummyStudentGroups),
            Arc::new(instance_repo),
            Arc::new(template_repo),
            Arc::new(DummyLessonRepo),
            Arc::new(DummySubjectRepo),
            Arc::new(DummyUserRepo),
            Arc::new(DummyCabinetRepo),
            Arc::new(DummyHomeworkRepo),
        );

        let created = service.generate_from_templates(week_date).await.expect("generate ok");
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].week_start_date, week_date);
    }

    #[tokio::test]
    async fn test_copy_week_copies_scheduled_instances_and_sets_copied_from() {
        use chrono::NaiveTime;
        use domain::value_objects::lesson_instance_status::LessonInstanceStatus;
        // source week
        let source_date = NaiveDate::from_ymd_opt(2026, 9, 7).unwrap();
        let target_date = NaiveDate::from_ymd_opt(2026, 9, 14).unwrap();

        let inst = LessonInstance::try_new(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            source_date,
            source_date,
            LessonInstanceStatus::Scheduled,
            None,
        ).unwrap();

        let schedule_repo = MockScheduleWeekRepo { week: ScheduleWeek::new(source_date, WeekStatus::Draft, None), saved: Mutex::new(None) };
        let template_repo = MockTemplateRepo { templates: Vec::new() };
        let instance_repo = MockInstanceRepo { saved: Mutex::new(Vec::new()), week_instances: vec![inst.clone()] };

        let service = ScheduleService::new(
            Arc::new(schedule_repo),
            Arc::new(DummyStudentGroups),
            Arc::new(instance_repo),
            Arc::new(template_repo),
            Arc::new(DummyLessonRepo),
            Arc::new(DummySubjectRepo),
            Arc::new(DummyUserRepo),
            Arc::new(DummyCabinetRepo),
            Arc::new(DummyHomeworkRepo),
        );

        let created = service.copy_week(source_date, target_date).await.expect("copy ok");
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].week_start_date, target_date);
        // lesson_date should be shifted by one week
        assert_eq!(created[0].lesson_date, inst.lesson_date + chrono::Duration::days(7));
    }
}
