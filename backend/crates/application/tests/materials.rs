//! Integration tests for `MaterialsService` against PostgreSQL (real repositories and
//! constraints) with an in-memory file storage.
//!
//! Run from `backend/` with `DATABASE_URL` set (role needs CREATEDB):
//!     cargo test -p application --test materials

use std::sync::Arc;

use application::use_cases::homework::upload_file::UploadHomeworkFileCommand;
use application::use_cases::lesson_access::LessonAccess;
use application::use_cases::materials::materials_service::{
    AddLinkCommand, MaterialsService, UpdateMaterialCommand, UploadMaterialCommand,
};
use domain::entities::lesson_material::MaterialContent;
use domain::entities::user::User;
use domain::errors::DomainError;
use domain::ports::file_storage::FileStorage;
use domain::repositories::lesson_material_repository::MaterialTopicFilter;
use domain::repositories::user_repository::UserRepository;
use infrastructure::postgres::{
    ClassRepositoryPg, LessonMaterialRepositoryPg, LessonRepositoryPg, StudentGroupRepositoryPg,
    SubjectRepositoryPg, UserRepositoryPg,
};
use infrastructure::storage::InMemoryFileStorage;
use sqlx::PgPool;
use uuid::Uuid;

struct World {
    pool: PgPool,
    users: UserRepositoryPg,
    class_id: Uuid,
    lesson: Uuid,
    group_lesson: Uuid,
    teacher: Uuid,
    other_teacher: Uuid,
    admin: Uuid,
    student: Uuid,
    outsider: Uuid,
}

impl World {
    async fn user(&self, id: Uuid) -> User {
        self.users.get_by_id(id).await.unwrap()
    }

    fn service(&self, storage: Option<Arc<dyn FileStorage>>) -> MaterialsService {
        let pool = &self.pool;
        MaterialsService::new(
            Arc::new(LessonMaterialRepositoryPg::new(pool.clone())),
            LessonAccess::new(
                Arc::new(LessonRepositoryPg::new(pool.clone())),
                Arc::new(UserRepositoryPg::new(pool.clone())),
                Arc::new(StudentGroupRepositoryPg::new(pool.clone())),
                Arc::new(SubjectRepositoryPg::new(pool.clone())),
                Arc::new(ClassRepositoryPg::new(pool.clone())),
            ),
            storage,
        )
    }
}

async fn exec(pool: &PgPool, sql: &str, ids: &[Uuid]) {
    let mut query = sqlx::query(sql);
    for id in ids {
        query = query.bind(*id);
    }
    query.execute(pool).await.unwrap();
}

async fn insert_user(pool: &PgPool, role: &str, class_id: Option<Uuid>) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (user_id, login, role, last_name, first_name, class_id)
         VALUES ($1, $2, $3::user_role, 'Тест', 'Тест', $4)",
    )
    .bind(id)
    .bind(format!("{role}_{}", id.simple()))
    .bind(role)
    .bind(class_id)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// 2027в: Матанализ (class course, taught by `teacher`) and a "Спецкурс" group course
/// attended by `outsider` (a student of 2027б).
async fn seed(pool: PgPool) -> World {
    let (class_id, other_class) = (Uuid::new_v4(), Uuid::new_v4());
    exec(&pool, "INSERT INTO classes (class_id, graduation_year, letter) VALUES ($1, 2027, 'в'), ($2, 2027, 'б')", &[class_id, other_class]).await;
    let (matan, special) = (Uuid::new_v4(), Uuid::new_v4());
    exec(&pool, "INSERT INTO subjects (subject_id, name) VALUES ($1, 'Матанализ'), ($2, 'Спецкурс')", &[matan, special]).await;
    let group_id = Uuid::new_v4();
    exec(&pool, "INSERT INTO student_groups (group_id, name) VALUES ($1, 'Спецкурс-1')", &[group_id]).await;

    let teacher = insert_user(&pool, "teacher", None).await;
    let other_teacher = insert_user(&pool, "teacher", None).await;
    let admin = insert_user(&pool, "admin", None).await;
    let student = insert_user(&pool, "student", Some(class_id)).await;
    let outsider = insert_user(&pool, "student", Some(other_class)).await;

    let (lesson, group_lesson) = (Uuid::new_v4(), Uuid::new_v4());
    exec(&pool, "INSERT INTO lessons (lesson_id, class_id, subject_id) VALUES ($1, $2, $3)", &[lesson, class_id, matan]).await;
    exec(&pool, "INSERT INTO lessons (lesson_id, group_id, subject_id) VALUES ($1, $2, $3)", &[group_lesson, group_id, special]).await;
    exec(&pool, "INSERT INTO lesson_teachers (lesson_id, teacher_id) VALUES ($1, $2)", &[lesson, teacher]).await;
    exec(&pool, "INSERT INTO group_members (student_id, group_id) VALUES ($1, $2)", &[outsider, group_id]).await;

    World {
        users: UserRepositoryPg::new(pool.clone()),
        pool,
        class_id,
        lesson,
        group_lesson,
        teacher,
        other_teacher,
        admin,
        student,
        outsider,
    }
}

fn link(lesson_id: Uuid, topic_id: Option<Uuid>, title: &str) -> AddLinkCommand {
    AddLinkCommand {
        lesson_id,
        topic_id,
        title: title.to_string(),
        url: "https://youtu.be/limits".to_string(),
        description: None,
    }
}

fn pdf(lesson_id: Uuid, topic_id: Option<Uuid>, title: Option<&str>) -> UploadMaterialCommand {
    UploadMaterialCommand {
        lesson_id,
        topic_id,
        title: title.map(str::to_string),
        description: Some("  ".to_string()),
        file: UploadHomeworkFileCommand::new(
            "Конспект пределы.pdf".to_string(),
            "application/pdf".to_string(),
            vec![1, 2, 3],
        ),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn class_courses_depend_on_role(pool: PgPool) {
    let w = seed(pool).await;
    let service = w.service(None);
    // Make the group course part of 2027в: one of its students joins the group.
    exec(&w.pool, "INSERT INTO group_members (student_id, group_id) SELECT $1, group_id FROM student_groups", &[w.student]).await;

    let for_teacher = service.class_courses(&w.user(w.teacher).await, w.class_id).await.unwrap();
    let for_other = service.class_courses(&w.user(w.other_teacher).await, w.class_id).await.unwrap();
    let for_student = service.class_courses(&w.user(w.student).await, w.class_id).await.unwrap();

    let view = |courses: Vec<application::use_cases::materials::materials_service::CourseSummary>| {
        courses.into_iter().map(|c| (c.subject_name, c.can_manage)).collect::<Vec<_>>()
    };
    assert_eq!(view(for_teacher), vec![("Матанализ".into(), true), ("Спецкурс".into(), true)]);
    // Not a teacher of these courses, still manages them.
    assert_eq!(view(for_other), vec![("Матанализ".into(), true), ("Спецкурс".into(), true)]);
    assert_eq!(view(for_student), vec![("Матанализ".into(), false), ("Спецкурс".into(), false)]);
    assert_eq!(
        service.class_courses(&w.user(w.teacher).await, Uuid::new_v4()).await.unwrap_err(),
        DomainError::ClassNotFound
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn topics_with_counts_and_materials_by_topic(pool: PgPool) {
    let w = seed(pool).await;
    let storage: Arc<dyn FileStorage> = Arc::new(InMemoryFileStorage::new());
    let service = w.service(Some(storage.clone()));
    let teacher = w.user(w.teacher).await;

    let limits = service.create_topic(&teacher, w.lesson, "Пределы").await.unwrap();
    let derivatives = service.create_topic(&teacher, w.lesson, "Производные").await.unwrap();
    assert_eq!((limits.sort_order, derivatives.sort_order), (0, 1));
    assert_eq!(
        service.create_topic(&teacher, w.lesson, "пределы").await.unwrap_err(),
        DomainError::MaterialTopicAlreadyExists
    );

    let video = service.add_link(&teacher, link(w.lesson, Some(limits.id), "Видеоразбор")).await.unwrap();
    let notes = service.upload_file(&teacher, pdf(w.lesson, Some(limits.id), None)).await.unwrap();
    service.add_link(&teacher, link(w.lesson, None, "Программа курса")).await.unwrap();

    // File: title defaults to the file name, blank description dropped, key under materials/.
    assert_eq!(notes.title, "Конспект пределы.pdf");
    assert_eq!(notes.description, None);
    assert_eq!(notes.sort_order, 1, "appended after the video");
    assert!(notes.storage_key().unwrap().starts_with("materials/"));

    let overview = service.topics(&teacher, w.lesson).await.unwrap();
    let counts: Vec<_> = overview.topics.iter().map(|t| (t.topic.name.as_str(), t.material_count)).collect();
    assert_eq!(counts, vec![("Пределы", 2), ("Производные", 0)]);
    assert_eq!((overview.without_topic_count, overview.total_count), (1, 3));

    let in_limits = service.materials(&teacher, w.lesson, MaterialTopicFilter::Topic(limits.id)).await.unwrap();
    assert_eq!(in_limits.iter().map(|m| m.id).collect::<Vec<_>>(), vec![video.id, notes.id]);

    // Download and delete the file; links have nothing to download.
    let downloaded = service.download(&w.user(w.student).await, notes.id).await.unwrap();
    assert_eq!((downloaded.bytes, downloaded.mime_type.as_str()), (vec![1, 2, 3], "application/pdf"));
    assert_eq!(service.download(&teacher, video.id).await.unwrap_err(), DomainError::MaterialIsNotAFile);
    let deleted = service.delete_material(&teacher, notes.id).await.unwrap();
    assert_eq!(deleted.orphaned_storage_key, None);
    assert_eq!(
        storage.download(notes.storage_key().unwrap()).await.unwrap_err(),
        DomainError::HomeworkFileNotFound,
        "the object is removed from storage"
    );

    // Deleting a topic keeps its materials under "Без темы".
    service.delete_topic(&teacher, limits.id).await.unwrap();
    let overview = service.topics(&teacher, w.lesson).await.unwrap();
    assert_eq!((overview.without_topic_count, overview.total_count), (2, 2));
}

#[sqlx::test(migrations = "../../migrations")]
async fn who_may_view_and_manage(pool: PgPool) {
    let w = seed(pool).await;
    let service = w.service(None);
    let teacher = w.user(w.teacher).await;
    service.add_link(&teacher, link(w.lesson, None, "Видео")).await.unwrap();

    // Viewing: students of the course and any staff; not other students.
    for viewer in [w.student, w.other_teacher, w.admin] {
        let list = service.materials(&w.user(viewer).await, w.lesson, MaterialTopicFilter::All).await;
        assert_eq!(list.unwrap().len(), 1);
    }
    assert_eq!(
        service.materials(&w.user(w.outsider).await, w.lesson, MaterialTopicFilter::All).await.unwrap_err(),
        DomainError::InsufficientPermissions
    );

    // Managing: any teacher (not only of this course) and admins; never students.
    let result = service.add_link(&w.user(w.student).await, link(w.lesson, None, "x")).await;
    assert_eq!(result.unwrap_err(), DomainError::InsufficientPermissions);
    assert!(service.add_link(&w.user(w.other_teacher).await, link(w.lesson, None, "Чужой учитель")).await.is_ok());
    assert!(service.create_topic(&w.user(w.admin).await, w.lesson, "Админская").await.is_ok());

    // The group course is visible to its member from another class.
    assert!(service.course(&w.user(w.outsider).await, w.group_lesson).await.is_ok());
}

#[sqlx::test(migrations = "../../migrations")]
async fn validation_and_foreign_topics(pool: PgPool) {
    let w = seed(pool).await;
    let service = w.service(None);
    let admin = w.user(w.admin).await;
    let foreign = service.create_topic(&admin, w.group_lesson, "Чужая").await.unwrap();

    let bad_url = AddLinkCommand { url: "javascript:alert(1)".into(), ..link(w.lesson, None, "x") };
    assert_eq!(service.add_link(&admin, bad_url).await.unwrap_err(), DomainError::InvalidMaterialUrl);
    assert_eq!(
        service.add_link(&admin, link(w.lesson, Some(foreign.id), "x")).await.unwrap_err(),
        DomainError::MaterialTopicNotFound
    );
    assert_eq!(
        service.materials(&admin, w.lesson, MaterialTopicFilter::Topic(foreign.id)).await.unwrap_err(),
        DomainError::MaterialTopicNotFound
    );
    // Without S3 files are refused, links still work.
    assert_eq!(
        service.upload_file(&admin, pdf(w.lesson, None, Some("x"))).await.unwrap_err(),
        DomainError::FileStorageUnavailable
    );
    assert!(service.add_link(&admin, link(w.lesson, None, "Видео")).await.is_ok());
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_moves_renames_and_clears(pool: PgPool) {
    let w = seed(pool).await;
    let service = w.service(None);
    let teacher = w.user(w.teacher).await;
    let topic = service.create_topic(&teacher, w.lesson, "Пределы").await.unwrap();
    let material = service
        .add_link(
            &teacher,
            AddLinkCommand { description: Some("старое".into()), ..link(w.lesson, Some(topic.id), "Видео") },
        )
        .await
        .unwrap();

    let updated = service
        .update_material(
            &teacher,
            material.id,
            UpdateMaterialCommand {
                title: Some("Видеоразбор №1".into()),
                description: Some(None),
                topic_id: Some(None),
                sort_order: Some(5),
            },
        )
        .await
        .unwrap();

    assert_eq!(updated.title, "Видеоразбор №1");
    assert_eq!((updated.description, updated.topic_id, updated.sort_order), (None, None, 5));
    assert_eq!(updated.content, MaterialContent::link("https://youtu.be/limits").unwrap());

    let renamed = service.update_topic(&teacher, topic.id, Some("Пределы функций"), None).await.unwrap();
    assert_eq!(renamed.name, "Пределы функций");
}
