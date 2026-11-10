//! A file attached to both lessons of a double lesson is ONE stored object with a record
//! in each homework. Integration tests against PostgreSQL with in-memory storage.
//!
//! Run from `backend/` with `DATABASE_URL` set (role needs CREATEDB):
//!     cargo test -p application --test shared_files

use std::sync::Arc;

use application::use_cases::homework::{
    attach_file_to_lesson::{AttachFileToLessonCommand, AttachFileToLessonUseCase},
    delete_file::{DeleteHomeworkFileCommand, DeleteHomeworkFileUseCase},
    delete_homework::DeleteHomeworkUseCase,
    editor::HomeworkEditor,
    upload_file::UploadHomeworkFileCommand,
};
use domain::entities::user::User;
use domain::errors::DomainError;
use domain::ports::file_storage::FileStorage;
use domain::repositories::homework_repository::HomeworkRepository;
use domain::value_objects::role::UserRole;
use infrastructure::postgres::HomeworkRepositoryPg;
use infrastructure::storage::InMemoryFileStorage;
use sqlx::PgPool;
use uuid::Uuid;

struct World {
    homeworks: Arc<HomeworkRepositoryPg>,
    storage: Arc<InMemoryFileStorage>,
    attach: AttachFileToLessonUseCase,
    delete_file: DeleteHomeworkFileUseCase,
    delete_homework: DeleteHomeworkUseCase,
    editor: HomeworkEditor,
    /// The two halves of a double lesson.
    first: Uuid,
    second: Uuid,
}

async fn exec(pool: &PgPool, sql: &str, ids: &[Uuid]) {
    let mut query = sqlx::query(sql);
    for id in ids {
        query = query.bind(*id);
    }
    query.execute(pool).await.unwrap();
}

async fn seed(pool: PgPool) -> World {
    let (class_id, subject_id, lesson_id) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    exec(&pool, "INSERT INTO classes (class_id, graduation_year, letter) VALUES ($1, 2027, 'б')", &[class_id]).await;
    exec(&pool, "INSERT INTO subjects (subject_id, name) VALUES ($1, 'Алгебра')", &[subject_id]).await;
    exec(&pool, "INSERT INTO lessons (lesson_id, class_id, subject_id) VALUES ($1, $2, $3)", &[lesson_id, class_id, subject_id]).await;
    exec(&pool, "INSERT INTO schedule_weeks (week_start_date, status) VALUES ('2026-10-19', 'published')", &[]).await;
    let (t1, t2, first, second) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    exec(&pool, "INSERT INTO lesson_templates (template_id, lesson_id, day, start_time, end_time) VALUES ($1, $3, 'mon', '09:00', '09:45'), ($2, $3, 'mon', '09:55', '10:40')", &[t1, t2, lesson_id]).await;
    exec(&pool, "INSERT INTO lesson_instances (instance_id, template_id, week_start_date, lesson_date) VALUES ($1, $3, '2026-10-19', '2026-10-19'), ($2, $4, '2026-10-19', '2026-10-19')", &[first, second, t1, t2]).await;

    let teacher_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (user_id, login, role, last_name, first_name) VALUES ($1, 'teacher_shared', 'teacher', 'Учитель', 'Тест')")
        .bind(teacher_id)
        .execute(&pool)
        .await
        .unwrap();
    let teacher = User::try_new(teacher_id, "teacher_shared".into(), UserRole::Teacher, "Учитель".into(), "Тест".into(), None, None).unwrap();

    let homeworks = Arc::new(HomeworkRepositoryPg::new(pool));
    let storage = Arc::new(InMemoryFileStorage::new());
    World {
        attach: AttachFileToLessonUseCase::new(homeworks.clone(), storage.clone()),
        delete_file: DeleteHomeworkFileUseCase::new(homeworks.clone(), storage.clone()),
        delete_homework: DeleteHomeworkUseCase::new(homeworks.clone(), Some(storage.clone())),
        editor: HomeworkEditor::authorize(&teacher, false).unwrap(),
        homeworks,
        storage,
        first,
        second,
    }
}

impl World {
    /// Attaches "sheet.pdf" to the first half and shares it with the second.
    async fn attach_shared(&self) -> (Uuid, String) {
        let shared = self
            .attach
            .execute_shared(
                AttachFileToLessonCommand {
                    lesson_instance_id: self.first,
                    editor: self.editor,
                    file: UploadHomeworkFileCommand::new("sheet.pdf".into(), "application/pdf".into(), b"%PDF-1.4".to_vec()),
                },
                &[self.second],
            )
            .await
            .unwrap();
        assert!(shared.failed.is_empty());
        (shared.main.file.id, shared.main.file.storage_key)
    }

    async fn records(&self, key: &str) -> usize {
        self.homeworks.get_files_by_storage_key(key).await.unwrap().len()
    }

    async fn object_exists(&self, key: &str) -> bool {
        self.storage.download(key).await.is_ok()
    }

    async fn homework_of(&self, lesson: Uuid) -> Option<Uuid> {
        match self.homeworks.get_by_lesson_instance(lesson).await {
            Ok(homework) => Some(homework.id),
            Err(DomainError::HomeworkNotFound) => None,
            Err(error) => panic!("{error}"),
        }
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn one_object_two_records_deleted_together(pool: PgPool) {
    let w = seed(pool).await;
    let (file_id, key) = w.attach_shared().await;

    assert_eq!(w.records(&key).await, 2, "a record in each half of the double");
    assert!(w.homework_of(w.second).await.is_some(), "the second half got homework too");

    let deleted = w
        .delete_file
        .execute(DeleteHomeworkFileCommand { file_id, editor: w.editor, with_shared_copies: true })
        .await
        .unwrap();

    assert_eq!(deleted.shared_copies_removed, 1);
    assert_eq!(w.records(&key).await, 0);
    assert!(!w.object_exists(&key).await, "the object is gone with its last record");
    // Both homeworks had only this file: both are gone.
    assert!(deleted.homework_deleted);
    assert!(w.homework_of(w.first).await.is_none());
    assert!(w.homework_of(w.second).await.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn object_stays_while_another_record_uses_it(pool: PgPool) {
    let w = seed(pool).await;
    let (file_id, key) = w.attach_shared().await;

    // Only this lesson's record (no `with_shared_copies`): the other half keeps the file.
    w.delete_file
        .execute(DeleteHomeworkFileCommand { file_id, editor: w.editor, with_shared_copies: false })
        .await
        .unwrap();

    assert_eq!(w.records(&key).await, 1);
    assert!(w.object_exists(&key).await, "still attached to the second half");
}

#[sqlx::test(migrations = "../../migrations")]
async fn deleting_one_homework_keeps_the_shared_object(pool: PgPool) {
    let w = seed(pool).await;
    let (_, key) = w.attach_shared().await;
    let first = w.homework_of(w.first).await.unwrap();
    let second = w.homework_of(w.second).await.unwrap();

    w.delete_homework.execute(w.editor, first).await.unwrap();
    assert!(w.object_exists(&key).await, "the second half still uses it");

    w.delete_homework.execute(w.editor, second).await.unwrap();
    assert!(!w.object_exists(&key).await, "no record left: the object is deleted");
}
