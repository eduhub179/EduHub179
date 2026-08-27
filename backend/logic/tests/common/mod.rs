#![allow(dead_code)]

use async_trait::async_trait;
use chrono::NaiveDate;
use domain::entities::cabinet::Cabinet;
use domain::entities::homework::{Homework, HomeworkFile};
use domain::entities::lesson::Lesson;
use domain::entities::lesson_instance::LessonInstance;
use domain::entities::lesson_template::LessonTemplate;
use domain::entities::schedule_week::ScheduleWeek;
use domain::entities::student_group::StudentGroup;
use domain::entities::subject::Subject;
use domain::entities::user::User;
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
use domain::value_objects::day_of_week::DayOfWeek;
use logic::services::ScheduleService;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use uuid::Uuid;

#[derive(Default)]
pub struct MockRepositories {
    pub weeks: Mutex<Vec<ScheduleWeek>>,
    pub student: Option<User>,
    pub groups: Vec<StudentGroup>,
    pub instances: Vec<LessonInstance>,
    pub templates: Vec<LessonTemplate>,
    pub lessons: Vec<Lesson>,
    pub subjects: Vec<Subject>,
}

impl MockRepositories {
    pub fn with_weeks(weeks: Vec<ScheduleWeek>) -> Self {
        Self {
            weeks: Mutex::new(weeks),
            ..Default::default()
        }
    }

    pub fn with_schedule(
        student: User,
        instances: Vec<LessonInstance>,
        templates: Vec<LessonTemplate>,
        lessons: Vec<Lesson>,
        subjects: Vec<Subject>,
    ) -> Self {
        Self {
            weeks: Mutex::new(vec![ScheduleWeek::new(
                date(2026, 8, 31),
                domain::value_objects::week_status::WeekStatus::Published,
                None,
            )]),
            student: Some(student),
            instances,
            templates,
            lessons,
            subjects,
            ..Default::default()
        }
    }
}

#[async_trait]
impl ScheduleWeekRepository for MockRepositories {
    async fn get_by_id(&self, date: NaiveDate) -> Result<ScheduleWeek, DomainError> {
        self.weeks
            .lock()
            .map_err(|_| DomainError::ScheduleWeekNotFound)?
            .iter()
            .find(|w| w.week_start_date == date)
            .cloned()
            .ok_or(DomainError::ScheduleWeekNotFound)
    }
    async fn get_all(&self) -> Result<Vec<ScheduleWeek>, DomainError> {
        self.weeks
            .lock()
            .map_err(|_| DomainError::ScheduleWeekNotFound)
            .map(|w| w.clone())
    }
    async fn save(&self, week: ScheduleWeek) -> Result<ScheduleWeek, DomainError> {
        let mut weeks = self
            .weeks
            .lock()
            .map_err(|_| DomainError::ScheduleWeekNotFound)?;
        if let Some(existing) = weeks
            .iter_mut()
            .find(|w| w.week_start_date == week.week_start_date)
        {
            *existing = week.clone();
        } else {
            weeks.push(week.clone());
        }
        Ok(week)
    }
}

#[async_trait]
impl LessonInstanceRepository for MockRepositories {
    async fn get_by_id(&self, _: Uuid) -> Result<LessonInstance, DomainError> {
        Err(DomainError::LessonInstanceNotFound)
    }
    async fn get_by_week(&self, week: NaiveDate) -> Result<Vec<LessonInstance>, DomainError> {
        Ok(self
            .instances
            .iter()
            .filter(|i| i.week_start_date == week)
            .cloned()
            .collect())
    }
    async fn get_by_date(&self, date: NaiveDate) -> Result<Vec<LessonInstance>, DomainError> {
        Ok(self
            .instances
            .iter()
            .filter(|i| i.lesson_date == date)
            .cloned()
            .collect())
    }
    async fn get_by_template(&self, _: Uuid) -> Result<Vec<LessonInstance>, DomainError> {
        Ok(Vec::new())
    }
    async fn save(&self, _: LessonInstance) -> Result<LessonInstance, DomainError> {
        Err(DomainError::LessonInstanceNotFound)
    }
}

#[async_trait]
impl LessonTemplateRepository for MockRepositories {
    async fn get_by_id(&self, id: Uuid) -> Result<LessonTemplate, DomainError> {
        self.templates
            .iter()
            .find(|t| t.id == id)
            .cloned()
            .ok_or(DomainError::LessonTemplateNotFound)
    }
    async fn get_by_lesson(&self, _: Uuid) -> Result<Vec<LessonTemplate>, DomainError> {
        Ok(Vec::new())
    }
    async fn get_active_for_day(&self, _: DayOfWeek) -> Result<Vec<LessonTemplate>, DomainError> {
        Ok(Vec::new())
    }
    async fn get_all_active(&self) -> Result<Vec<LessonTemplate>, DomainError> {
        Ok(Vec::new())
    }
    async fn save(&self, _: LessonTemplate) -> Result<LessonTemplate, DomainError> {
        Err(DomainError::LessonTemplateNotFound)
    }
}

#[async_trait]
impl LessonRepository for MockRepositories {
    async fn get_by_id(&self, id: Uuid) -> Result<Lesson, DomainError> {
        self.lessons
            .iter()
            .find(|l| l.id == id)
            .cloned()
            .ok_or(DomainError::LessonNotFound)
    }
    async fn save(&self, _: Lesson) -> Result<Lesson, DomainError> {
        Err(DomainError::LessonNotFound)
    }
    async fn get_by_class(&self, _: Uuid) -> Result<Vec<Lesson>, DomainError> {
        Ok(Vec::new())
    }
    async fn get_by_group(&self, _: Uuid) -> Result<Vec<Lesson>, DomainError> {
        Ok(Vec::new())
    }
    async fn get_by_teacher(&self, _: Uuid) -> Result<Vec<Lesson>, DomainError> {
        Ok(Vec::new())
    }
    async fn assign_teacher(&self, _: Uuid, _: Uuid) -> Result<(), DomainError> {
        Ok(())
    }
    async fn unassign_teacher(&self, _: Uuid, _: Uuid) -> Result<(), DomainError> {
        Ok(())
    }
    async fn get_teacher_ids(&self, _: Uuid) -> Result<Vec<Uuid>, DomainError> {
        Ok(Vec::new())
    }
}

#[async_trait]
impl StudentGroupRepository for MockRepositories {
    async fn get_by_id(&self, _: Uuid) -> Result<StudentGroup, DomainError> {
        Err(DomainError::StudentGroupNotFound)
    }
    async fn get_all(&self) -> Result<Vec<StudentGroup>, DomainError> {
        Ok(Vec::new())
    }
    async fn save(&self, _: StudentGroup) -> Result<StudentGroup, DomainError> {
        Err(DomainError::StudentGroupNotFound)
    }
    async fn add_member(&self, _: Uuid, _: Uuid) -> Result<(), DomainError> {
        Ok(())
    }
    async fn add_members(&self, _: Uuid, _: &[Uuid]) -> Result<(), DomainError> {
        Ok(())
    }
    async fn remove_member(&self, _: Uuid, _: Uuid) -> Result<(), DomainError> {
        Ok(())
    }
    async fn get_member_ids(&self, _: Uuid) -> Result<Vec<Uuid>, DomainError> {
        Ok(Vec::new())
    }
    async fn get_groups_by_student(&self, _: Uuid) -> Result<Vec<StudentGroup>, DomainError> {
        Ok(self.groups.clone())
    }
    async fn has_member(&self, _: Uuid, _: Uuid) -> Result<bool, DomainError> {
        Ok(false)
    }
}

#[async_trait]
impl SubjectRepository for MockRepositories {
    async fn get_by_id(&self, id: Uuid) -> Result<Subject, DomainError> {
        self.subjects
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or(DomainError::SubjectNotFound)
    }
    async fn get_all(&self) -> Result<Vec<Subject>, DomainError> {
        Ok(Vec::new())
    }
    async fn save(&self, _: Subject) -> Result<Subject, DomainError> {
        Err(DomainError::SubjectNotFound)
    }
}

#[async_trait]
impl UserRepository for MockRepositories {
    async fn get_by_id(&self, id: Uuid) -> Result<User, DomainError> {
        self.student
            .as_ref()
            .filter(|s| s.id == id)
            .cloned()
            .ok_or(DomainError::UserNotFound)
    }
    async fn get_by_email(&self, _: &str) -> Result<User, DomainError> {
        Err(DomainError::UserNotFound)
    }
    async fn get_active_students_by_class(&self, _: Uuid) -> Result<Vec<User>, DomainError> {
        Ok(Vec::new())
    }
    async fn save(&self, _: User) -> Result<User, DomainError> {
        Err(DomainError::UserNotFound)
    }
}

#[async_trait]
impl CabinetRepository for MockRepositories {
    async fn get_by_id(&self, _: Uuid) -> Result<Cabinet, DomainError> {
        Err(DomainError::CabinetNotFound)
    }
    async fn get_by_number(&self, _: i32) -> Result<Cabinet, DomainError> {
        Err(DomainError::CabinetNotFound)
    }
    async fn get_all(&self) -> Result<Vec<Cabinet>, DomainError> {
        Ok(Vec::new())
    }
    async fn get_by_floor(&self, _: i32) -> Result<Vec<Cabinet>, DomainError> {
        Ok(Vec::new())
    }
    async fn save(&self, _: Cabinet) -> Result<Cabinet, DomainError> {
        Err(DomainError::CabinetNotFound)
    }
}

#[async_trait]
impl HomeworkRepository for MockRepositories {
    async fn get_by_id(&self, _: Uuid) -> Result<Homework, DomainError> {
        Err(DomainError::HomeworkNotFound)
    }
    async fn get_by_lesson_instance(&self, _: Uuid) -> Result<Homework, DomainError> {
        Err(DomainError::HomeworkNotFound)
    }
    async fn get_files(&self, _: Uuid) -> Result<Vec<HomeworkFile>, DomainError> {
        Ok(Vec::new())
    }
    async fn save(&self, _: Homework) -> Result<Homework, DomainError> {
        Err(DomainError::HomeworkNotFound)
    }
    async fn add_file(&self, _: HomeworkFile) -> Result<HomeworkFile, DomainError> {
        Err(DomainError::HomeworkFileNotFound)
    }
    async fn remove_file(&self, _: Uuid) -> Result<(), DomainError> {
        Err(DomainError::HomeworkFileNotFound)
    }
    async fn delete(&self, _: Uuid) -> Result<(), DomainError> {
        Err(DomainError::HomeworkNotFound)
    }
    async fn create_with_files(
        &self,
        _: Homework,
        _: Vec<HomeworkFile>,
    ) -> Result<Homework, DomainError> {
        Err(DomainError::HomeworkNotFound)
    }
}

pub fn service_with(weeks: Vec<ScheduleWeek>) -> ScheduleService {
    let r = Arc::new(MockRepositories::with_weeks(weeks));
    ScheduleService::new(
        r.clone(),
        r.clone(),
        r.clone(),
        r.clone(),
        r.clone(),
        r.clone(),
        r.clone(),
        r.clone(),
        r,
    )
}

pub fn service_with_personal_schedule(
    student: User,
    instances: Vec<LessonInstance>,
    templates: Vec<LessonTemplate>,
    lessons: Vec<Lesson>,
    subjects: Vec<Subject>,
) -> ScheduleService {
    let r = Arc::new(MockRepositories::with_schedule(
        student, instances, templates, lessons, subjects,
    ));
    ScheduleService::new(
        r.clone(),
        r.clone(),
        r.clone(),
        r.clone(),
        r.clone(),
        r.clone(),
        r.clone(),
        r.clone(),
        r,
    )
}

pub fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("valid test date")
}

pub fn block_on<F: Future>(future: F) -> F::Output {
    fn no_op(_: *const ()) {}
    fn clone(_: *const ()) -> RawWaker {
        RawWaker::new(std::ptr::null(), &VTABLE)
    }
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, no_op, no_op, no_op);
    let waker = unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) };
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match Pin::new(&mut future).poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
