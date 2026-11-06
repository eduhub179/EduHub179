//! Integration tests for `PlusnikService` against PostgreSQL (real repositories, real
//! constraints and triggers). Each test gets a fresh database from `sqlx::test`.
//!
//! Run from `backend/` with `DATABASE_URL` set (role needs CREATEDB):
//!     cargo test -p application --test plusnik

use std::sync::Arc;

use application::use_cases::plusnik::plusnik_service::{
    CreateSheetCommand, HistoryFilter, PlusBatchCommand, PlusCell, PlusnikService,
    UpdateSheetCommand,
};
use chrono::NaiveDate;
use domain::entities::user::User;
use domain::errors::DomainError;
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::sheet_status::SheetStatus;
use infrastructure::postgres::{
    ClassRepositoryPg, LessonRepositoryPg, PlusnikRepositoryPg, StudentGroupRepositoryPg,
    SubjectRepositoryPg, UserRepositoryPg,
};
use sqlx::PgPool;
use uuid::Uuid;

/// Seeded data: a class lesson with its teacher and students, plus outsiders.
struct World {
    service: PlusnikService,
    users: UserRepositoryPg,
    lesson_id: Uuid,
    group_lesson_id: Uuid,
    class_id: Uuid,
    other_class_id: Uuid,
    teacher: Uuid,
    other_teacher: Uuid,
    admin: Uuid,
    /// Students of the class lesson.
    anna: Uuid,
    boris: Uuid,
    /// Student of another class, member of the group of `group_lesson_id`.
    outsider: Uuid,
}

impl World {
    async fn user(&self, id: Uuid) -> User {
        self.users.get_by_id(id).await.unwrap()
    }
}

async fn exec(pool: &PgPool, sql: &str, ids: &[Uuid]) {
    let mut query = sqlx::query(sql);
    for id in ids {
        query = query.bind(*id);
    }
    query.execute(pool).await.unwrap();
}

async fn insert_user(pool: &PgPool, role: &str, last_name: &str, class_id: Option<Uuid>) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (user_id, login, role, last_name, first_name, class_id)
         VALUES ($1, $2, $3::user_role, $4, 'Тест', $5)",
    )
    .bind(id)
    .bind(format!("{role}.{id}@example.com"))
    .bind(role)
    .bind(last_name)
    .bind(class_id)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn seed(pool: PgPool) -> World {
    let (class_id, other_class_id) = (Uuid::new_v4(), Uuid::new_v4());
    exec(&pool, "INSERT INTO classes (class_id, graduation_year, letter) VALUES ($1, 2027, 'б'), ($2, 2027, 'в')", &[class_id, other_class_id]).await;
    let subject_id = Uuid::new_v4();
    exec(&pool, "INSERT INTO subjects (subject_id, name) VALUES ($1, 'Матанализ')", &[subject_id]).await;
    let group_id = Uuid::new_v4();
    exec(&pool, "INSERT INTO student_groups (group_id, name) VALUES ($1, 'Спецкурс')", &[group_id]).await;

    let teacher = insert_user(&pool, "teacher", "Учитель", None).await;
    let other_teacher = insert_user(&pool, "teacher", "Чужой", None).await;
    let admin = insert_user(&pool, "admin", "Админ", None).await;
    // Inserted out of alphabetical order: the matrix must sort by name.
    let boris = insert_user(&pool, "student", "Борисов", Some(class_id)).await;
    let anna = insert_user(&pool, "student", "Андреева", Some(class_id)).await;
    let outsider = insert_user(&pool, "student", "Другой", Some(other_class_id)).await;

    let (lesson_id, group_lesson_id) = (Uuid::new_v4(), Uuid::new_v4());
    exec(&pool, "INSERT INTO lessons (lesson_id, class_id, subject_id) VALUES ($1, $2, $3)", &[lesson_id, class_id, subject_id]).await;
    exec(&pool, "INSERT INTO lessons (lesson_id, group_id, subject_id) VALUES ($1, $2, $3)", &[group_lesson_id, group_id, subject_id]).await;
    exec(&pool, "INSERT INTO lesson_teachers (lesson_id, teacher_id) VALUES ($1, $3), ($2, $3)", &[lesson_id, group_lesson_id, teacher]).await;
    exec(&pool, "INSERT INTO group_members (student_id, group_id) VALUES ($1, $2)", &[outsider, group_id]).await;

    let service = PlusnikService::new(
        Arc::new(PlusnikRepositoryPg::new(pool.clone())),
        Arc::new(LessonRepositoryPg::new(pool.clone())),
        Arc::new(UserRepositoryPg::new(pool.clone())),
        Arc::new(StudentGroupRepositoryPg::new(pool.clone())),
        Arc::new(SubjectRepositoryPg::new(pool.clone())),
        Arc::new(ClassRepositoryPg::new(pool.clone())),
    );
    World {
        service,
        users: UserRepositoryPg::new(pool),
        lesson_id,
        group_lesson_id,
        class_id,
        other_class_id,
        teacher,
        other_teacher,
        admin,
        anna,
        boris,
        outsider,
    }
}

fn sheet_command(lesson_id: Uuid, status: SheetStatus, tasks: &[&str]) -> CreateSheetCommand {
    CreateSheetCommand {
        lesson_id,
        name: "Листок 1: Пределы".to_string(),
        issue_date: NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
        deadline: None,
        status,
        task_numbers: tasks.iter().map(|t| t.to_string()).collect(),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_teacher_lists_all_lessons(pool: PgPool) {
    let w = seed(pool).await;

    let lessons = w.service.teacher_lessons(&w.user(w.teacher).await, None).await.unwrap();
    // Not a teacher of any lesson, still sees (and manages) all of them.
    let others = w.service.teacher_lessons(&w.user(w.other_teacher).await, None).await.unwrap();
    let student = w.service.teacher_lessons(&w.user(w.anna).await, None).await;

    let names: Vec<_> = lessons.iter().map(|l| l.target_name.as_str()).collect();
    assert_eq!(names, vec!["2027б", "Спецкурс"]);
    assert_eq!(lessons[0].subject_name, "Матанализ");
    assert_eq!(others, lessons);
    assert_eq!(student.unwrap_err(), DomainError::InsufficientPermissions);
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_sheet_with_tasks_in_order(pool: PgPool) {
    let w = seed(pool).await;

    let created = w
        .service
        .create_sheet(&w.user(w.teacher).await, sheet_command(w.lesson_id, SheetStatus::Draft, &["1а", "1б", "2"]))
        .await
        .unwrap();

    let numbers: Vec<_> = created.tasks.iter().map(|t| t.task_number.as_str()).collect();
    assert_eq!(numbers, vec!["1а", "1б", "2"]);
    assert_eq!(created.sheet.created_by, w.teacher);
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_sheet_rejects_duplicates_before_saving(pool: PgPool) {
    let w = seed(pool).await;
    let teacher = w.user(w.teacher).await;

    let result = w
        .service
        .create_sheet(&teacher, sheet_command(w.lesson_id, SheetStatus::Draft, &["1", "2", "1"]))
        .await;

    assert_eq!(result.unwrap_err(), DomainError::PlusnikTaskAlreadyExists);
    assert!(w.service.lesson_sheets(&teacher, w.lesson_id).await.unwrap().is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn any_teacher_or_admin_manages_sheets(pool: PgPool) {
    let w = seed(pool).await;
    let command = sheet_command(w.lesson_id, SheetStatus::Published, &["1"]);
    let other_teacher = w.user(w.other_teacher).await;

    // A teacher who does not teach the lesson.
    let foreign = w.service.create_sheet(&other_teacher, command.clone()).await.unwrap();
    let plus = w.service.grant_plus(&other_teacher, foreign.tasks[0].id, w.anna).await;
    let student = w.service.create_sheet(&w.user(w.anna).await, command.clone()).await;
    let admin = w.service.create_sheet(&w.user(w.admin).await, command).await;
    let unknown = w
        .service
        .create_sheet(&other_teacher, sheet_command(Uuid::new_v4(), SheetStatus::Draft, &["1"]))
        .await;

    assert!(plus.is_ok());
    assert_eq!(student.unwrap_err(), DomainError::InsufficientPermissions);
    assert!(admin.is_ok());
    assert_eq!(unknown.unwrap_err(), DomainError::LessonNotFound);
}

#[sqlx::test(migrations = "../../migrations")]
async fn students_see_only_published_sheets_of_their_lessons(pool: PgPool) {
    let w = seed(pool).await;
    let teacher = w.user(w.teacher).await;
    w.service.create_sheet(&teacher, sheet_command(w.lesson_id, SheetStatus::Draft, &["1"])).await.unwrap();
    w.service.create_sheet(&teacher, sheet_command(w.lesson_id, SheetStatus::Published, &["1"])).await.unwrap();

    let for_teacher = w.service.lesson_sheets(&teacher, w.lesson_id).await.unwrap();
    let for_student = w.service.lesson_sheets(&w.user(w.anna).await, w.lesson_id).await.unwrap();
    let for_outsider = w.service.lesson_sheets(&w.user(w.outsider).await, w.lesson_id).await;

    assert_eq!(for_teacher.len(), 2);
    assert_eq!(for_student.len(), 1);
    assert_eq!(for_student[0].sheet.status, SheetStatus::Published);
    assert_eq!(for_outsider.unwrap_err(), DomainError::InsufficientPermissions);
}

#[sqlx::test(migrations = "../../migrations")]
async fn students_of_lesson_see_matrix_of_published_sheet(pool: PgPool) {
    let w = seed(pool).await;
    let teacher = w.user(w.teacher).await;
    let draft = w.service.create_sheet(&teacher, sheet_command(w.lesson_id, SheetStatus::Draft, &["1"])).await.unwrap();
    let published = w.service.create_sheet(&teacher, sheet_command(w.lesson_id, SheetStatus::Published, &["1"])).await.unwrap();
    let task = published.tasks[0].id;
    w.service.grant_plus(&teacher, task, w.boris).await.unwrap();

    let anna = w.user(w.anna).await;
    let matrix = w.service.sheet_matrix(&anna, published.sheet.id).await.unwrap();
    let hidden = w.service.sheet_matrix(&anna, draft.sheet.id).await;
    let outsider = w.service.sheet_matrix(&w.user(w.outsider).await, published.sheet.id).await;
    let other_teacher = w.service.sheet_matrix(&w.user(w.other_teacher).await, draft.sheet.id).await;

    // The whole lesson is visible, including other students' pluses.
    assert_eq!(matrix.students.len(), 2);
    assert_eq!(matrix.pluses.len(), 1);
    assert_eq!(matrix.pluses[0].student_id, w.boris);
    assert_eq!(hidden.unwrap_err(), DomainError::PlusnikSheetNotFound);
    assert_eq!(outsider.unwrap_err(), DomainError::InsufficientPermissions);
    // Any teacher sees any sheet, drafts included.
    assert!(other_teacher.is_ok());
}

#[sqlx::test(migrations = "../../migrations")]
async fn grant_and_revoke_plus_roundtrip(pool: PgPool) {
    let w = seed(pool).await;
    let teacher = w.user(w.teacher).await;
    let sheet = w
        .service
        .create_sheet(&teacher, sheet_command(w.lesson_id, SheetStatus::Published, &["1", "2"]))
        .await
        .unwrap();
    let task = sheet.tasks[0].id;

    let first = w.service.grant_plus(&teacher, task, w.anna).await.unwrap();
    let repeated = w.service.grant_plus(&teacher, task, w.anna).await.unwrap();
    assert!(first.created);
    assert!(!repeated.created, "a second tap must not fail");
    assert_eq!(repeated.record.id, first.record.id);

    let matrix = w.service.sheet_matrix(&teacher, sheet.sheet.id).await.unwrap();
    let names: Vec<_> = matrix.students.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["Андреева Тест", "Борисов Тест"]);
    assert_eq!(matrix.pluses.len(), 1);

    let revoked = w.service.revoke_plus(&teacher, task, w.anna, Some("ошибся".into())).await.unwrap();
    let revoked_again = w.service.revoke_plus(&teacher, task, w.anna, None).await.unwrap();
    assert!(revoked);
    assert!(!revoked_again);
    assert!(w.service.sheet_matrix(&teacher, sheet.sheet.id).await.unwrap().pluses.is_empty());

    // After revoking, the plus can be put again (a new record; history keeps the old one).
    assert!(w.service.grant_plus(&teacher, task, w.anna).await.unwrap().created);
}

#[sqlx::test(migrations = "../../migrations")]
async fn grant_requires_published_sheet_and_lesson_student(pool: PgPool) {
    let w = seed(pool).await;
    let teacher = w.user(w.teacher).await;
    let draft = w
        .service
        .create_sheet(&teacher, sheet_command(w.lesson_id, SheetStatus::Draft, &["1"]))
        .await
        .unwrap();
    let published = w
        .service
        .create_sheet(&teacher, sheet_command(w.lesson_id, SheetStatus::Published, &["1"]))
        .await
        .unwrap();

    let on_draft = w.service.grant_plus(&teacher, draft.tasks[0].id, w.anna).await;
    let to_outsider = w.service.grant_plus(&teacher, published.tasks[0].id, w.outsider).await;
    let by_student = w
        .service
        .grant_plus(&w.user(w.boris).await, published.tasks[0].id, w.anna)
        .await;

    assert_eq!(on_draft.unwrap_err(), DomainError::PlusnikSheetNotPublished);
    assert_eq!(to_outsider.unwrap_err(), DomainError::StudentNotInLesson);
    assert_eq!(by_student.unwrap_err(), DomainError::InsufficientPermissions);
}

#[sqlx::test(migrations = "../../migrations")]
async fn group_lesson_uses_group_members(pool: PgPool) {
    let w = seed(pool).await;
    let teacher = w.user(w.teacher).await;
    let sheet = w
        .service
        .create_sheet(&teacher, sheet_command(w.group_lesson_id, SheetStatus::Published, &["1"]))
        .await
        .unwrap();

    let matrix = w.service.sheet_matrix(&teacher, sheet.sheet.id).await.unwrap();
    let granted = w.service.grant_plus(&teacher, sheet.tasks[0].id, w.outsider).await;
    let class_student = w.service.grant_plus(&teacher, sheet.tasks[0].id, w.anna).await;

    assert_eq!(matrix.students.iter().map(|s| s.id).collect::<Vec<_>>(), vec![w.outsider]);
    assert!(granted.is_ok());
    assert_eq!(class_student.unwrap_err(), DomainError::StudentNotInLesson);
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_add_and_delete(pool: PgPool) {
    let w = seed(pool).await;
    let teacher = w.user(w.teacher).await;
    let sheet = w
        .service
        .create_sheet(&teacher, sheet_command(w.lesson_id, SheetStatus::Draft, &["1"]))
        .await
        .unwrap();
    let sheet_id = sheet.sheet.id;

    let updated = w
        .service
        .update_sheet(
            &teacher,
            sheet_id,
            UpdateSheetCommand {
                name: Some("Листок 1: Пределы и ряды".into()),
                status: Some(SheetStatus::Published),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.sheet.name, "Листок 1: Пределы и ряды");
    assert_eq!(updated.sheet.status, SheetStatus::Published);
    assert_eq!(updated.tasks.len(), 1);

    let added = w.service.add_task(&teacher, sheet_id, "2*".into()).await.unwrap();
    assert_eq!(added.sort_order, 1);
    assert_eq!(
        w.service.add_task(&teacher, sheet_id, "2*".into()).await.unwrap_err(),
        DomainError::PlusnikTaskAlreadyExists
    );

    // A task / sheet with pluses (even revoked ones) keeps its history: cannot be deleted.
    w.service.grant_plus(&teacher, sheet.tasks[0].id, w.boris).await.unwrap();
    w.service.revoke_plus(&teacher, sheet.tasks[0].id, w.boris, None).await.unwrap();
    assert_eq!(
        w.service.delete_task(&teacher, sheet.tasks[0].id).await.unwrap_err(),
        DomainError::PlusnikTaskHasRecords
    );
    assert_eq!(
        w.service.delete_sheet(&teacher, sheet_id).await.unwrap_err(),
        DomainError::PlusnikSheetHasRecords
    );
    w.service.delete_task(&teacher, added.id).await.unwrap();

    let empty = w
        .service
        .create_sheet(&teacher, sheet_command(w.lesson_id, SheetStatus::Draft, &["1"]))
        .await
        .unwrap();
    w.service.delete_sheet(&teacher, empty.sheet.id).await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn student_progress_counts_solved_tasks(pool: PgPool) {
    let w = seed(pool).await;
    let teacher = w.user(w.teacher).await;
    let sheet = w
        .service
        .create_sheet(&teacher, sheet_command(w.lesson_id, SheetStatus::Published, &["1", "2", "3"]))
        .await
        .unwrap();
    w.service.create_sheet(&teacher, sheet_command(w.lesson_id, SheetStatus::Draft, &["1"])).await.unwrap();
    w.service.grant_plus(&teacher, sheet.tasks[0].id, w.anna).await.unwrap();
    w.service.grant_plus(&teacher, sheet.tasks[2].id, w.anna).await.unwrap();
    w.service.grant_plus(&teacher, sheet.tasks[1].id, w.boris).await.unwrap();

    let progress = w.service.student_progress(&w.user(w.anna).await).await.unwrap();

    assert_eq!(progress.len(), 1, "draft sheets are hidden");
    assert_eq!(progress[0].subject_name, "Матанализ");
    assert_eq!(progress[0].total_tasks, 3);
    assert_eq!(progress[0].solved_task_ids, vec![sheet.tasks[0].id, sheet.tasks[2].id]);
    assert_eq!(
        w.service.student_progress(&teacher).await.unwrap_err(),
        DomainError::InsufficientPermissions
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_manages_plusnik_of_any_lesson(pool: PgPool) {
    let w = seed(pool).await;
    let admin = w.user(w.admin).await;

    // The admin teaches nothing, yet sees every lesson of the school.
    let lessons = w.service.teacher_lessons(&admin, None).await.unwrap();
    let ids: Vec<_> = lessons.iter().map(|l| l.lesson_id).collect();
    assert!(ids.contains(&w.lesson_id) && ids.contains(&w.group_lesson_id));
    assert_eq!(ids.len(), 2);

    // Full cycle on a lesson taught by someone else.
    let sheet = w
        .service
        .create_sheet(&admin, sheet_command(w.lesson_id, SheetStatus::Published, &["1"]))
        .await
        .unwrap();
    let task = sheet.tasks[0].id;
    assert_eq!(w.service.lesson_sheets(&admin, w.lesson_id).await.unwrap().len(), 1);
    assert!(w.service.grant_plus(&admin, task, w.anna).await.unwrap().created);
    let matrix = w.service.sheet_matrix(&admin, sheet.sheet.id).await.unwrap();
    assert_eq!(matrix.pluses.len(), 1);
    assert_eq!(matrix.pluses[0].granted_by, w.admin);
    assert!(w.service.revoke_plus(&admin, task, w.anna, None).await.unwrap());

    // Students still get no lesson list.
    assert_eq!(
        w.service.teacher_lessons(&w.user(w.anna).await, None).await.unwrap_err(),
        DomainError::InsufficientPermissions
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn lessons_filtered_by_class_include_its_students_groups(pool: PgPool) {
    let w = seed(pool).await;
    let teacher = w.user(w.teacher).await;
    let admin = w.user(w.admin).await;
    let ids = |lessons: Vec<application::use_cases::plusnik::plusnik_service::LessonSummary>| {
        lessons.into_iter().map(|l| l.lesson_id).collect::<Vec<_>>()
    };

    // 2027б: only its whole-class lesson (nobody from it is in the group).
    let own_class = w.service.teacher_lessons(&teacher, Some(w.class_id)).await.unwrap();
    // 2027в: no class lessons, but its student attends the group lesson.
    let other_class = w.service.teacher_lessons(&teacher, Some(w.other_class_id)).await.unwrap();
    let admin_other = w.service.teacher_lessons(&admin, Some(w.other_class_id)).await.unwrap();
    let other_teacher = w
        .service
        .teacher_lessons(&w.user(w.other_teacher).await, Some(w.class_id))
        .await
        .unwrap();

    assert_eq!(ids(own_class), vec![w.lesson_id]);
    assert_eq!(ids(other_class), vec![w.group_lesson_id]);
    assert_eq!(ids(admin_other), vec![w.group_lesson_id]);
    assert_eq!(ids(other_teacher), vec![w.lesson_id], "any teacher sees every lesson of the class");
    assert_eq!(
        w.service.teacher_lessons(&teacher, Some(Uuid::new_v4())).await.unwrap_err(),
        DomainError::ClassNotFound
    );
}

// ============================================================================
// Batch task editing, batch pluses, course summary, history
// ============================================================================

fn numbers(tasks: &[domain::entities::plusnik::PlusnikTask]) -> Vec<&str> {
    tasks.iter().map(|t| t.task_number.as_str()).collect()
}

fn cell(task_id: Uuid, student_id: Uuid) -> PlusCell {
    PlusCell { task_id, student_id }
}

#[sqlx::test(migrations = "../../migrations")]
async fn tasks_insert_in_the_middle_and_in_batches(pool: PgPool) {
    let w = seed(pool).await;
    let teacher = w.user(w.teacher).await;
    let sheet = w.service.create_sheet(&teacher, sheet_command(w.lesson_id, SheetStatus::Draft, &["1", "3"])).await.unwrap();
    let id = sheet.sheet.id;

    let two = w.service.insert_task(&teacher, id, "2".into(), Some(1)).await.unwrap();
    assert_eq!(two.task_number, "2");
    let added = w.service.add_tasks(&teacher, id, vec!["4а".into(), "4б".into()], None).await.unwrap();
    assert_eq!(numbers(&added.tasks), vec!["1", "2", "3", "4а", "4б"]);
    let first = w.service.add_tasks(&teacher, id, vec!["0".into()], Some(0)).await.unwrap();
    assert_eq!(numbers(&first.tasks), vec!["0", "1", "2", "3", "4а", "4б"]);
    let orders: Vec<i32> = first.tasks.iter().map(|t| t.sort_order).collect();
    assert_eq!(orders, vec![0, 1, 2, 3, 4, 5]);

    // A duplicate (with the sheet or inside the batch) saves nothing.
    let clash = w.service.add_tasks(&teacher, id, vec!["5".into(), "1".into()], None).await;
    let repeat = w.service.add_tasks(&teacher, id, vec!["6".into(), "6".into()], None).await;
    let empty = w.service.add_tasks(&teacher, id, vec![], None).await;
    assert_eq!(clash.unwrap_err(), DomainError::PlusnikTaskAlreadyExists);
    assert_eq!(repeat.unwrap_err(), DomainError::PlusnikTaskAlreadyExists);
    assert_eq!(empty.unwrap_err(), DomainError::InvalidPlusnikBatch);
    let tasks = w.service.sheet_matrix(&teacher, id).await.unwrap().tasks;
    assert_eq!(tasks.len(), 6);

    let student = w.service.add_tasks(&w.user(w.anna).await, id, vec!["7".into()], None).await;
    assert_eq!(student.unwrap_err(), DomainError::InsufficientPermissions);
}

#[sqlx::test(migrations = "../../migrations")]
async fn tasks_rename_reorder_and_batch_delete(pool: PgPool) {
    let w = seed(pool).await;
    let teacher = w.user(w.teacher).await;
    let sheet = w.service.create_sheet(&teacher, sheet_command(w.lesson_id, SheetStatus::Published, &["1", "2", "3"])).await.unwrap();
    let id = sheet.sheet.id;
    let [t1, t2, t3] = [sheet.tasks[0].id, sheet.tasks[1].id, sheet.tasks[2].id];
    w.service.grant_plus(&teacher, t1, w.anna).await.unwrap();

    // Rename keeps the pluses; a taken number is a conflict.
    let renamed = w.service.rename_task(&teacher, t1, " 1* ".into()).await.unwrap();
    assert_eq!(renamed.task_number, "1*");
    assert_eq!(w.service.rename_task(&teacher, t1, "2".into()).await.unwrap_err(), DomainError::PlusnikTaskAlreadyExists);
    assert_eq!(w.service.sheet_matrix(&teacher, id).await.unwrap().pluses.len(), 1);

    let reordered = w.service.reorder_tasks(&teacher, id, vec![t3, t1, t2]).await.unwrap();
    assert_eq!(numbers(&reordered), vec!["3", "1*", "2"]);
    let missing = w.service.reorder_tasks(&teacher, id, vec![t3, t1]).await;
    let doubled = w.service.reorder_tasks(&teacher, id, vec![t3, t1, t1]).await;
    assert_eq!(missing.unwrap_err(), DomainError::InvalidTaskOrder);
    assert_eq!(doubled.unwrap_err(), DomainError::InvalidTaskOrder);

    // All or nothing: t1 has a plus, so t2 stays too.
    let blocked = w.service.delete_tasks(&teacher, id, vec![t1, t2]).await;
    assert_eq!(blocked.unwrap_err(), DomainError::PlusnikTaskHasRecords);
    let foreign = w.service.delete_tasks(&teacher, id, vec![t2, Uuid::new_v4()]).await;
    assert_eq!(foreign.unwrap_err(), DomainError::PlusnikTaskNotFound);
    assert_eq!(w.service.sheet_matrix(&teacher, id).await.unwrap().tasks.len(), 3);

    let rest = w.service.delete_tasks(&teacher, id, vec![t2, t3]).await.unwrap();
    assert_eq!(numbers(&rest), vec!["1*"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn batch_pluses_grant_and_revoke_in_one_call(pool: PgPool) {
    let w = seed(pool).await;
    let teacher = w.user(w.teacher).await;
    let sheet = w.service.create_sheet(&teacher, sheet_command(w.lesson_id, SheetStatus::Published, &["1", "2"])).await.unwrap();
    let id = sheet.sheet.id;
    let (t1, t2) = (sheet.tasks[0].id, sheet.tasks[1].id);
    w.service.grant_plus(&teacher, t2, w.boris).await.unwrap();

    // The whole column of task 1, plus removing boris's task 2.
    let result = w
        .service
        .apply_pluses(&teacher, id, PlusBatchCommand {
            grant: vec![cell(t1, w.anna), cell(t1, w.boris)],
            revoke: vec![cell(t2, w.boris)],
            comment: Some("пересдача".into()),
        })
        .await
        .unwrap();
    assert_eq!((result.granted, result.revoked), (2, 1));
    assert_eq!(result.pluses.len(), 2);
    assert!(result.pluses.iter().all(|p| p.task_id == t1));

    // Repeat-safe: nothing changes the second time.
    let again = w
        .service
        .apply_pluses(&teacher, id, PlusBatchCommand {
            grant: vec![cell(t1, w.anna)],
            revoke: vec![cell(t2, w.boris)],
            comment: None,
        })
        .await
        .unwrap();
    assert_eq!((again.granted, again.revoked), (0, 0));

    let batch = |grant: Vec<PlusCell>, revoke: Vec<PlusCell>| PlusBatchCommand { grant, revoke, comment: None };
    let outsider = w.service.apply_pluses(&teacher, id, batch(vec![cell(t2, w.anna), cell(t2, w.outsider)], vec![])).await;
    let contradiction = w.service.apply_pluses(&teacher, id, batch(vec![cell(t2, w.anna)], vec![cell(t2, w.anna)])).await;
    let other_sheet = w.service.apply_pluses(&teacher, id, batch(vec![cell(Uuid::new_v4(), w.anna)], vec![])).await;
    let empty = w.service.apply_pluses(&teacher, id, batch(vec![], vec![])).await;
    let student = w.service.apply_pluses(&w.user(w.anna).await, id, batch(vec![cell(t2, w.anna)], vec![])).await;
    assert_eq!(outsider.unwrap_err(), DomainError::StudentNotInLesson);
    assert_eq!(contradiction.unwrap_err(), DomainError::InvalidPlusnikBatch);
    assert_eq!(other_sheet.unwrap_err(), DomainError::TaskNotInSheet);
    assert_eq!(empty.unwrap_err(), DomainError::InvalidPlusnikBatch);
    assert_eq!(student.unwrap_err(), DomainError::InsufficientPermissions);
    // The failed batches changed nothing: anna still has no plus for task 2.
    let matrix = w.service.sheet_matrix(&teacher, id).await.unwrap();
    assert!(!matrix.pluses.iter().any(|p| p.task_id == t2));

    // Grants need a published sheet, revokes do not.
    w.service.update_sheet(&teacher, id, UpdateSheetCommand { status: Some(SheetStatus::Archived), ..Default::default() }).await.unwrap();
    let on_archived = w.service.apply_pluses(&teacher, id, batch(vec![cell(t2, w.anna)], vec![])).await;
    assert_eq!(on_archived.unwrap_err(), DomainError::PlusnikSheetNotPublished);
    let revoke_archived = w.service.apply_pluses(&teacher, id, batch(vec![], vec![cell(t1, w.anna)])).await.unwrap();
    assert_eq!(revoke_archived.revoked, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn course_summary_counts_students_by_sheet(pool: PgPool) {
    let w = seed(pool).await;
    let teacher = w.user(w.teacher).await;
    let mut older = sheet_command(w.lesson_id, SheetStatus::Published, &["1", "2", "3"]);
    older.issue_date = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
    let older = w.service.create_sheet(&teacher, older).await.unwrap();
    let newer = w.service.create_sheet(&teacher, sheet_command(w.lesson_id, SheetStatus::Published, &["1", "2"])).await.unwrap();
    w.service.create_sheet(&teacher, sheet_command(w.lesson_id, SheetStatus::Draft, &["1"])).await.unwrap();

    w.service.grant_plus(&teacher, older.tasks[0].id, w.anna).await.unwrap();
    w.service.grant_plus(&teacher, older.tasks[1].id, w.anna).await.unwrap();
    w.service.grant_plus(&teacher, newer.tasks[0].id, w.anna).await.unwrap();
    w.service.grant_plus(&teacher, newer.tasks[1].id, w.boris).await.unwrap();
    w.service.revoke_plus(&teacher, newer.tasks[1].id, w.boris, None).await.unwrap();

    let summary = w.service.course_summary(&teacher, w.lesson_id).await.unwrap();
    let sheet_ids: Vec<Uuid> = summary.sheets.iter().map(|s| s.sheet.id).collect();
    assert_eq!(sheet_ids, vec![older.sheet.id, newer.sheet.id], "drafts out, oldest first");
    assert_eq!(summary.sheets.iter().map(|s| s.total_tasks).collect::<Vec<_>>(), vec![3, 2]);
    assert_eq!(summary.total_tasks, 5);
    assert_eq!(summary.students[0].id, w.anna);
    assert_eq!(summary.students[0].solved, vec![2, 1]);
    assert_eq!(summary.students[0].solved_total, 3);
    assert_eq!(summary.students[1].solved, vec![0, 0], "revoked pluses do not count");

    let student = w.service.course_summary(&w.user(w.anna).await, w.lesson_id).await;
    assert_eq!(student.unwrap_err(), DomainError::InsufficientPermissions);
}

#[sqlx::test(migrations = "../../migrations")]
async fn sheet_history_keeps_revoked_pluses_with_names(pool: PgPool) {
    let w = seed(pool).await;
    let teacher = w.user(w.teacher).await;
    let admin = w.user(w.admin).await;
    let sheet = w.service.create_sheet(&teacher, sheet_command(w.lesson_id, SheetStatus::Published, &["1", "2"])).await.unwrap();
    let id = sheet.sheet.id;
    let (t1, t2) = (sheet.tasks[0].id, sheet.tasks[1].id);
    w.service.grant_plus(&teacher, t1, w.anna).await.unwrap();
    w.service.revoke_plus(&admin, t1, w.anna, Some("не та задача".into())).await.unwrap();
    w.service.grant_plus(&teacher, t2, w.boris).await.unwrap();

    let all = w.service.sheet_history(&teacher, id, HistoryFilter::default()).await.unwrap();
    assert_eq!(all.len(), 2);
    let revoked = all.iter().find(|e| e.record.task_id == t1).unwrap();
    assert_eq!(revoked.task_number, "1");
    assert_eq!(revoked.student_name, "Андреева Тест");
    assert_eq!(revoked.granted_by_name, "Учитель Тест");
    assert_eq!(revoked.revoked_by_name.as_deref(), Some("Админ Тест"));
    assert_eq!(revoked.record.revoke_comment.as_deref(), Some("не та задача"));

    let boris_only = w.service.sheet_history(&teacher, id, HistoryFilter { student_id: Some(w.boris), task_id: None }).await.unwrap();
    assert_eq!(boris_only.len(), 1);
    assert_eq!(boris_only[0].record.task_id, t2);
    let none = w.service.sheet_history(&teacher, id, HistoryFilter { student_id: Some(w.boris), task_id: Some(t1) }).await.unwrap();
    assert!(none.is_empty());

    let student = w.service.sheet_history(&w.user(w.anna).await, id, HistoryFilter::default()).await;
    assert_eq!(student.unwrap_err(), DomainError::InsufficientPermissions);
}

#[sqlx::test(migrations = "../../migrations")]
async fn students_cannot_edit_tasks(pool: PgPool) {
    let w = seed(pool).await;
    let teacher = w.user(w.teacher).await;
    let sheet = w.service.create_sheet(&teacher, sheet_command(w.lesson_id, SheetStatus::Published, &["1", "2"])).await.unwrap();
    let id = sheet.sheet.id;
    let (t1, t2) = (sheet.tasks[0].id, sheet.tasks[1].id);

    // A student of the course itself: sees the sheet, still cannot change it.
    let anna = w.user(w.anna).await;
    let rename = w.service.rename_task(&anna, t1, "1*".into()).await;
    let reorder = w.service.reorder_tasks(&anna, id, vec![t2, t1]).await;
    let delete = w.service.delete_tasks(&anna, id, vec![t2]).await;
    let insert = w.service.insert_task(&anna, id, "3".into(), Some(0)).await;
    assert_eq!(rename.unwrap_err(), DomainError::InsufficientPermissions);
    assert_eq!(reorder.unwrap_err(), DomainError::InsufficientPermissions);
    assert_eq!(delete.unwrap_err(), DomainError::InsufficientPermissions);
    assert_eq!(insert.unwrap_err(), DomainError::InsufficientPermissions);

    let tasks = w.service.sheet_matrix(&teacher, id).await.unwrap().tasks;
    assert_eq!(numbers(&tasks), vec!["1", "2"], "nothing changed");
}
