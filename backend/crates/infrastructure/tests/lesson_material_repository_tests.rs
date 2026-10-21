//! Integration tests for `LessonMaterialRepositoryPg` (real PostgreSQL, constraints, FKs).
//!
//! Run from `backend/` with `DATABASE_URL` set (role needs CREATEDB):
//!     cargo test -p infrastructure --test lesson_material_repository_tests

use chrono::Utc;
use domain::entities::lesson_material::{LessonMaterial, LessonMaterialTopic, MaterialContent};
use domain::errors::DomainError;
use domain::repositories::lesson_material_repository::{
    LessonMaterialRepository, MaterialTopicFilter,
};
use infrastructure::postgres::LessonMaterialRepositoryPg;
use sqlx::PgPool;
use uuid::Uuid;

struct Seed {
    lesson: Uuid,
    other_lesson: Uuid,
    teacher: Uuid,
}

async fn seed(pool: &PgPool) -> Seed {
    let (class_id, subject_id, other_subject) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let (lesson, other_lesson, teacher) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    for (sql, ids) in [
        ("INSERT INTO classes (class_id, graduation_year, letter) VALUES ($1, 2027, 'в')", vec![class_id]),
        ("INSERT INTO subjects (subject_id, name) VALUES ($1, 'Матанализ'), ($2, 'Физика')", vec![subject_id, other_subject]),
        ("INSERT INTO lessons (lesson_id, class_id, subject_id) VALUES ($1, $3, $4), ($2, $3, $5)", vec![lesson, other_lesson, class_id, subject_id, other_subject]),
        ("INSERT INTO users (user_id, login, role, last_name, first_name) VALUES ($1, 'teacher_m', 'teacher', 'Т', 'Т')", vec![teacher]),
    ] {
        let mut query = sqlx::query(sql);
        for id in ids {
            query = query.bind(id);
        }
        query.execute(pool).await.unwrap();
    }
    Seed {
        lesson,
        other_lesson,
        teacher,
    }
}

fn topic(lesson: Uuid, name: &str, order: i32, teacher: Uuid) -> LessonMaterialTopic {
    LessonMaterialTopic::try_new(Uuid::new_v4(), lesson, name, order, teacher, Utc::now()).unwrap()
}

fn link(lesson: Uuid, topic: Option<Uuid>, title: &str, order: i32, teacher: Uuid) -> LessonMaterial {
    LessonMaterial::try_new(
        Uuid::new_v4(),
        lesson,
        topic,
        title,
        Some("описание"),
        MaterialContent::link("https://example.com/video").unwrap(),
        order,
        teacher,
        Utc::now(),
    )
    .unwrap()
}

fn file(lesson: Uuid, topic: Option<Uuid>, title: &str, teacher: Uuid) -> LessonMaterial {
    LessonMaterial::try_new(
        Uuid::new_v4(),
        lesson,
        topic,
        title,
        None,
        MaterialContent::file("materials/1_a.pdf", "a.pdf", "application/pdf", 1234).unwrap(),
        0,
        teacher,
        Utc::now(),
    )
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn topics_are_ordered_and_names_unique_case_insensitive(pool: PgPool) {
    let s = seed(&pool).await;
    let repo = LessonMaterialRepositoryPg::new(pool);

    repo.save_topic(topic(s.lesson, "Производные", 1, s.teacher)).await.unwrap();
    let limits = repo.save_topic(topic(s.lesson, "Пределы", 0, s.teacher)).await.unwrap();
    // Same name in another course is fine.
    repo.save_topic(topic(s.other_lesson, "Пределы", 0, s.teacher)).await.unwrap();

    let names: Vec<_> = repo.get_topics(s.lesson).await.unwrap().into_iter().map(|t| t.name).collect();
    assert_eq!(names, vec!["Пределы", "Производные"]);
    assert_eq!(
        repo.save_topic(topic(s.lesson, "  ПРЕДЕЛЫ ", 5, s.teacher)).await.unwrap_err(),
        DomainError::MaterialTopicAlreadyExists
    );

    // Upsert renames in place.
    let mut renamed = limits.clone();
    renamed.name = "Пределы функций".into();
    assert_eq!(repo.save_topic(renamed).await.unwrap().name, "Пределы функций");
    assert_eq!(repo.get_topic(limits.id).await.unwrap().name, "Пределы функций");
    assert_eq!(repo.get_topic(Uuid::new_v4()).await.unwrap_err(), DomainError::MaterialTopicNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn materials_roundtrip_and_filters(pool: PgPool) {
    let s = seed(&pool).await;
    let repo = LessonMaterialRepositoryPg::new(pool);
    let topic = repo.save_topic(topic(s.lesson, "Пределы", 0, s.teacher)).await.unwrap();

    let video = repo.save_material(link(s.lesson, Some(topic.id), "Видеоразбор", 1, s.teacher)).await.unwrap();
    let pdf = repo.save_material(file(s.lesson, Some(topic.id), "Конспект", s.teacher)).await.unwrap();
    let loose = repo.save_material(link(s.lesson, None, "Программа курса", 0, s.teacher)).await.unwrap();

    assert_eq!(repo.get_material(pdf.id).await.unwrap(), pdf);
    assert_eq!(pdf.storage_key(), Some("materials/1_a.pdf"));
    assert_eq!(video.storage_key(), None);

    let titles = |items: Vec<LessonMaterial>| items.into_iter().map(|m| m.title).collect::<Vec<_>>();
    assert_eq!(
        titles(repo.get_materials(s.lesson, MaterialTopicFilter::Topic(topic.id)).await.unwrap()),
        vec!["Конспект", "Видеоразбор"]
    );
    assert_eq!(
        titles(repo.get_materials(s.lesson, MaterialTopicFilter::WithoutTopic).await.unwrap()),
        vec!["Программа курса"]
    );
    assert_eq!(repo.get_materials(s.lesson, MaterialTopicFilter::All).await.unwrap().len(), 3);
    assert!(repo.get_materials(s.other_lesson, MaterialTopicFilter::All).await.unwrap().is_empty());

    let mut counts = repo.count_materials_by_topic(s.lesson).await.unwrap();
    counts.sort_by_key(|(topic_id, _)| topic_id.is_some());
    assert_eq!(counts, vec![(None, 1), (Some(topic.id), 2)]);

    // Upsert changes title/topic but keeps content.
    let mut moved = loose.clone();
    moved.topic_id = Some(topic.id);
    moved.title = "Программа".into();
    let moved = repo.save_material(moved).await.unwrap();
    assert_eq!((moved.topic_id, moved.title.as_str()), (Some(topic.id), "Программа"));
    assert_eq!(moved.content, loose.content);

    assert_eq!(repo.delete_material(pdf.id).await.unwrap().id, pdf.id);
    assert_eq!(repo.delete_material(pdf.id).await.unwrap_err(), DomainError::MaterialNotFound);
    assert_eq!(repo.get_material(pdf.id).await.unwrap_err(), DomainError::MaterialNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn topic_of_another_course_is_rejected_by_the_db(pool: PgPool) {
    let s = seed(&pool).await;
    let repo = LessonMaterialRepositoryPg::new(pool);
    let foreign = repo.save_topic(topic(s.other_lesson, "Кинематика", 0, s.teacher)).await.unwrap();

    let result = repo.save_material(link(s.lesson, Some(foreign.id), "Чужая тема", 0, s.teacher)).await;

    assert_eq!(result.unwrap_err(), DomainError::MaterialTopicNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn deleting_topic_moves_materials_to_without_topic(pool: PgPool) {
    let s = seed(&pool).await;
    let repo = LessonMaterialRepositoryPg::new(pool);
    let topic = repo.save_topic(topic(s.lesson, "Пределы", 0, s.teacher)).await.unwrap();
    let material = repo.save_material(link(s.lesson, Some(topic.id), "Видео", 0, s.teacher)).await.unwrap();

    repo.delete_topic(topic.id).await.unwrap();

    let kept = repo.get_material(material.id).await.unwrap();
    assert_eq!(kept.topic_id, None);
    assert_eq!(kept.lesson_id, s.lesson, "SET NULL (topic_id) must not touch lesson_id");
    assert_eq!(repo.delete_topic(topic.id).await.unwrap_err(), DomainError::MaterialTopicNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn db_checks_back_up_the_domain(pool: PgPool) {
    let s = seed(&pool).await;

    // A row with both a URL and a file breaks chk_lesson_materials_one_content.
    let both = sqlx::query(
        "INSERT INTO lesson_materials (lesson_id, title, url, storage_key, file_name, mime_type, size_bytes, created_by)
         VALUES ($1, 't', 'https://e.com', 'k', 'n', 'm', 1, $2)",
    )
    .bind(s.lesson)
    .bind(s.teacher)
    .execute(&pool)
    .await;
    let empty_name = sqlx::query(
        "INSERT INTO lesson_material_topics (lesson_id, name, created_by) VALUES ($1, '   ', $2)",
    )
    .bind(s.lesson)
    .bind(s.teacher)
    .execute(&pool)
    .await;

    assert!(both.is_err());
    assert!(empty_name.is_err());
}
