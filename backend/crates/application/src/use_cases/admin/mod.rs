//! Admin panel use cases: users, lessons of a day (cancel, room, substitute),
//! schedule weeks, announcements, and the timetable itself (courses, regular slots,
//! one-off lessons).
//!
//! Dependencies: `domain` crate (entities, repositories), `ScheduleService`.
//! Guarantees: every method checks that the caller is an admin, returns `Result`, no panics.

pub mod admin_service;
pub mod timetable_service;

pub use admin_service::{
    AdminService, AnnouncementInput, LessonChange, NewUser, UserChange, WeekSource,
};
pub use timetable_service::{
    ClassTimetable, Course, Slot, SlotInput, TimetableChange, TimetableService,
};
