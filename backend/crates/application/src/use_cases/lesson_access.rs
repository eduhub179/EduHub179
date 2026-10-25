//! Who may do what with a course (a `Lesson`: subject + class/group).
//!
//! Shared by features attached to courses (plusnik, course materials), so the access
//! rules live in one place:
//! - manage: any teacher or admin, for every course (teachers substitute each other,
//!   so access is not limited to `lesson_teachers`);
//! - audience: active students of the course's class, or members of its group;
//! - courses of a class: its whole-class courses plus group courses attended by its
//!   students (the same rule as the class schedule).
//!
//! Dependencies: `domain` repositories.
//! Guarantees: all methods return `Result`, no panics.

use std::collections::HashSet;
use std::sync::Arc;

use domain::{
    entities::{lesson::Lesson, user::User},
    errors::DomainError,
    repositories::{
        class_repository::ClassRepository, lesson_repository::LessonRepository,
        student_group_repository::StudentGroupRepository,
        subject_repository::SubjectRepository, user_repository::UserRepository,
    },
    value_objects::{lesson_target::LessonTarget, role::UserRole},
};
use uuid::Uuid;

/// Display names of a course: "Матанализ" + "2027в" (or the group name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseNames {
    pub subject_name: String,
    pub target_name: String,
}

#[derive(Clone)]
pub struct LessonAccess {
    lessons: Arc<dyn LessonRepository>,
    users: Arc<dyn UserRepository>,
    groups: Arc<dyn StudentGroupRepository>,
    subjects: Arc<dyn SubjectRepository>,
    classes: Arc<dyn ClassRepository>,
}

impl LessonAccess {
    pub fn new(
        lessons: Arc<dyn LessonRepository>,
        users: Arc<dyn UserRepository>,
        groups: Arc<dyn StudentGroupRepository>,
        subjects: Arc<dyn SubjectRepository>,
        classes: Arc<dyn ClassRepository>,
    ) -> Self {
        Self {
            lessons,
            users,
            groups,
            subjects,
            classes,
        }
    }

    /// Any teacher or admin. The course must exist.
    ///
    /// # Fail-safe поведение
    /// `LessonNotFound` for an unknown course, whatever the role.
    pub async fn can_manage(&self, user: &User, lesson_id: Uuid) -> Result<bool, DomainError> {
        self.lessons.get_by_id(lesson_id).await?;
        Ok(matches!(user.role, UserRole::Teacher | UserRole::Admin))
    }

    /// `InsufficientPermissions` unless `can_manage`.
    pub async fn require_manager(&self, user: &User, lesson_id: Uuid) -> Result<(), DomainError> {
        if self.can_manage(user, lesson_id).await? {
            Ok(())
        } else {
            Err(DomainError::InsufficientPermissions)
        }
    }

    /// Is the student in the course's class or group?
    pub async fn is_in_audience(&self, student_id: Uuid, lesson: &Lesson) -> Result<bool, DomainError> {
        match lesson.target {
            LessonTarget::Class(class_id) => match self.users.get_by_id(student_id).await {
                Ok(student) => Ok(student.role.is_student()
                    && student.is_active
                    && student.class_id == Some(class_id)),
                Err(DomainError::UserNotFound) => Ok(false),
                Err(error) => Err(error),
            },
            LessonTarget::Group(group_id) => self.groups.has_member(group_id, student_id).await,
        }
    }

    /// Active students of the course's class or group.
    pub async fn lesson_students(&self, lesson: &Lesson) -> Result<Vec<User>, DomainError> {
        match lesson.target {
            LessonTarget::Class(class_id) => self.users.get_active_students_by_class(class_id).await,
            LessonTarget::Group(group_id) => {
                let mut students = Vec::new();
                for student_id in self.groups.get_member_ids(group_id).await? {
                    match self.users.get_by_id(student_id).await {
                        Ok(user) if user.is_active && user.role.is_student() => students.push(user),
                        Ok(_) | Err(DomainError::UserNotFound) => {}
                        Err(error) => return Err(error),
                    }
                }
                Ok(students)
            }
        }
    }

    /// Whole-class courses of the class plus courses of every group that has at least
    /// one active student of the class. Deduplicated by id.
    ///
    /// # Fail-safe поведение
    /// `ClassNotFound` for an unknown class.
    pub async fn class_lessons(&self, class_id: Uuid) -> Result<Vec<Lesson>, DomainError> {
        self.classes.get_by_id(class_id).await?;

        let mut lessons = self.lessons.get_by_class(class_id).await?;
        let mut group_ids = HashSet::new();
        for student in self.users.get_active_students_by_class(class_id).await? {
            for group in self.groups.get_groups_by_student(student.id).await? {
                group_ids.insert(group.id);
            }
        }
        for group_id in group_ids {
            lessons.extend(self.lessons.get_by_group(group_id).await?);
        }

        let mut seen = HashSet::new();
        lessons.retain(|lesson| seen.insert(lesson.id));
        Ok(lessons)
    }

    /// Courses of all active classes and all groups (the repository has no "get all";
    /// every course belongs to exactly one class or group). Deduplicated by id.
    pub async fn all_lessons(&self) -> Result<Vec<Lesson>, DomainError> {
        let mut lessons = Vec::new();
        for class in self.classes.get_all_active().await? {
            lessons.extend(self.lessons.get_by_class(class.id).await?);
        }
        for group in self.groups.get_all().await? {
            lessons.extend(self.lessons.get_by_group(group.id).await?);
        }
        let mut seen = HashSet::new();
        lessons.retain(|lesson| seen.insert(lesson.id));
        Ok(lessons)
    }

    pub async fn get_lesson(&self, lesson_id: Uuid) -> Result<Lesson, DomainError> {
        self.lessons.get_by_id(lesson_id).await
    }

    pub async fn lessons_of_teacher(&self, teacher_id: Uuid) -> Result<Vec<Lesson>, DomainError> {
        self.lessons.get_by_teacher(teacher_id).await
    }

    /// "Матанализ" + "2027в" (or the group name) for a course.
    pub async fn names(&self, lesson: &Lesson) -> Result<CourseNames, DomainError> {
        let subject_name = self.subjects.get_by_id(lesson.subject_id).await?.name;
        let target_name = match lesson.target {
            LessonTarget::Class(class_id) => self.classes.get_by_id(class_id).await?.full_name(),
            LessonTarget::Group(group_id) => self.groups.get_by_id(group_id).await?.name,
        };
        Ok(CourseNames {
            subject_name,
            target_name,
        })
    }
}
