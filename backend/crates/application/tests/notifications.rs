//! Integration tests for homework push notifications against PostgreSQL, with a
//! recording `NotificationSender` instead of real push services.
//!
//! Run from `backend/` with `DATABASE_URL` set (role needs CREATEDB):
//!     cargo test -p application --test notifications

use std::sync::{Arc, Mutex};

use application::use_cases::notifications::notify_homework_changed::{
    HomeworkChange, NotifyHomeworkChangedUseCase,
};
use application::use_cases::notifications::push_subscriptions::PushSubscriptionsUseCase;
use application::use_cases::notifications::resolve_homework_recipients::ResolveHomeworkRecipientsUseCase;
use domain::errors::DomainError;
use domain::ports::notification_sender::{NotificationPayload, NotificationSender};
use domain::repositories::device_token_repository::DeviceTokenRepository;
use domain::value_objects::notification_platform::NotificationPlatform;
use domain::value_objects::web_push_subscription::WebPushSubscription;
use infrastructure::postgres::{
    DeviceTokenRepositoryPg, HomeworkRepositoryPg, LessonInstanceRepositoryPg, LessonRepositoryPg,
    LessonTemplateRepositoryPg, StudentGroupRepositoryPg, SubjectRepositoryPg, UserRepositoryPg,
};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Default)]
struct RecordingSender {
    sent: Mutex<Vec<NotificationPayload>>,
    failing_user: Option<Uuid>,
}

#[async_trait::async_trait]
impl NotificationSender for RecordingSender {
    async fn send_to_device(&self, _: &str, _: NotificationPlatform, _: &NotificationPayload) -> Result<(), DomainError> {
        unimplemented!()
    }
    async fn send_to_user(&self, user_id: Uuid, payload: &NotificationPayload) -> Result<(), DomainError> {
        if self.failing_user == Some(user_id) {
            return Err(DomainError::InternalError);
        }
        self.sent.lock().unwrap().push(payload.clone());
        Ok(())
    }
}

struct World {
    pool: PgPool,
    homework: Uuid,
    students: Vec<Uuid>,
    other_class_student: Uuid,
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

/// Algebra for 2027в on Wednesday 21.10.2026 with homework "§12, № 5–7".
async fn seed(pool: PgPool) -> World {
    let (class_id, other_class) = (Uuid::new_v4(), Uuid::new_v4());
    exec(&pool, "INSERT INTO classes (class_id, graduation_year, letter) VALUES ($1, 2027, 'в'), ($2, 2027, 'б')", &[class_id, other_class]).await;
    let subject = Uuid::new_v4();
    exec(&pool, "INSERT INTO subjects (subject_id, name) VALUES ($1, 'Алгебра')", &[subject]).await;
    let teacher = insert_user(&pool, "teacher", None).await;
    let students = vec![
        insert_user(&pool, "student", Some(class_id)).await,
        insert_user(&pool, "student", Some(class_id)).await,
        insert_user(&pool, "student", Some(class_id)).await,
    ];
    let other_class_student = insert_user(&pool, "student", Some(other_class)).await;

    let (lesson, template, instance, homework) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    exec(&pool, "INSERT INTO lessons (lesson_id, class_id, subject_id) VALUES ($1, $2, $3)", &[lesson, class_id, subject]).await;
    exec(&pool, "INSERT INTO schedule_weeks (week_start_date, status) VALUES ('2026-10-19', 'published')", &[]).await;
    exec(&pool, "INSERT INTO lesson_templates (template_id, lesson_id, day, start_time, end_time) VALUES ($1, $2, 'wed', '09:00', '09:45')", &[template, lesson]).await;
    exec(&pool, "INSERT INTO lesson_instances (instance_id, template_id, week_start_date, lesson_date) VALUES ($1, $2, '2026-10-19', '2026-10-21')", &[instance, template]).await;
    exec(
        &pool,
        "INSERT INTO homeworks (homework_id, lesson_instance_id, created_by, created_by_role, text_content, status)
         VALUES ($1, $2, $3, 'teacher', '§12,  № 5–7', 'published')",
        &[homework, instance, teacher],
    )
    .await;

    World {
        pool,
        homework,
        students,
        other_class_student,
    }
}

fn notifier(pool: &PgPool, sender: Arc<RecordingSender>) -> NotifyHomeworkChangedUseCase {
    let homeworks = Arc::new(HomeworkRepositoryPg::new(pool.clone()));
    let instances = Arc::new(LessonInstanceRepositoryPg::new(pool.clone()));
    let templates = Arc::new(LessonTemplateRepositoryPg::new(pool.clone()));
    let lessons = Arc::new(LessonRepositoryPg::new(pool.clone()));
    NotifyHomeworkChangedUseCase::new(
        homeworks.clone(),
        instances.clone(),
        templates.clone(),
        lessons.clone(),
        Arc::new(SubjectRepositoryPg::new(pool.clone())),
        ResolveHomeworkRecipientsUseCase::new(
            homeworks,
            instances,
            templates,
            lessons,
            Arc::new(StudentGroupRepositoryPg::new(pool.clone())),
            Arc::new(UserRepositoryPg::new(pool.clone())),
        ),
        sender,
    )
}

#[sqlx::test(migrations = "../../migrations")]
async fn class_students_get_it_except_the_author(pool: PgPool) {
    let w = seed(pool).await;
    let sender = Arc::new(RecordingSender::default());

    // The second student sent this homework to the bot: they are the author.
    let summary = notifier(&w.pool, sender.clone())
        .execute(w.homework, HomeworkChange::Created, Some(w.students[1]))
        .await
        .unwrap();

    let sent = sender.sent.lock().unwrap();
    let mut recipients: Vec<_> = sent.iter().map(|p| p.user_id).collect();
    recipients.sort();
    let mut expected = vec![w.students[0], w.students[2]];
    expected.sort();
    assert_eq!(recipients, expected);
    assert!(!recipients.contains(&w.other_class_student));
    assert_eq!((summary.recipients, summary.failed), (2, 0));

    let payload = &sent[0];
    assert_eq!(payload.title, "Новое ДЗ · Алгебра");
    assert_eq!(payload.body, "К уроку 21.10: §12, № 5–7");
    assert_eq!(payload.data["url"], "/main");
    assert_eq!(payload.data["tag"], format!("homework-{}", w.homework));
}

#[sqlx::test(migrations = "../../migrations")]
async fn file_added_text_and_failures_are_counted(pool: PgPool) {
    let w = seed(pool).await;
    let sender = Arc::new(RecordingSender {
        failing_user: Some(w.students[0]),
        ..Default::default()
    });

    let summary = notifier(&w.pool, sender.clone())
        .execute(w.homework, HomeworkChange::FileAdded { file_name: "листок.pdf".into() }, None)
        .await
        .unwrap();

    assert_eq!((summary.recipients, summary.failed), (3, 1), "one failure must not stop the rest");
    let sent = sender.sent.lock().unwrap();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0].title, "ДЗ изменено · Алгебра");
    assert_eq!(sent[0].body, "К уроку 21.10: добавлен файл «листок.pdf»");
}

#[sqlx::test(migrations = "../../migrations")]
async fn subscribe_is_idempotent_and_unsubscribe_deactivates(pool: PgPool) {
    let w = seed(pool).await;
    let tokens = Arc::new(DeviceTokenRepositoryPg::new(w.pool.clone()));
    let subscriptions = PushSubscriptionsUseCase::new(tokens.clone());
    let student = w.students[0];
    let browser = WebPushSubscription::try_new(
        "https://fcm.googleapis.com/fcm/send/abc",
        "BNcRdreALRFXTkOOUHK1EtK2wtaz5Ry4YfYCA_0QTpQtUbVlUls0VJXg7A8u-Ts1XbjhazAkj7I99e8QcYP7DkM",
        "tBHItJI5svbpez7KI4CCXg",
    )
    .unwrap();

    subscriptions.subscribe(student, &browser).await.unwrap();
    subscriptions.subscribe(student, &browser).await.unwrap();
    let active = tokens.get_active_by_user(student).await.unwrap();
    assert_eq!(active.len(), 1, "the same browser twice is one subscription");
    assert_eq!(active[0].platform, NotificationPlatform::Web);
    assert_eq!(active[0].token, browser.to_token());

    assert!(subscriptions.unsubscribe(student, &browser).await.unwrap());
    assert!(!subscriptions.unsubscribe(student, &browser).await.unwrap());
    assert!(tokens.get_active_by_user(student).await.unwrap().is_empty());

    // Subscribing again (notifications turned back on) re-activates it.
    subscriptions.subscribe(student, &browser).await.unwrap();
    assert_eq!(tokens.get_active_by_user(student).await.unwrap().len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn file_removed_and_deleted_homework_are_announced(pool: PgPool) {
    let w = seed(pool).await;
    let sender = Arc::new(RecordingSender::default());
    let notifier = notifier(&w.pool, sender.clone());
    let instance: Uuid = sqlx::query_scalar("SELECT lesson_instance_id FROM homeworks WHERE homework_id = $1")
        .bind(w.homework)
        .fetch_one(&w.pool)
        .await
        .unwrap();

    notifier
        .execute_for_lesson(instance, w.homework, HomeworkChange::FileRemoved { file_name: "старый.pdf".into() }, None)
        .await
        .unwrap();
    // The homework row is gone: the notification must still reach the class.
    exec(&w.pool, "DELETE FROM homeworks WHERE homework_id = $1", &[w.homework]).await;
    let summary = notifier
        .execute_for_lesson(instance, w.homework, HomeworkChange::Deleted, None)
        .await
        .unwrap();

    assert_eq!(summary.recipients, 3);
    let sent = sender.sent.lock().unwrap();
    assert_eq!(sent[0].title, "ДЗ изменено · Алгебра");
    assert_eq!(sent[0].body, "К уроку 21.10: удалён файл «старый.pdf»");
    let last = sent.last().unwrap();
    assert_eq!(last.title, "ДЗ удалено · Алгебра");
    assert_eq!(last.body, "К уроку 21.10 задание отменено");
    assert_eq!(last.data["tag"], format!("homework-{}", w.homework), "replaces the earlier notification");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_browser_belongs_to_the_last_user_who_linked_it(pool: PgPool) {
    let w = seed(pool).await;
    let tokens = Arc::new(DeviceTokenRepositoryPg::new(w.pool.clone()));
    let subscriptions = PushSubscriptionsUseCase::new(tokens.clone());
    let (anna, boris) = (w.students[0], w.students[1]);
    let browser = WebPushSubscription::try_new(
        "https://fcm.googleapis.com/fcm/send/shared-computer",
        "BNcRdreALRFXTkOOUHK1EtK2wtaz5Ry4YfYCA_0QTpQtUbVlUls0VJXg7A8u-Ts1XbjhazAkj7I99e8QcYP7DkM",
        "tBHItJI5svbpez7KI4CCXg",
    )
    .unwrap();

    // Anna's session expires (no logout, so no unlinking); Boris logs in on the same browser.
    subscriptions.subscribe(anna, &browser).await.unwrap();
    subscriptions.subscribe(boris, &browser).await.unwrap();

    assert!(tokens.get_active_by_user(anna).await.unwrap().is_empty(), "Anna's homework must not reach Boris");
    assert_eq!(tokens.get_active_by_user(boris).await.unwrap().len(), 1);

    // Anna logs back in: the browser is hers again, and only hers.
    subscriptions.subscribe(anna, &browser).await.unwrap();
    assert_eq!(tokens.get_active_by_user(anna).await.unwrap().len(), 1);
    assert!(tokens.get_active_by_user(boris).await.unwrap().is_empty());
}
