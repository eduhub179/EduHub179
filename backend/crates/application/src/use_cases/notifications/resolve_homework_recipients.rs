//! Use case: resolve the list of users who should receive notifications about a homework.
//!
//! This is intentionally a pure business-rule helper: it answers the question
//! "who are the recipients for this homework?" without sending anything.
//! The actual push delivery happens in `notify_new_homework.rs`.

use domain::errors::DomainError;
use domain::repositories::homework_repository::HomeworkRepository;
use domain::repositories::lesson_instance_repository::LessonInstanceRepository;
use domain::repositories::lesson_repository::LessonRepository;
use domain::repositories::lesson_template_repository::LessonTemplateRepository;
use domain::repositories::student_group_repository::StudentGroupRepository;
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::lesson_target::LessonTarget;
use std::sync::Arc;
use uuid::Uuid;

pub struct ResolveHomeworkRecipientsUseCase {
    homeworks: Arc<dyn HomeworkRepository>,
    lesson_instances: Arc<dyn LessonInstanceRepository>,
    lesson_templates: Arc<dyn LessonTemplateRepository>,
    lessons: Arc<dyn LessonRepository>,
    student_groups: Arc<dyn StudentGroupRepository>,
    users: Arc<dyn UserRepository>,
}

impl ResolveHomeworkRecipientsUseCase {
    pub fn new(
        homeworks: Arc<dyn HomeworkRepository>,
        lesson_instances: Arc<dyn LessonInstanceRepository>,
        lesson_templates: Arc<dyn LessonTemplateRepository>,
        lessons: Arc<dyn LessonRepository>,
        student_groups: Arc<dyn StudentGroupRepository>,
        users: Arc<dyn UserRepository>,
    ) -> Self {
        Self {
            homeworks,
            lesson_instances,
            lesson_templates,
            lessons,
            student_groups,
            users,
        }
    }

    pub async fn execute(&self, homework_id: Uuid) -> Result<Vec<Uuid>, DomainError> {
        let homework = self.homeworks.get_by_id(homework_id).await?;
        let instance = self
            .lesson_instances
            .get_by_id(homework.lesson_instance_id)
            .await?;
        let template = self.lesson_templates.get_by_id(instance.template_id).await?;
        let lesson = self.lessons.get_by_id(template.lesson_id).await?;

        match lesson.target {
            LessonTarget::Class(class_id) => {
                let students = self.users.get_active_students_by_class(class_id).await?;
                Ok(students.into_iter().map(|user| user.id).collect())
            }
            LessonTarget::Group(group_id) => {
                let member_ids = self.student_groups.get_member_ids(group_id).await?;
                let mut recipients = Vec::new();

                for user_id in member_ids {
                    match self.users.get_by_id(user_id).await {
                        Ok(user) if user.is_active => recipients.push(user.id),
                        Ok(_) => {}
                        Err(DomainError::UserNotFound) => {}
                        Err(err) => return Err(err),
                    }
                }

                Ok(recipients)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::entities::class::Class;
    use domain::entities::device_token::DeviceToken;
    use domain::entities::homework::Homework;
    use domain::entities::lesson::Lesson;
    use domain::entities::lesson_instance::LessonInstance;
    use domain::entities::lesson_template::LessonTemplate;
    use domain::entities::student_group::StudentGroup;
    use domain::entities::user::User;
    use domain::repositories::device_token_repository::DeviceTokenRepository;
    use domain::repositories::homework_repository::HomeworkRepository;
    use domain::repositories::lesson_instance_repository::LessonInstanceRepository;
    use domain::repositories::lesson_repository::LessonRepository;
    use domain::repositories::lesson_template_repository::LessonTemplateRepository;
    use domain::repositories::student_group_repository::StudentGroupRepository;
    use domain::repositories::user_repository::UserRepository;
    use domain::value_objects::{class_letter::ClassLetter, day_of_week::DayOfWeek, homework_status::HomeworkStatus, lesson_instance_status::LessonInstanceStatus, role::UserRole, week_parity::WeekParity};
    use chrono::{NaiveDate, NaiveTime, Utc};

    struct MockHomeworkRepo {
        homework: Homework,
    }

    #[async_trait::async_trait]
    impl HomeworkRepository for MockHomeworkRepo {
        async fn get_by_id(&self, _homework_id: Uuid) -> Result<Homework, DomainError> {
            Ok(self.homework.clone())
        }

        async fn get_by_lesson_instance(&self, _lesson_instance_id: Uuid) -> Result<Homework, DomainError> {
            unimplemented!()
        }

        async fn get_files(&self, _homework_id: Uuid) -> Result<Vec<domain::entities::homework::HomeworkFile>, DomainError> {
            Ok(Vec::new())
        }

        async fn save(&self, _homework: Homework) -> Result<Homework, DomainError> {
            unimplemented!()
        }

        async fn add_file(&self, _file: domain::entities::homework::HomeworkFile) -> Result<domain::entities::homework::HomeworkFile, DomainError> {
            unimplemented!()
        }

        async fn remove_file(&self, _file_id: Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }

        async fn delete(&self, _homework_id: Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }

        async fn create_with_files(
            &self,
            _homework: Homework,
            _files: Vec<domain::entities::homework::HomeworkFile>,
        ) -> Result<Homework, DomainError> {
            unimplemented!()
        }
    }

    struct MockLessonInstanceRepo {
        instance: LessonInstance,
    }

    #[async_trait::async_trait]
    impl LessonInstanceRepository for MockLessonInstanceRepo {
        async fn get_by_id(&self, _instance_id: Uuid) -> Result<LessonInstance, DomainError> {
            Ok(self.instance.clone())
        }

        async fn get_by_week(&self, _week_start_date: NaiveDate) -> Result<Vec<LessonInstance>, DomainError> {
            unimplemented!()
        }

        async fn get_by_date(&self, _lesson_date: NaiveDate) -> Result<Vec<LessonInstance>, DomainError> {
            unimplemented!()
        }

        async fn get_by_template(&self, _template_id: Uuid) -> Result<Vec<LessonInstance>, DomainError> {
            unimplemented!()
        }

        async fn save(&self, _instance: LessonInstance) -> Result<LessonInstance, DomainError> {
            unimplemented!()
        }
    }

    struct MockLessonTemplateRepo {
        template: LessonTemplate,
    }

    #[async_trait::async_trait]
    impl LessonTemplateRepository for MockLessonTemplateRepo {
        async fn get_by_id(&self, _template_id: Uuid) -> Result<LessonTemplate, DomainError> {
            Ok(self.template.clone())
        }

        async fn get_by_lesson(&self, _lesson_id: Uuid) -> Result<Vec<LessonTemplate>, DomainError> {
            unimplemented!()
        }

        async fn get_active_for_day(&self, _day: domain::value_objects::day_of_week::DayOfWeek) -> Result<Vec<LessonTemplate>, DomainError> {
            unimplemented!()
        }

        async fn get_all_active(&self) -> Result<Vec<LessonTemplate>, DomainError> {
            unimplemented!()
        }

        async fn save(&self, _template: LessonTemplate) -> Result<LessonTemplate, DomainError> {
            unimplemented!()
        }
    }

    struct MockLessonRepo {
        lesson: Lesson,
    }

    #[async_trait::async_trait]
    impl LessonRepository for MockLessonRepo {
        async fn get_by_id(&self, _lesson_id: Uuid) -> Result<Lesson, DomainError> {
            Ok(self.lesson.clone())
        }

        async fn save(&self, _lesson: Lesson) -> Result<Lesson, DomainError> {
            unimplemented!()
        }

        async fn get_by_class(&self, _class_id: Uuid) -> Result<Vec<Lesson>, DomainError> {
            unimplemented!()
        }

        async fn get_by_group(&self, _group_id: Uuid) -> Result<Vec<Lesson>, DomainError> {
            unimplemented!()
        }

        async fn get_by_teacher(&self, _teacher_id: Uuid) -> Result<Vec<Lesson>, DomainError> {
            unimplemented!()
        }

        async fn assign_teacher(&self, _lesson_id: Uuid, _teacher_id: Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }

        async fn unassign_teacher(&self, _lesson_id: Uuid, _teacher_id: Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }

        async fn get_teacher_ids(&self, _lesson_id: Uuid) -> Result<Vec<Uuid>, DomainError> {
            Ok(Vec::new())
        }
    }

    struct MockStudentGroupRepo;

    #[async_trait::async_trait]
    impl StudentGroupRepository for MockStudentGroupRepo {
        async fn get_by_id(&self, _group_id: Uuid) -> Result<StudentGroup, DomainError> {
            unimplemented!()
        }

        async fn get_all(&self) -> Result<Vec<StudentGroup>, DomainError> {
            unimplemented!()
        }

        async fn save(&self, _group: StudentGroup) -> Result<StudentGroup, DomainError> {
            unimplemented!()
        }

        async fn add_member(&self, _group_id: Uuid, _student_id: Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }

        async fn add_members(&self, _group_id: Uuid, _student_ids: &[Uuid]) -> Result<(), DomainError> {
            unimplemented!()
        }

        async fn remove_member(&self, _group_id: Uuid, _student_id: Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }

        async fn get_member_ids(&self, _group_id: Uuid) -> Result<Vec<Uuid>, DomainError> {
            Ok(vec![])
        }

        async fn get_groups_by_student(&self, _student_id: Uuid) -> Result<Vec<StudentGroup>, DomainError> {
            unimplemented!()
        }

        async fn has_member(&self, _group_id: Uuid, _student_id: Uuid) -> Result<bool, DomainError> {
            Ok(false)
        }
    }

    struct MockUserRepo {
        students: Vec<User>,
    }

    #[async_trait::async_trait]
    impl UserRepository for MockUserRepo {
        async fn get_by_id(&self, user_id: Uuid) -> Result<User, DomainError> {
            self.students
                .iter()
                .find(|user| user.id == user_id)
                .cloned()
                .ok_or(DomainError::UserNotFound)
        }

        async fn get_by_login(&self, _login: &str) -> Result<User, DomainError> {
            unimplemented!()
        }

        async fn get_active_students_by_class(&self, _class_id: Uuid) -> Result<Vec<User>, DomainError> {
            Ok(self.students.clone())
        }

        async fn save(&self, _user: User) -> Result<User, DomainError> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn resolves_students_for_class_target() {
        let class_id = Uuid::new_v4();
        let user_1 = User::try_new(
            Uuid::new_v4(),
            "student_1".to_string(),
            UserRole::Student,
            "Petrov".to_string(),
            "Ivan".to_string(),
            None,
            Some(class_id),
        )
        .unwrap();
        let user_2 = User::try_new(
            Uuid::new_v4(),
            "student_2".to_string(),
            UserRole::Student,
            "Sidorov".to_string(),
            "Petr".to_string(),
            None,
            Some(class_id),
        )
        .unwrap();

        let homework_id = Uuid::new_v4();
        let homework = Homework::try_new(
            homework_id,
            Uuid::new_v4(),
            Uuid::new_v4(),
            UserRole::Teacher,
            Some("Do exercises".to_string()),
            HomeworkStatus::Published,
            false,
            None,
            Utc::now(),
        )
        .unwrap();

        let lesson_instance = LessonInstance::try_new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            NaiveDate::from_ymd_opt(2026, 9, 7).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 8).unwrap(),
            LessonInstanceStatus::Scheduled,
            None,
        )
        .unwrap();

        let lesson = Lesson::new(Uuid::new_v4(), domain::value_objects::lesson_target::LessonTarget::Class(class_id), Uuid::new_v4(), true);
        let template = LessonTemplate::try_new(
            lesson_instance.template_id,
            lesson.id,
            DayOfWeek::Mon,
            NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            NaiveTime::from_hms_opt(9, 45, 0).unwrap(),
            WeekParity::Every,
            None,
            true,
        )
        .unwrap();

        let use_case = ResolveHomeworkRecipientsUseCase::new(
            Arc::new(MockHomeworkRepo { homework }),
            Arc::new(MockLessonInstanceRepo { instance: lesson_instance }),
            Arc::new(MockLessonTemplateRepo { template }),
            Arc::new(MockLessonRepo { lesson }),
            Arc::new(MockStudentGroupRepo),
            Arc::new(MockUserRepo { students: vec![user_1.clone(), user_2.clone()] }),
        );

        let recipients = use_case.execute(homework_id).await.unwrap();
        assert_eq!(recipients.len(), 2);
        assert!(recipients.contains(&user_1.id));
        assert!(recipients.contains(&user_2.id));
    }
}
