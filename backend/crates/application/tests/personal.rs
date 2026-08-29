mod common;

use common::{date, service_with_personal_schedule};
use domain::entities::lesson::Lesson;
use domain::entities::lesson_instance::LessonInstance;
use domain::entities::lesson_template::LessonTemplate;
use domain::entities::subject::Subject;
use domain::entities::user::User;
use domain::value_objects::day_of_week::DayOfWeek;
use domain::value_objects::lesson_instance_status::LessonInstanceStatus;
use domain::value_objects::lesson_target::LessonTarget;
use domain::value_objects::role::UserRole;
use domain::value_objects::week_parity::WeekParity;
use uuid::Uuid;

#[tokio::test]
async fn returns_only_students_class_for_day_and_week() {
    let student_id = Uuid::new_v4();
    let student_class_id = Uuid::new_v4();
    let other_class_id = Uuid::new_v4();
    let student = User::try_new(
        student_id,
        "student@example.com".to_string(),
        UserRole::Student,
        "Student".to_string(),
        "Test".to_string(),
        None,
        Some(student_class_id),
    )
    .unwrap();

    let subject_id = Uuid::new_v4();
    let matching_lesson_id = Uuid::new_v4();
    let other_lesson_id = Uuid::new_v4();
    let matching_template = LessonTemplate::try_new(
        Uuid::new_v4(),
        matching_lesson_id,
        DayOfWeek::Mon,
        chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
        chrono::NaiveTime::from_hms_opt(9, 45, 0).unwrap(),
        WeekParity::Every,
        None,
        true,
    )
    .unwrap();
    let other_template = LessonTemplate::try_new(
        Uuid::new_v4(),
        other_lesson_id,
        DayOfWeek::Mon,
        chrono::NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
        chrono::NaiveTime::from_hms_opt(10, 45, 0).unwrap(),
        WeekParity::Every,
        None,
        true,
    )
    .unwrap();
    let week_start = date(2026, 8, 31);
    let instances = vec![
        LessonInstance::for_template(
            Uuid::new_v4(),
            &matching_template,
            week_start,
            LessonInstanceStatus::Scheduled,
            None,
        ),
        LessonInstance::for_template(
            Uuid::new_v4(),
            &other_template,
            week_start,
            LessonInstanceStatus::Scheduled,
            None,
        ),
    ];
    let lessons = vec![
        Lesson::new(
            matching_lesson_id,
            LessonTarget::Class(student_class_id),
            subject_id,
            true,
        ),
        Lesson::new(
            other_lesson_id,
            LessonTarget::Class(other_class_id),
            subject_id,
            true,
        ),
    ];
    let subject = Subject::try_new(subject_id, "Mathematics".to_string()).unwrap();
    let service = service_with_personal_schedule(
        student,
        instances,
        vec![matching_template, other_template],
        lessons,
        vec![subject],
    );

    let day = service.day_schedule(student_id, week_start).await.unwrap();
    assert_eq!(day.lessons.len(), 1);
    assert_eq!(day.lessons[0].lesson.id, matching_lesson_id);

    let week = service.current_week_schedule(student_id, week_start).await.unwrap();
    assert_eq!(week.days[0].lessons.len(), 1);
    assert_eq!(
        week.days.iter().map(|day| day.lessons.len()).sum::<usize>(),
        1
    );
}
