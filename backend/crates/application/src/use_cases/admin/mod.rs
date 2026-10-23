//! Admin panel use cases: users, lessons of a day (cancel, room, substitute),
//! schedule weeks and announcements.
//!
//! Dependencies: `domain` crate (entities, repositories), `ScheduleService`.
//! Guarantees: every method checks that the caller is an admin, returns `Result`, no panics.

pub mod admin_service;

pub use admin_service::{
    AdminService, AnnouncementInput, LessonChange, NewUser, UserChange, WeekSource,
};
