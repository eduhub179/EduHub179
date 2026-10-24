//! Admin editing of the timetable itself:
//! - courses (`lessons`: class or group + subject) and their teachers;
//! - the regular timetable (`lesson_templates`), from which weeks are generated;
//! - one-off lessons in a concrete week: add a lesson, move a lesson.
//!
//! Regular timetable changes never rewrite history. A changed slot becomes a new
//! template (the old one is deactivated), and lessons of already created weeks are
//! moved to it only from `apply_from` on. A lesson with homework is never moved to
//! another subject: it is left in place and counted in `TimetableChange::kept`.
//!
//! One-off lessons use an inactive template, so they are neither generated into
//! new weeks nor copied by `ScheduleService::copy_week`.
//!
//! Access: admins only, as in `AdminService`.

use std::collections::HashSet;
use std::sync::Arc;

use chrono::{Datelike, NaiveDate, NaiveTime, Weekday};
use domain::{
    entities::{
        lesson::Lesson, lesson_template::LessonTemplate, student_group::StudentGroup,
        subject::Subject, user::User,
    },
    errors::DomainError,
    repositories::{
        cabinet_repository::CabinetRepository,
        lesson_instance_repository::LessonInstanceRepository,
        lesson_repository::LessonRepository,
        lesson_template_repository::LessonTemplateRepository,
        schedule_week_repository::ScheduleWeekRepository,
        student_group_repository::StudentGroupRepository,
        subject_repository::SubjectRepository,
        timetable_repository::{SlotSpec, TimetableRepository, WriteCount},
        user_repository::UserRepository,
    },
    value_objects::{day_of_week::DayOfWeek, lesson_target::LessonTarget, week_parity::WeekParity},
};
use uuid::Uuid;

use crate::use_cases::schedule::schedule::{ScheduleLesson, ScheduleService};

/// A course with display data.
#[derive(Debug, Clone, PartialEq)]
pub struct Course {
    pub lesson: Lesson,
    pub subject: Subject,
    /// "2027б" or the group name.
    pub target_name: String,
    pub teachers: Vec<User>,
}

/// A slot of the regular timetable with its course.
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub template: LessonTemplate,
    pub course: Course,
    pub cabinet_number: Option<i32>,
}

/// The regular timetable of a class: its courses (class and group ones) and slots.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassTimetable {
    pub courses: Vec<Course>,
    pub slots: Vec<Slot>,
}

/// Time and place of a slot.
pub type SlotInput = SlotSpec;

/// What a regular timetable change did to already created weeks.
pub type TimetableChange = WriteCount;

pub struct TimetableService {
    lessons: Arc<dyn LessonRepository>,
    templates: Arc<dyn LessonTemplateRepository>,
    instances: Arc<dyn LessonInstanceRepository>,
    weeks: Arc<dyn ScheduleWeekRepository>,
    subjects: Arc<dyn SubjectRepository>,
    groups: Arc<dyn StudentGroupRepository>,
    users: Arc<dyn UserRepository>,
    cabinets: Arc<dyn CabinetRepository>,
    timetable: Arc<dyn TimetableRepository>,
    schedule: Arc<ScheduleService>,
}

fn require_admin(actor: &User) -> Result<(), DomainError> {
    if actor.is_admin() && actor.is_active {
        Ok(())
    } else {
        Err(DomainError::InsufficientPermissions)
    }
}

/// Day of a school date; Sunday has no lessons.
fn day_of(date: NaiveDate) -> Result<DayOfWeek, DomainError> {
    Ok(match date.weekday() {
        Weekday::Mon => DayOfWeek::Mon,
        Weekday::Tue => DayOfWeek::Tue,
        Weekday::Wed => DayOfWeek::Wed,
        Weekday::Thu => DayOfWeek::Thu,
        Weekday::Fri => DayOfWeek::Fri,
        Weekday::Sat => DayOfWeek::Sat,
        Weekday::Sun => return Err(DomainError::InvalidLessonInstanceDate),
    })
}

fn monday_of(date: NaiveDate) -> NaiveDate {
    date - chrono::Duration::days(i64::from(date.weekday().num_days_from_monday()))
}

impl TimetableService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        lessons: Arc<dyn LessonRepository>,
        templates: Arc<dyn LessonTemplateRepository>,
        instances: Arc<dyn LessonInstanceRepository>,
        weeks: Arc<dyn ScheduleWeekRepository>,
        subjects: Arc<dyn SubjectRepository>,
        groups: Arc<dyn StudentGroupRepository>,
        users: Arc<dyn UserRepository>,
        cabinets: Arc<dyn CabinetRepository>,
        timetable: Arc<dyn TimetableRepository>,
        schedule: Arc<ScheduleService>,
    ) -> Self {
        Self {
            lessons,
            templates,
            instances,
            weeks,
            subjects,
            groups,
            users,
            cabinets,
            timetable,
            schedule,
        }
    }

    // ------------------------------------------------------------------
    // Directories
    // ------------------------------------------------------------------

    pub async fn subjects(&self, actor: &User) -> Result<Vec<Subject>, DomainError> {
        require_admin(actor)?;
        let mut subjects = self.subjects.get_all().await?;
        subjects.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(subjects)
    }

    /// # Fail-safe поведение
    /// `InvalidSubjectNameFormat` for an empty name, `SubjectAlreadyExists` for a taken one.
    pub async fn create_subject(&self, actor: &User, name: &str) -> Result<Subject, DomainError> {
        require_admin(actor)?;
        self.subjects
            .save(Subject::try_new(Uuid::new_v4(), name.trim().to_string())?)
            .await
    }

    pub async fn groups(&self, actor: &User) -> Result<Vec<StudentGroup>, DomainError> {
        require_admin(actor)?;
        let mut groups = self.groups.get_all().await?;
        groups.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(groups)
    }

    // ------------------------------------------------------------------
    // Courses
    // ------------------------------------------------------------------

    async fn course(&self, lesson: Lesson) -> Result<Course, DomainError> {
        let subject = self.subjects.get_by_id(lesson.subject_id).await?;
        let target_name = match lesson.target {
            LessonTarget::Class(_) => {
                // Same name as in the schedule.
                self.schedule_target_name(&lesson).await?
            }
            LessonTarget::Group(group_id) => self.groups.get_by_id(group_id).await?.name,
        };
        let mut teachers = Vec::new();
        for teacher_id in self.lessons.get_teacher_ids(lesson.id).await? {
            teachers.push(self.users.get_by_id(teacher_id).await?);
        }
        teachers.sort_by(|a, b| a.last_name.cmp(&b.last_name));
        Ok(Course {
            lesson,
            subject,
            target_name,
            teachers,
        })
    }

    async fn schedule_target_name(&self, lesson: &Lesson) -> Result<String, DomainError> {
        self.schedule.target_name_of(lesson).await
    }

    /// Active courses of a class and of the groups its students are in.
    async fn class_lessons(&self, class_id: Uuid) -> Result<Vec<Lesson>, DomainError> {
        let mut lessons = self.lessons.get_by_class(class_id).await?;
        for group_id in self.timetable.class_group_ids(class_id).await? {
            lessons.extend(self.lessons.get_by_group(group_id).await?);
        }
        Ok(lessons)
    }

    /// Courses and regular slots of a class, slots by day and time.
    pub async fn class_timetable(
        &self,
        actor: &User,
        class_id: Uuid,
    ) -> Result<ClassTimetable, DomainError> {
        require_admin(actor)?;
        let mut courses = Vec::new();
        for lesson in self.class_lessons(class_id).await? {
            courses.push(self.course(lesson).await?);
        }
        courses.sort_by(|a, b| {
            (&a.subject.name, &a.target_name).cmp(&(&b.subject.name, &b.target_name))
        });

        let mut slots = Vec::new();
        for course in &courses {
            for template in self.templates.get_by_lesson(course.lesson.id).await? {
                if !template.is_active {
                    continue;
                }
                let cabinet_number = match template.cabinet_id {
                    Some(id) => Some(self.cabinets.get_by_id(id).await?.number),
                    None => None,
                };
                slots.push(Slot {
                    template,
                    course: course.clone(),
                    cabinet_number,
                });
            }
        }
        slots.sort_by_key(|slot| {
            (
                slot.template.day.num_days_from_monday(),
                slot.template.start_time,
            )
        });
        Ok(ClassTimetable { courses, slots })
    }

    /// Teachers and admins only; deactivated users are refused.
    async fn check_teachers(&self, teacher_ids: &[Uuid]) -> Result<(), DomainError> {
        for id in teacher_ids {
            let teacher = self.users.get_by_id(*id).await?;
            if teacher.role.is_student() {
                return Err(DomainError::UserNotFound);
            }
            if !teacher.is_active {
                return Err(DomainError::UserIsInactive);
            }
        }
        Ok(())
    }

    /// Creates a course (class or group + subject) with its teachers.
    ///
    /// # Fail-safe поведение
    /// `LessonAlreadyExists` if a course with this target and subject exists.
    pub async fn create_course(
        &self,
        actor: &User,
        target: LessonTarget,
        subject_id: Uuid,
        teacher_ids: &[Uuid],
    ) -> Result<Course, DomainError> {
        require_admin(actor)?;
        self.subjects.get_by_id(subject_id).await?;
        self.check_teachers(teacher_ids).await?;
        match target {
            LessonTarget::Class(_) => {}
            LessonTarget::Group(group_id) => {
                self.groups.get_by_id(group_id).await?;
            }
        }
        let lesson = self
            .lessons
            .save(Lesson::new(Uuid::new_v4(), target, subject_id, true))
            .await?;
        for id in teacher_ids {
            self.lessons.assign_teacher(lesson.id, *id).await?;
        }
        self.course(lesson).await
    }

    /// Replaces the teachers of a course (for every week, past ones included:
    /// a one-day change is a substitution instead).
    pub async fn set_course_teachers(
        &self,
        actor: &User,
        lesson_id: Uuid,
        teacher_ids: &[Uuid],
    ) -> Result<Course, DomainError> {
        require_admin(actor)?;
        let lesson = self.lessons.get_by_id(lesson_id).await?;
        self.check_teachers(teacher_ids).await?;
        let current: HashSet<Uuid> = self
            .lessons
            .get_teacher_ids(lesson_id)
            .await?
            .into_iter()
            .collect();
        let wanted: HashSet<Uuid> = teacher_ids.iter().copied().collect();
        for id in wanted.difference(&current) {
            self.lessons.assign_teacher(lesson_id, *id).await?;
        }
        for id in current.difference(&wanted) {
            self.lessons.unassign_teacher(lesson_id, *id).await?;
        }
        self.course(lesson).await
    }

    // ------------------------------------------------------------------
    // Regular timetable
    // ------------------------------------------------------------------

    async fn check_slot(&self, input: &SlotInput) -> Result<(), DomainError> {
        self.lessons.get_by_id(input.lesson_id).await?;
        if let Some(cabinet_id) = input.cabinet_id {
            self.cabinets.get_by_id(cabinet_id).await?;
        }
        if input.end_time <= input.start_time {
            return Err(DomainError::InvalidLessonTemplateTime);
        }
        Ok(())
    }

    /// Adds a regular slot. With `apply_from`, the lesson is also added to already
    /// created weeks from that date on.
    ///
    /// # Fail-safe поведение
    /// `LessonTemplateAlreadyExists` for the same course at the same time,
    /// `LessonTemplateSlotConflict` for an overlapping slot of the same course.
    pub async fn add_slot(
        &self,
        actor: &User,
        input: SlotInput,
        apply_from: Option<NaiveDate>,
    ) -> Result<TimetableChange, DomainError> {
        require_admin(actor)?;
        self.check_slot(&input).await?;
        let (_, change) = self.timetable.add_slot(input, apply_from).await?;
        Ok(change)
    }

    /// Changes a regular slot (course, day, time, cabinet). With `apply_from`, lessons of
    /// already created weeks from that date on move to the new slot.
    pub async fn update_slot(
        &self,
        actor: &User,
        template_id: Uuid,
        input: SlotInput,
        apply_from: Option<NaiveDate>,
    ) -> Result<TimetableChange, DomainError> {
        require_admin(actor)?;
        self.check_slot(&input).await?;
        self.timetable
            .change_slot(template_id, input, apply_from)
            .await
    }

    /// Removes a regular slot. With `apply_from`, its lessons in already created weeks
    /// from that date on are deleted; those with homework are cancelled instead.
    pub async fn remove_slot(
        &self,
        actor: &User,
        template_id: Uuid,
        apply_from: Option<NaiveDate>,
    ) -> Result<TimetableChange, DomainError> {
        require_admin(actor)?;
        self.timetable.remove_slot(template_id, apply_from).await
    }

    // ------------------------------------------------------------------
    // One-off lessons
    // ------------------------------------------------------------------

    /// Checks a one-off place and returns its slot and week.
    async fn one_off_slot(
        &self,
        lesson_id: Uuid,
        date: NaiveDate,
        start_time: NaiveTime,
        end_time: NaiveTime,
        cabinet_id: Option<Uuid>,
    ) -> Result<(SlotInput, NaiveDate), DomainError> {
        let day = day_of(date)?;
        let monday = monday_of(date);
        self.weeks.get_by_id(monday).await?;
        let slot = SlotInput {
            lesson_id,
            day,
            start_time,
            end_time,
            parity: WeekParity::Every,
            cabinet_id,
        };
        self.check_slot(&slot).await?;
        Ok((slot, monday))
    }

    /// Adds a lesson to one date only (the week must exist). A cancelled lesson in the
    /// same slot is restored instead of duplicated.
    ///
    /// # Fail-safe поведение
    /// `ScheduleWeekNotFound` without a week, `InvalidLessonInstanceDate` on Sunday,
    /// `LessonInstanceAlreadyExists` if the lesson is already there.
    pub async fn add_one_off(
        &self,
        actor: &User,
        lesson_id: Uuid,
        date: NaiveDate,
        start_time: NaiveTime,
        end_time: NaiveTime,
        cabinet_id: Option<Uuid>,
    ) -> Result<ScheduleLesson, DomainError> {
        require_admin(actor)?;
        let (slot, monday) = self
            .one_off_slot(lesson_id, date, start_time, end_time, cabinet_id)
            .await?;
        let id = self
            .timetable
            .place_lesson(slot, monday, date, None)
            .await?;
        self.schedule.schedule_lesson(id).await
    }

    /// Moves one lesson to another date / time (within an existing week): the original
    /// is cancelled, the new one is added, and homework goes with it.
    pub async fn move_lesson(
        &self,
        actor: &User,
        instance_id: Uuid,
        date: NaiveDate,
        start_time: NaiveTime,
        end_time: NaiveTime,
        cabinet_id: Option<Uuid>,
    ) -> Result<ScheduleLesson, DomainError> {
        require_admin(actor)?;
        let mut original = self.instances.get_by_id(instance_id).await?;
        if original.status.is_cancelled() {
            return Err(DomainError::LessonInstanceNotFound);
        }
        let template = self.templates.get_by_id(original.template_id).await?;
        if original.lesson_date == date
            && template.start_time == start_time
            && template.end_time == end_time
        {
            // Same time: only the cabinet can change.
            original.cabinet_id = cabinet_id;
            self.instances.save(original).await?;
            return self.schedule.schedule_lesson(instance_id).await;
        }

        let (slot, monday) = self
            .one_off_slot(template.lesson_id, date, start_time, end_time, cabinet_id)
            .await?;
        let id = self
            .timetable
            .place_lesson(slot, monday, date, Some(instance_id))
            .await?;
        self.schedule.schedule_lesson(id).await
    }
}
