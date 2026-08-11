//! Domain crate: сущности, value objects, трейты репозиториев

pub mod entities {
    pub mod class;
    pub mod lesson;
    pub mod student_group;
    pub mod subject;
    pub mod user;
    // pub mod homework; TODO: сделать
    // pub mod plusnik_record; TODO: сделать
    // pub mod message; TODO: сделать
}

pub mod value_objects {
    pub mod class_letter;
    pub mod lesson_target;
    pub mod role;
    // pub mod deadline; TODO: сделать
}

pub mod repositories;

pub mod errors;
