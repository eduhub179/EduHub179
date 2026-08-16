//! Domain crate: сущности, value objects, трейты репозиториев

pub mod entities {
    pub mod cabinet;
    pub mod class;
    pub mod homework;
    pub mod lesson;
    pub mod lesson_instance;
    pub mod lesson_template;
    pub mod schedule_week;
    pub mod student_group;
    pub mod subject;
    pub mod user;
    // pub mod plusnik_record; TODO: сделать
    // pub mod message; TODO: сделать
}

pub mod value_objects {
    pub mod class_letter;
    pub mod day_of_week;
    pub mod homework_status;
    pub mod lesson_instance_status;
    pub mod lesson_target;
    pub mod role;
    pub mod week_parity;
    pub mod week_status;
    // pub mod deadline; TODO: сделать
}

pub mod repositories;

pub mod errors;
