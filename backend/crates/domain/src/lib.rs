//! Domain crate: entities, value objects, repository traits

pub mod entities {
    pub mod user;
    pub mod class;
    pub mod subject;
    pub mod student_group;
    pub mod homework;
    // pub mod plusnik_record; TODO: implement
    // pub mod message; TODO: implement
}

pub mod value_objects {
    pub mod role;
    pub mod class_letter;
    pub mod homework_status;
    // pub mod deadline; TODO: implement

}

pub mod repositories;

pub mod errors;
