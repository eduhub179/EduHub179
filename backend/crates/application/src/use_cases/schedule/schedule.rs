//! Schedule-building use cases.
//!
//! Dependencies: `domain::entities::schedule_week` and
//! `domain::repositories::schedule_week_repository`.
//! Guarantees: persistence is accessed through a domain trait, so this module
//! remains independent of the PostgreSQL adapter.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{Datelike, NaiveDate, NaiveTime};
use domain::entities::homework::{Homework, HomeworkFile};
use domain::entities::lesson::Lesson;
use domain::entities::lesson_template::LessonTemplate;
use domain::entities::schedule_week::ScheduleWeek;
use domain::entities::{
    cabinet::Cabinet, lesson_instance::LessonInstance, lesson_substitution::LessonSubstitution,
    subject::Subject, user::User,
};
use domain::errors::DomainError;
use domain::repositories::cabinet_repository::CabinetRepository;
use domain::repositories::class_repository::ClassRepository;
use domain::repositories::homework_repository::HomeworkRepository;
use domain::repositories::lesson_instance_repository::LessonInstanceRepository;
use domain::repositories::lesson_repository::LessonRepository;
use domain::repositories::lesson_substitution_repository::LessonSubstitutionRepository;
use domain::repositories::lesson_template_repository::LessonTemplateRepository;
use domain::repositories::schedule_week_repository::ScheduleWeekRepository;
use domain::repositories::student_group_repository::StudentGroupRepository;
use domain::repositories::subject_repository::SubjectRepository;
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::lesson_target::LessonTarget;
use domain::value_objects::week_status::WeekStatus;
use uuid::Uuid;

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
    /// Class names for `ScheduleLesson::target_name`.
    classes: Arc<dyn ClassRepository>,
    /// Substitute teachers and notes set in the admin panel; `None` = not loaded.
    substitutions: Option<Arc<dyn LessonSubstitutionRepository>>,
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
    /// Who the lesson is for: class ("2027б") or group name. A teacher's schedule
    /// is unreadable without it; for students it names the group of group lessons.
    pub target_name: String,
    /// All teachers assigned to the lesson.
    pub teachers: Vec<User>,
    /// Cabinet used by this instance, falling back to the template cabinet.
    pub cabinet: Option<Cabinet>,
    /// Homework and attached files; absent means no homework was created.
    pub homework: Option<HomeworkDetails>,
    /// Substitute teacher and/or note set by an admin; `teachers` stay the usual ones.
    pub substitution: Option<LessonSubstitution>,
    /// The substitute teacher from `substitution`, loaded for display.
    pub substitute_teacher: Option<User>,
    /// Added by an admin to this date only (one-off or moved lesson).
    pub is_extra: bool,
}

/// Which lessons a schedule shows.
enum Scope {
    /// Lessons of these courses, plus single instances of other courses (the lessons
    /// a teacher substitutes in).
    Courses {
        lessons: Vec<Lesson>,
        extra_instances: HashSet<Uuid>,
    },
    /// Every lesson of the school (admin panel).
    All,
}

/// Data shared by many lessons of one schedule, loaded at most once per request:
/// a week repeats the same courses, subjects, teachers and cabinets many times.
/// Lives only while one schedule is built, so it never serves stale data.
#[derive(Default)]
struct LoadCache {
    templates: HashMap<Uuid, LessonTemplate>,
    lessons: HashMap<Uuid, Lesson>,
    subjects: HashMap<Uuid, Subject>,
    users: HashMap<Uuid, User>,
    cabinets: HashMap<Uuid, Cabinet>,
    /// Teachers by course id.
    teachers: HashMap<Uuid, Vec<User>>,
    /// Class / group name by course id.
    target_names: HashMap<Uuid, String>,
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
        classes: Arc<dyn ClassRepository>,
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
            classes,
            substitutions: None,
        }
    }

    /// Shows substitutions in every schedule and puts substituted lessons into the
    /// substitute teacher's schedule.
    pub fn with_substitutions(mut self, substitutions: Arc<dyn LessonSubstitutionRepository>) -> Self {
        self.substitutions = Some(substitutions);
        self
    }

    /// Every lesson of a date for the admin panel, also in a draft week.
    ///
    /// # Fail-safe поведение
    /// A date without a week returns `ScheduleWeekNotFound`, as for students.
    pub async fn all_lessons_for_date(&self, date: NaiveDate) -> Result<ScheduleDay, DomainError> {
        self.schedule_weeks.get_by_id(monday_of(date)).await?;
        self.build_day(date, Scope::All).await
    }

    /// One lesson instance with all display data (for the admin panel after an edit).
    pub async fn schedule_lesson(&self, instance_id: uuid::Uuid) -> Result<ScheduleLesson, DomainError> {
        let instance = self.instances.get_by_id(instance_id).await?;
        self.build_schedule_lesson(instance).await
    }

    /// Instances in `[from, to]` where the teacher substitutes; empty without substitutions.
    async fn substituted_instance_ids(
        &self,
        teacher_id: uuid::Uuid,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<std::collections::HashSet<uuid::Uuid>, DomainError> {
        match &self.substitutions {
            Some(repo) => Ok(repo
                .instance_ids_for_teacher(teacher_id, from, to)
                .await?
                .into_iter()
                .collect()),
            None => Ok(std::collections::HashSet::new()),
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
        student_id: Uuid,
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
        let scope = self.student_scope(&student).await?;
        self.build_day(date, scope).await
    }

    /// Loads the personal schedule for the student and week containing `date`.
    ///
    /// The date may be any day of the week; Monday is calculated internally.
    /// Only published weeks are returned because draft weeks are invisible to
    /// students. Fail-safe: a missing or draft week returns
    /// `DomainError::ScheduleWeekNotFound` and no partial response is emitted.
    pub async fn current_week_schedule(
        &self,
        student_id: Uuid,
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

        let scope = self.student_scope(&student).await?;
        self.build_week(week_start_date, scope).await
    }

    /// Loads the weekly schedule for a concrete class.
    ///
    /// This variant is used by staff screens or registration flows that need the
    /// schedule for a class, without resolving a student account first.
    pub async fn class_week_schedule(
        &self,
        class_id: Uuid,
        date: NaiveDate,
    ) -> Result<WeeklySchedule, DomainError> {
        let week_start_date = monday_of(date);
        let week = self.schedule_weeks.get_by_id(week_start_date).await?;
        if !week.is_published() {
            return Err(DomainError::ScheduleWeekNotFound);
        }

        let scope = self.class_scope(class_id).await?;
        self.build_week(week_start_date, scope).await
    }

    /// Loads the day schedule for a concrete class.
    pub async fn class_day_schedule(
        &self,
        class_id: Uuid,
        date: NaiveDate,
    ) -> Result<ScheduleDay, DomainError> {
        let week_start_date = monday_of(date);
        let week = self.schedule_weeks.get_by_id(week_start_date).await?;
        if !week.is_published() {
            return Err(DomainError::ScheduleWeekNotFound);
        }

        let scope = self.class_scope(class_id).await?;
        self.build_day(date, scope).await
    }

    /// Loads the week containing `date` for a teacher: lessons of every course the
    /// teacher is assigned to (`lesson_teachers`), each with its class/group name.
    ///
    /// # Fail-safe поведение
    /// `UserNotFound` if there is no such user or the user is a student;
    /// `UserIsInactive` for a deactivated teacher; a missing or draft week returns
    /// `ScheduleWeekNotFound`, as for students.
    pub async fn teacher_week_schedule(
        &self,
        teacher_id: Uuid,
        date: NaiveDate,
    ) -> Result<WeeklySchedule, DomainError> {
        let lessons = self.teacher_lessons(teacher_id).await?;
        let week_start_date = monday_of(date);
        let week = self.schedule_weeks.get_by_id(week_start_date).await?;
        if !week.is_published() {
            return Err(DomainError::ScheduleWeekNotFound);
        }

        let substituted = self
            .substituted_instance_ids(
                teacher_id,
                week_start_date,
                week_start_date + chrono::Duration::days(6),
            )
            .await?;
        let scope = Scope::Courses {
            lessons,
            extra_instances: substituted,
        };
        self.build_week(week_start_date, scope).await
    }

    /// Loads one day for a teacher; same rules as `teacher_week_schedule`.
    pub async fn teacher_day_schedule(
        &self,
        teacher_id: Uuid,
        date: NaiveDate,
    ) -> Result<ScheduleDay, DomainError> {
        let lessons = self.teacher_lessons(teacher_id).await?;
        let week = self.schedule_weeks.get_by_id(monday_of(date)).await?;
        if !week.is_published() {
            return Err(DomainError::ScheduleWeekNotFound);
        }

        let substituted = self.substituted_instance_ids(teacher_id, date, date).await?;
        let scope = Scope::Courses {
            lessons,
            extra_instances: substituted,
        };
        self.build_day(date, scope).await
    }

    /// Courses of a teacher. Admins may teach too, so only students are rejected.
    async fn teacher_lessons(&self, teacher_id: Uuid) -> Result<Vec<Lesson>, DomainError> {
        let teacher = self.users.get_by_id(teacher_id).await?;
        if teacher.role.is_student() {
            return Err(DomainError::UserNotFound);
        }
        if !teacher.is_active {
            return Err(DomainError::UserIsInactive);
        }
        self.lessons.get_by_teacher(teacher_id).await
    }

    /// Courses a student attends: those of their class and of their groups.
    async fn student_scope(&self, student: &User) -> Result<Scope, DomainError> {
        let group_ids: HashSet<Uuid> = self
            .student_groups
            .get_groups_by_student(student.id)
            .await?
            .into_iter()
            .map(|group| group.id)
            .collect();
        self.courses_scope(student.class_id, &group_ids).await
    }

    /// Courses of a class: its own plus group courses attended by its students.
    async fn class_scope(&self, class_id: Uuid) -> Result<Scope, DomainError> {
        let mut group_ids = HashSet::new();
        for student in self.users.get_active_students_by_class(class_id).await? {
            for group in self.student_groups.get_groups_by_student(student.id).await? {
                group_ids.insert(group.id);
            }
        }
        self.courses_scope(Some(class_id), &group_ids).await
    }

    async fn courses_scope(
        &self,
        class_id: Option<Uuid>,
        group_ids: &HashSet<Uuid>,
    ) -> Result<Scope, DomainError> {
        let mut lessons = match class_id {
            Some(class_id) => self.lessons.get_by_class(class_id).await?,
            None => Vec::new(),
        };
        for group_id in group_ids {
            lessons.extend(self.lessons.get_by_group(*group_id).await?);
        }
        Ok(Scope::Courses {
            lessons,
            extra_instances: HashSet::new(),
        })
    }

    /// The week starting `week_start_date` with the lessons of `scope`, Monday to Sunday.
    async fn build_week(
        &self,
        week_start_date: NaiveDate,
        scope: Scope,
    ) -> Result<WeeklySchedule, DomainError> {
        let instances = self.instances.get_by_week(week_start_date).await?;
        let mut days = (0..7)
            .map(|offset| ScheduleDay {
                date: week_start_date + chrono::Duration::days(offset),
                lessons: Vec::new(),
            })
            .collect::<Vec<_>>();

        for lesson in self.load_lessons(instances, scope).await? {
            let day_index = usize::try_from(lesson.instance.day_of_week())
                .map_err(|_| DomainError::InvalidLessonInstanceDate)?;
            days.get_mut(day_index)
                .ok_or(DomainError::InvalidLessonInstanceDate)?
                .lessons
                .push(lesson);
        }

        for day in &mut days {
            day.lessons.sort_by_key(|lesson| lesson.start_time);
        }
        Ok(WeeklySchedule {
            week_start_date,
            days,
        })
    }

    /// One day with the lessons of `scope`, by start time.
    async fn build_day(&self, date: NaiveDate, scope: Scope) -> Result<ScheduleDay, DomainError> {
        let instances = self.instances.get_by_date(date).await?;
        let mut lessons = self.load_lessons(instances, scope).await?;
        lessons.sort_by_key(|lesson| lesson.start_time);
        Ok(ScheduleDay { date, lessons })
    }

    /// Keeps the instances of `scope` and loads their display data.
    ///
    /// The school has hundreds of instances a week and a student a few dozen, so
    /// the scope is resolved *before* touching them: the templates of the scope's
    /// courses are loaded once and instances are matched by `template_id` in memory,
    /// with no query per foreign instance. Templates, courses, subjects, teachers,
    /// cabinets and class/group names repeat from lesson to lesson and are loaded at
    /// most once per call (`LoadCache`).
    ///
    /// # Fail-safe поведение
    /// Only instances that end up in the result are resolved, so a broken record of
    /// another class no longer fails this schedule; errors of the kept ones are
    /// returned unchanged.
    async fn load_lessons(
        &self,
        instances: Vec<LessonInstance>,
        scope: Scope,
    ) -> Result<Vec<ScheduleLesson>, DomainError> {
        let mut cache = LoadCache::default();
        let selected: Vec<LessonInstance> = match scope {
            Scope::Courses {
                lessons,
                extra_instances,
            } => {
                let mut course_templates = HashSet::new();
                for lesson in lessons {
                    if cache.lessons.contains_key(&lesson.id) {
                        continue;
                    }
                    // All templates, archived too: an instance may still point to one.
                    for template in self.templates.get_by_lesson(lesson.id).await? {
                        course_templates.insert(template.id);
                        cache.templates.insert(template.id, template);
                    }
                    cache.lessons.insert(lesson.id, lesson);
                }
                instances
                    .into_iter()
                    .filter(|instance| {
                        course_templates.contains(&instance.template_id)
                            || extra_instances.contains(&instance.id)
                    })
                    .collect()
            }
            Scope::All => {
                // One query for the usual case; archived templates load on demand.
                for template in self.templates.get_all_active().await? {
                    cache.templates.insert(template.id, template);
                }
                instances
            }
        };

        let mut lessons = Vec::with_capacity(selected.len());
        for instance in selected {
            lessons.push(self.build_lesson_cached(instance, &mut cache).await?);
        }
        Ok(lessons)
    }

    /// The other lessons of a double lesson: the same course right before and after
    /// `instance_id` in that class's (or group's) day, cancelled lessons skipped.
    /// Same rule as the chat agent, which puts one homework on both lessons of a pair.
    pub async fn double_lesson_partners(
        &self,
        instance_id: Uuid,
    ) -> Result<Vec<ScheduleLesson>, DomainError> {
        let lesson = self.schedule_lesson(instance_id).await?;
        // Only the day of the same class (or group) matters, not the whole school.
        let lessons = match lesson.lesson.target {
            LessonTarget::Class(class_id) => self.lessons.get_by_class(class_id).await?,
            LessonTarget::Group(group_id) => self.lessons.get_by_group(group_id).await?,
        };
        let scope = Scope::Courses {
            lessons,
            extra_instances: HashSet::from([instance_id]),
        };
        let mut day: Vec<ScheduleLesson> = self
            .build_day(lesson.instance.lesson_date, scope)
            .await?
            .lessons
            .into_iter()
            .filter(|other| {
                other.lesson.target == lesson.lesson.target
                    && (other.instance.id == instance_id || !other.instance.status.is_cancelled())
            })
            .collect();
        day.sort_by_key(|other| other.start_time);
        let Some(position) = day.iter().position(|other| other.instance.id == instance_id) else {
            return Ok(Vec::new());
        };
        let same_course = |other: &ScheduleLesson| other.lesson.id == lesson.lesson.id;
        let mut first = position;
        while first > 0 && same_course(&day[first - 1]) {
            first -= 1;
        }
        let mut last = position;
        while last + 1 < day.len() && same_course(&day[last + 1]) {
            last += 1;
        }
        Ok(day
            .drain(first..=last)
            .filter(|other| other.instance.id != instance_id)
            .collect())
    }

    /// "2027б" for a class lesson, the group name for a group lesson.
    pub async fn target_name_of(&self, lesson: &Lesson) -> Result<String, DomainError> {
        self.target_name(lesson).await
    }

    async fn target_name(&self, lesson: &Lesson) -> Result<String, DomainError> {
        match lesson.target {
            LessonTarget::Class(class_id) => Ok(self.classes.get_by_id(class_id).await?.full_name()),
            LessonTarget::Group(group_id) => Ok(self.student_groups.get_by_id(group_id).await?.name),
        }
    }

    /// Loads all related data for one schedule instance.
    async fn build_schedule_lesson(
        &self,
        instance: LessonInstance,
    ) -> Result<ScheduleLesson, DomainError> {
        self.build_lesson_cached(instance, &mut LoadCache::default()).await
    }

    /// `build_schedule_lesson` that reuses data already in `cache`. Homework,
    /// substitution and `is_extra` belong to this instance only and are not cached.
    async fn build_lesson_cached(
        &self,
        instance: LessonInstance,
        cache: &mut LoadCache,
    ) -> Result<ScheduleLesson, DomainError> {
        let template = self.cached_template(cache, instance.template_id).await?;
        let lesson = self.cached_lesson(cache, template.lesson_id).await?;
        let subject = self.cached_subject(cache, lesson.subject_id).await?;
        let teacher_list = self.cached_teachers(cache, lesson.id).await?;

        let cabinet = match instance.cabinet_id.or(template.cabinet_id) {
            Some(id) => Some(self.cached_cabinet(cache, id).await?),
            None => None,
        };
        let homework = load_homework(&self.homeworks, instance.id).await?;
        let target_name = self.cached_target_name(cache, &lesson).await?;
        let (substitution, is_extra) = match &self.substitutions {
            Some(repo) => (repo.get(instance.id).await?, repo.is_extra(instance.id).await?),
            None => (None, false),
        };
        let substitute_teacher = match substitution.as_ref().and_then(|s| s.substitute_teacher_id) {
            Some(id) => Some(self.cached_user(cache, id).await?),
            None => None,
        };
        Ok(ScheduleLesson {
            instance,
            start_time: template.start_time,
            end_time: template.end_time,
            lesson,
            subject,
            target_name,
            teachers: teacher_list,
            cabinet,
            homework,
            substitution,
            substitute_teacher,
            is_extra,
        })
    }

    async fn cached_template(&self, cache: &mut LoadCache, id: Uuid) -> Result<LessonTemplate, DomainError> {
        if let Some(found) = cache.templates.get(&id) {
            return Ok(found.clone());
        }
        let loaded = self.templates.get_by_id(id).await?;
        cache.templates.insert(id, loaded.clone());
        Ok(loaded)
    }

    async fn cached_lesson(&self, cache: &mut LoadCache, id: Uuid) -> Result<Lesson, DomainError> {
        if let Some(found) = cache.lessons.get(&id) {
            return Ok(found.clone());
        }
        let loaded = self.lessons.get_by_id(id).await?;
        cache.lessons.insert(id, loaded.clone());
        Ok(loaded)
    }

    async fn cached_subject(&self, cache: &mut LoadCache, id: Uuid) -> Result<Subject, DomainError> {
        if let Some(found) = cache.subjects.get(&id) {
            return Ok(found.clone());
        }
        let loaded = self.subjects.get_by_id(id).await?;
        cache.subjects.insert(id, loaded.clone());
        Ok(loaded)
    }

    async fn cached_user(&self, cache: &mut LoadCache, id: Uuid) -> Result<User, DomainError> {
        if let Some(found) = cache.users.get(&id) {
            return Ok(found.clone());
        }
        let loaded = self.users.get_by_id(id).await?;
        cache.users.insert(id, loaded.clone());
        Ok(loaded)
    }

    async fn cached_cabinet(&self, cache: &mut LoadCache, id: Uuid) -> Result<Cabinet, DomainError> {
        if let Some(found) = cache.cabinets.get(&id) {
            return Ok(found.clone());
        }
        let loaded = self.cabinets.get_by_id(id).await?;
        cache.cabinets.insert(id, loaded.clone());
        Ok(loaded)
    }

    /// Teachers of a course, in `get_teacher_ids` order.
    async fn cached_teachers(&self, cache: &mut LoadCache, lesson_id: Uuid) -> Result<Vec<User>, DomainError> {
        if let Some(found) = cache.teachers.get(&lesson_id) {
            return Ok(found.clone());
        }
        let teacher_ids = self.lessons.get_teacher_ids(lesson_id).await?;
        let mut teachers = Vec::with_capacity(teacher_ids.len());
        for teacher_id in teacher_ids {
            teachers.push(self.cached_user(cache, teacher_id).await?);
        }
        cache.teachers.insert(lesson_id, teachers.clone());
        Ok(teachers)
    }

    async fn cached_target_name(&self, cache: &mut LoadCache, lesson: &Lesson) -> Result<String, DomainError> {
        if let Some(found) = cache.target_names.get(&lesson.id) {
            return Ok(found.clone());
        }
        let loaded = self.target_name(lesson).await?;
        cache.target_names.insert(lesson.id, loaded.clone());
        Ok(loaded)
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
        let target_week = match self.schedule_weeks.get_by_id(target_week_start_date).await {
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
            // Inactive templates are one-off lessons and slots removed from the regular
            // timetable: they belong to the source week only.
            if !self.templates.get_by_id(inst.template_id).await?.is_active {
                continue;
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

    struct DummyClassRepo;
    #[async_trait]
    impl ClassRepository for DummyClassRepo {
        async fn get_by_id(
            &self,
            _class_id: uuid::Uuid,
        ) -> Result<domain::entities::class::Class, DomainError> {
            Err(DomainError::ClassNotFound)
        }
        async fn get_active_by_year(
            &self,
            _graduation_year: i32,
        ) -> Result<Vec<domain::entities::class::Class>, DomainError> {
            Ok(Vec::new())
        }
        async fn get_all_active(&self) -> Result<Vec<domain::entities::class::Class>, DomainError> {
            Ok(Vec::new())
        }
        async fn save(
            &self,
            class: domain::entities::class::Class,
        ) -> Result<domain::entities::class::Class, DomainError> {
            Ok(class)
        }
    }

    struct DummyStudentGroups;
    #[async_trait]
    impl StudentGroupRepository for DummyStudentGroups {
        async fn get_by_id(
            &self,
            _group_id: uuid::Uuid,
        ) -> Result<domain::entities::student_group::StudentGroup, DomainError> {
            Err(DomainError::StudentGroupNotFound)
        }

        async fn get_all(
            &self,
        ) -> Result<Vec<domain::entities::student_group::StudentGroup>, DomainError> {
            Ok(Vec::new())
        }

        async fn save(
            &self,
            _group: domain::entities::student_group::StudentGroup,
        ) -> Result<domain::entities::student_group::StudentGroup, DomainError> {
            Err(DomainError::InternalError)
        }

        async fn add_member(
            &self,
            _group_id: uuid::Uuid,
            _student_id: uuid::Uuid,
        ) -> Result<(), DomainError> {
            Err(DomainError::StudentGroupNotFound)
        }

        async fn add_members(
            &self,
            _group_id: uuid::Uuid,
            _student_ids: &[uuid::Uuid],
        ) -> Result<(), DomainError> {
            Err(DomainError::StudentGroupNotFound)
        }

        async fn remove_member(
            &self,
            _group_id: uuid::Uuid,
            _student_id: uuid::Uuid,
        ) -> Result<(), DomainError> {
            Err(DomainError::StudentGroupNotFound)
        }

        async fn get_member_ids(
            &self,
            _group_id: uuid::Uuid,
        ) -> Result<Vec<uuid::Uuid>, DomainError> {
            Ok(Vec::new())
        }

        async fn get_groups_by_student(
            &self,
            _student_id: uuid::Uuid,
        ) -> Result<Vec<domain::entities::student_group::StudentGroup>, DomainError> {
            Ok(Vec::new())
        }

        async fn has_member(
            &self,
            _group_id: uuid::Uuid,
            _student_id: uuid::Uuid,
        ) -> Result<bool, DomainError> {
            Ok(false)
        }
    }

    struct DummyLessonRepo;
    #[async_trait]
    impl LessonRepository for DummyLessonRepo {
        async fn get_by_id(&self, _lesson_id: uuid::Uuid) -> Result<Lesson, DomainError> {
            Err(DomainError::LessonNotFound)
        }
        async fn get_teacher_ids(
            &self,
            _lesson_id: uuid::Uuid,
        ) -> Result<Vec<uuid::Uuid>, DomainError> {
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
        async fn get_by_teacher(
            &self,
            _teacher_id: uuid::Uuid,
        ) -> Result<Vec<Lesson>, DomainError> {
            Ok(Vec::new())
        }
        async fn assign_teacher(
            &self,
            _lesson_id: uuid::Uuid,
            _teacher_id: uuid::Uuid,
        ) -> Result<(), DomainError> {
            Err(DomainError::LessonNotFound)
        }
        async fn unassign_teacher(
            &self,
            _lesson_id: uuid::Uuid,
            _teacher_id: uuid::Uuid,
        ) -> Result<(), DomainError> {
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
        async fn get_by_id(
            &self,
            _user_id: uuid::Uuid,
        ) -> Result<domain::entities::user::User, DomainError> {
            Err(DomainError::UserNotFound)
        }
        async fn get_by_login(
            &self,
            _login: &str,
        ) -> Result<domain::entities::user::User, DomainError> {
            Err(DomainError::UserNotFound)
        }
        async fn get_active_students_by_class(
            &self,
            _class_id: uuid::Uuid,
        ) -> Result<Vec<domain::entities::user::User>, DomainError> {
            Ok(Vec::new())
        }
        async fn save(
            &self,
            _user: domain::entities::user::User,
        ) -> Result<domain::entities::user::User, DomainError> {
            Err(DomainError::InternalError)
        }
    }

    struct DummyCabinetRepo;
    #[async_trait]
    impl CabinetRepository for DummyCabinetRepo {
        async fn get_by_id(&self, _cabinet_id: uuid::Uuid) -> Result<Cabinet, DomainError> {
            Err(DomainError::CabinetNotFound)
        }
        async fn get_by_number(&self, _number: i32) -> Result<Cabinet, DomainError> {
            Err(DomainError::CabinetNotFound)
        }
        async fn get_all(&self) -> Result<Vec<Cabinet>, DomainError> {
            Ok(Vec::new())
        }
        async fn get_by_floor(&self, _floor: i32) -> Result<Vec<Cabinet>, DomainError> {
            Ok(Vec::new())
        }
        async fn save(&self, _cabinet: Cabinet) -> Result<Cabinet, DomainError> {
            Err(DomainError::InternalError)
        }
    }

    struct DummyHomeworkRepo;
    #[async_trait]
    impl HomeworkRepository for DummyHomeworkRepo {
        async fn get_by_id(&self, _homework_id: uuid::Uuid) -> Result<Homework, DomainError> {
            Err(DomainError::HomeworkNotFound)
        }
        async fn get_by_lesson_instance(
            &self,
            _lesson_instance_id: uuid::Uuid,
        ) -> Result<Homework, DomainError> {
            Err(DomainError::HomeworkNotFound)
        }
        async fn get_files(
            &self,
            _homework_id: uuid::Uuid,
        ) -> Result<Vec<HomeworkFile>, DomainError> {
            Ok(Vec::new())
        }
        async fn save(&self, _homework: Homework) -> Result<Homework, DomainError> {
            Err(DomainError::InternalError)
        }
        async fn add_file(
            &self,
            _file: domain::entities::homework::HomeworkFile,
        ) -> Result<domain::entities::homework::HomeworkFile, DomainError> {
            Err(DomainError::InternalError)
        }
        async fn remove_file(&self, _file_id: uuid::Uuid) -> Result<HomeworkFile, DomainError> {
            Err(DomainError::InternalError)
        }
        async fn delete(&self, _homework_id: uuid::Uuid) -> Result<(), DomainError> {
            Err(DomainError::InternalError)
        }
        async fn create_with_files(
            &self,
            _homework: Homework,
            _files: Vec<domain::entities::homework::HomeworkFile>,
        ) -> Result<Homework, DomainError> {
            Err(DomainError::InternalError)
        }
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
        async fn get_all(&self) -> Result<Vec<ScheduleWeek>, DomainError> {
            Ok(vec![self.week.clone()])
        }
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
        async fn get_by_id(
            &self,
            template_id: uuid::Uuid,
        ) -> Result<domain::entities::lesson_template::LessonTemplate, DomainError> {
            self.templates
                .iter()
                .find(|t| t.id == template_id)
                .cloned()
                .ok_or(DomainError::LessonTemplateNotFound)
        }
        async fn get_by_lesson(
            &self,
            _lesson_id: uuid::Uuid,
        ) -> Result<Vec<domain::entities::lesson_template::LessonTemplate>, DomainError> {
            Ok(Vec::new())
        }
        async fn get_active_for_day(
            &self,
            _day: domain::value_objects::day_of_week::DayOfWeek,
        ) -> Result<Vec<domain::entities::lesson_template::LessonTemplate>, DomainError> {
            Ok(Vec::new())
        }
        async fn get_all_active(
            &self,
        ) -> Result<Vec<domain::entities::lesson_template::LessonTemplate>, DomainError> {
            Ok(self.templates.clone())
        }
        async fn save(
            &self,
            _template: domain::entities::lesson_template::LessonTemplate,
        ) -> Result<domain::entities::lesson_template::LessonTemplate, DomainError> {
            Err(DomainError::InternalError)
        }
    }

    struct MockInstanceRepo {
        saved: Mutex<Vec<LessonInstance>>,
        week_instances: Vec<LessonInstance>,
    }

    #[async_trait]
    impl LessonInstanceRepository for MockInstanceRepo {
        async fn get_by_id(&self, _instance_id: uuid::Uuid) -> Result<LessonInstance, DomainError> {
            Err(DomainError::LessonInstanceNotFound)
        }
        async fn get_by_week(
            &self,
            week_start_date: NaiveDate,
        ) -> Result<Vec<LessonInstance>, DomainError> {
            let mut out = Vec::new();
            for inst in &self.week_instances {
                if inst.week_start_date == week_start_date {
                    out.push(inst.clone());
                }
            }
            Ok(out)
        }
        async fn get_by_date(
            &self,
            _lesson_date: NaiveDate,
        ) -> Result<Vec<LessonInstance>, DomainError> {
            Ok(Vec::new())
        }
        async fn get_by_template(
            &self,
            _template_id: uuid::Uuid,
        ) -> Result<Vec<LessonInstance>, DomainError> {
            Ok(Vec::new())
        }
        async fn save(&self, instance: LessonInstance) -> Result<LessonInstance, DomainError> {
            self.saved.lock().unwrap().push(instance.clone());
            Ok(instance)
        }
    }

    #[tokio::test]
    async fn test_generate_from_templates_creates_instances() {
        use chrono::NaiveTime;


        let week_date = NaiveDate::from_ymd_opt(2026, 9, 7).unwrap();
        let week = ScheduleWeek::new(week_date, WeekStatus::Draft, None);
        let schedule_repo = MockScheduleWeekRepo {
            week: week.clone(),
            saved: Mutex::new(None),
        };

        let template = domain::entities::lesson_template::LessonTemplate::try_new(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            domain::value_objects::day_of_week::DayOfWeek::Mon,
            NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            NaiveTime::from_hms_opt(9, 45, 0).unwrap(),
            domain::value_objects::week_parity::WeekParity::Every,
            None,
            true,
        )
        .unwrap();

        let template_repo = MockTemplateRepo {
            templates: vec![template],
        };
        let instance_repo = MockInstanceRepo {
            saved: Mutex::new(Vec::new()),
            week_instances: Vec::new(),
        };

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
            Arc::new(DummyClassRepo),
        );

        let created = service
            .generate_from_templates(week_date)
            .await
            .expect("generate ok");
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].week_start_date, week_date);
    }

    fn copy_week_template(is_active: bool) -> domain::entities::lesson_template::LessonTemplate {
        domain::entities::lesson_template::LessonTemplate::try_new(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            domain::value_objects::day_of_week::DayOfWeek::Mon,
            NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            NaiveTime::from_hms_opt(9, 45, 0).unwrap(),
            domain::value_objects::week_parity::WeekParity::Every,
            None,
            is_active,
        )
        .unwrap()
    }

    async fn run_copy_week(
        templates: Vec<domain::entities::lesson_template::LessonTemplate>,
        instances: Vec<LessonInstance>,
        source_date: NaiveDate,
        target_date: NaiveDate,
    ) -> Vec<LessonInstance> {
        let service = ScheduleService::new(
            Arc::new(MockScheduleWeekRepo {
                week: ScheduleWeek::new(source_date, WeekStatus::Draft, None),
                saved: Mutex::new(None),
            }),
            Arc::new(DummyStudentGroups),
            Arc::new(MockInstanceRepo {
                saved: Mutex::new(Vec::new()),
                week_instances: instances,
            }),
            Arc::new(MockTemplateRepo { templates }),
            Arc::new(DummyLessonRepo),
            Arc::new(DummySubjectRepo),
            Arc::new(DummyUserRepo),
            Arc::new(DummyCabinetRepo),
            Arc::new(DummyHomeworkRepo),
            Arc::new(DummyClassRepo),
        );
        service
            .copy_week(source_date, target_date)
            .await
            .expect("copy ok")
    }

    #[tokio::test]
    async fn test_copy_week_copies_scheduled_instances_and_sets_copied_from() {
        use domain::value_objects::lesson_instance_status::LessonInstanceStatus;
        let source_date = NaiveDate::from_ymd_opt(2026, 9, 7).unwrap();
        let target_date = NaiveDate::from_ymd_opt(2026, 9, 14).unwrap();

        let template = copy_week_template(true);
        let inst = LessonInstance::try_new(
            uuid::Uuid::new_v4(),
            template.id,
            source_date,
            source_date,
            LessonInstanceStatus::Scheduled,
            None,
        )
        .unwrap();

        let created =
            run_copy_week(vec![template], vec![inst.clone()], source_date, target_date).await;
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].week_start_date, target_date);
        // lesson_date should be shifted by one week
        assert_eq!(
            created[0].lesson_date,
            inst.lesson_date + chrono::Duration::days(7)
        );
    }

    #[tokio::test]
    async fn test_copy_week_skips_inactive_templates() {
        use domain::value_objects::lesson_instance_status::LessonInstanceStatus;
        let source_date = NaiveDate::from_ymd_opt(2026, 9, 7).unwrap();
        let target_date = NaiveDate::from_ymd_opt(2026, 9, 14).unwrap();

        let regular = copy_week_template(true);
        let one_off = copy_week_template(false);
        let instances = [&regular, &one_off]
            .iter()
            .map(|t| {
                LessonInstance::try_new(
                    uuid::Uuid::new_v4(),
                    t.id,
                    source_date,
                    source_date,
                    LessonInstanceStatus::Scheduled,
                    None,
                )
                .unwrap()
            })
            .collect();

        let created = run_copy_week(
            vec![regular.clone(), one_off],
            instances,
            source_date,
            target_date,
        )
        .await;
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].template_id, regular.id);
    }
}
