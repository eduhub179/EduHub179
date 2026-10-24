//! Domain crate: сущности, value objects, трейты репозиториев

pub mod entities {
    pub mod announcement;
    pub mod cabinet;
    pub mod class;
    pub mod device_token;
    pub mod event;
    pub mod homework;
    pub mod lesson;
    pub mod lesson_instance;
    pub mod lesson_material;
    pub mod lesson_substitution;
    pub mod lesson_template;
    pub mod plusnik;
    pub mod schedule_week;
    pub mod student_group;
    pub mod subject;
    pub mod user;
    pub mod web_session;
    // pub mod message; TODO: сделать
}

pub mod value_objects {
    pub mod announcement_priority;
    pub mod announcement_status;
    pub mod announcements;
    pub mod class_letter;
    pub mod csrf_token_hash;
    pub mod day_of_week;
    pub mod homework_status;
    pub mod lesson_instance_status;
    pub mod lesson_target;
    pub mod login;
    pub mod notification_platform;
    pub mod role;
    pub mod session_token_hash;
    pub mod sheet_status;
    pub mod telegram_session_state;
    pub mod week_parity;
    pub mod web_push_subscription;
    pub mod week_status;
}

pub mod errors;
pub mod ports;
pub mod repositories;
pub mod settings;
