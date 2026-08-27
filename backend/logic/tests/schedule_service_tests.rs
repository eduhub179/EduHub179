//! Integration tests for the schedule business logic.
//!
//! These tests live outside `src` and do not require PostgreSQL. The mock
//! repositories stand in for infrastructure adapters and verify the public
//! service contract used by presentation handlers.

use async_trait::async_trait;
use chrono::NaiveDate;
use domain::entities::cabinet::Cabinet;
use domain::entities::homework::{Homework, HomeworkFile};
use domain::entities::lesson::Lesson;
use domain::entities::lesson_instance::LessonInstance;
use domain::entities::lesson_template::LessonTemplate;
use domain::entities::schedule_week::ScheduleWeek;
use domain::entities::subject::Subject;
use domain::entities::user::User;
use domain::errors::DomainError;
use domain::repositories::cabinet_repository::CabinetRepository;
use domain::repositories::homework_repository::HomeworkRepository;
use domain::repositories::lesson_instance_repository::LessonInstanceRepository;
use domain::repositories::lesson_repository::LessonRepository;
use domain::repositories::lesson_template_repository::LessonTemplateRepository;
use domain::repositories::schedule_week_repository::ScheduleWeekRepository;
use domain::repositories::subject_repository::SubjectRepository;
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::day_of_week::DayOfWeek;
use domain::value_objects::week_status::WeekStatus;
use logic::services::ScheduleService;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use uuid::Uuid;

#[derive(Default)]
struct MockRepositories {
    weeks: Mutex<Vec<ScheduleWeek>>,
}

impl MockRepositories {
    fn with_weeks(weeks: Vec<ScheduleWeek>) -> Self {
        Self {
            weeks: Mutex::new(weeks),
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
            .find(|week| week.week_start_date == date)
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

#[async_trait]
impl LessonInstanceRepository for MockRepositories {
    async fn get_by_id(&self, _: Uuid) -> Result<LessonInstance, DomainError> {
        Err(DomainError::LessonInstanceNotFound)
    }

    async fn get_by_week(&self, _: NaiveDate) -> Result<Vec<LessonInstance>, DomainError> {
        Ok(Vec::new())
    }

    async fn get_by_date(&self, _: NaiveDate) -> Result<Vec<LessonInstance>, DomainError> {
        Ok(Vec::new())
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
    async fn get_by_id(&self, _: Uuid) -> Result<LessonTemplate, DomainError> {
        Err(DomainError::LessonTemplateNotFound)
    }

    async fn get_by_lesson(&self, _: Uuid) -> Result<Vec<LessonTemplate>, DomainError> {
        Ok(Vec::new())
    }

    async fn get_active_for_day(
        &self,
        _: DayOfWeek,
    ) -> Result<Vec<LessonTemplate>, DomainError> {
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
    async fn get_by_id(&self, _: Uuid) -> Result<Lesson, DomainError> {
        Err(DomainError::LessonNotFound)
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
impl SubjectRepository for MockRepositories {
    async fn get_by_id(&self, _: Uuid) -> Result<Subject, DomainError> {
        Err(DomainError::SubjectNotFound)
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
    async fn get_by_id(&self, _: Uuid) -> Result<User, DomainError> {
        Err(DomainError::UserNotFound)
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

fn service_with(weeks: Vec<ScheduleWeek>) -> ScheduleService {
    let repositories = Arc::new(MockRepositories::with_weeks(weeks));
    ScheduleService::new(
        repositories.clone(),
        repositories.clone(),
        repositories.clone(),
        repositories.clone(),
        repositories.clone(),
        repositories.clone(),
        repositories.clone(),
        repositories,
    )
}

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("test date must be valid")
}

fn block_on<F: Future>(future: F) -> F::Output {
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

#[test]
fn create_draft_and_list_use_the_injected_repository() {
    let service = service_with(Vec::new());

    let created = block_on(service.create_draft(date(2026, 8, 31), None))
        .expect("draft creation should succeed");
    let weeks = block_on(service.list()).expect("list should succeed");

    assert_eq!(created.status, WeekStatus::Draft);
    assert_eq!(weeks, vec![created]);
}

#[test]
fn publish_changes_a_saved_week() {
    let service = service_with(vec![ScheduleWeek::new(
        date(2026, 8, 31),
        WeekStatus::Draft,
        None,
    )]);

    let published = block_on(service.publish(date(2026, 8, 31)))
        .expect("publishing should succeed");

    assert!(published.is_published());
    assert!(block_on(service.list())
        .expect("list should succeed")
        .first()
        .expect("published week should exist")
        .is_published());
}

#[test]
fn publish_missing_week_returns_a_domain_error() {
    let service = service_with(Vec::new());

    let result = block_on(service.publish(date(2026, 8, 31)));

    assert_eq!(result, Err(DomainError::ScheduleWeekNotFound));
}

#[test]
fn day_schedule_does_not_expose_a_draft_week() {
    let service = service_with(vec![ScheduleWeek::new(
        date(2026, 8, 31),
        WeekStatus::Draft,
        None,
    )]);

    let result = block_on(service.day_schedule(date(2026, 9, 2)));

    assert_eq!(result, Err(DomainError::ScheduleWeekNotFound));
}
