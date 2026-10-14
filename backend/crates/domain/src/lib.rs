//! Domain crate: сущности, value objects, трейты репозиториев

pub mod entities {
    pub mod cabinet;
    pub mod class;
    pub mod device_token;
    pub mod event;
    pub mod homework;
    pub mod lesson;
    pub mod lesson_instance;
    pub mod lesson_template;
    pub mod plusnik;
    pub mod schedule_week;
    pub mod student_group;
    pub mod subject;
    pub mod user;
    // pub mod message; TODO: сделать
}

pub mod value_objects {
    pub mod class_letter;
    pub mod day_of_week;
    pub mod homework_status;
    pub mod lesson_instance_status;
    pub mod lesson_target;
    pub mod login;
    pub mod notification_platform;
    pub mod role;
    pub mod sheet_status;
    pub mod telegram_session_state;
    pub mod week_parity;
    pub mod week_status;
}

pub mod errors;
pub mod ports;
pub mod repositories;
pub mod settings;
