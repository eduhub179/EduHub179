//! Domain crate: сущности, value objects, трейты репозиториев

pub mod entities {
    pub mod user;
    pub mod subject;
    pub mod homework;
    pub mod plusnik_record;
    pub mod message;
}

pub mod value_objects {
    pub mod deadline;
    pub mod role;
}

pub mod repositories;

pub mod errors;
