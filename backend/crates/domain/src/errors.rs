//! Domain-level errors.
//!
//! Guarantees: All possible business logic errors are enumerated here.
//! This ensures at compile time that no erroneous state is ignored
//! (using `Result` is mandatory). No panics are allowed.

use std::fmt::{self};

/// Base domain error. Contains no implementation details (e.g., SQL errors).
#[derive(Debug, Clone, PartialEq)]
pub enum DomainError {
    // Unexpected internal failure (e.g., hashing, JWT signing, DB connection).
    /// Never exposed to the user; presentation layer maps it to HTTP 500.
    InternalError,

    /// User with the specified ID or login was not found.
    UserNotFound,

    /// Uniqueness violation (e.g., registration with an existing login).
    LoginAlreadyExists,

    /// Invalid login format.
    InvalidLoginFormat,

    /// Invalid email format.
    InvalidEmailFormat,

    /// Invalid name format (e.g., empty or whitespace-only string).
    InvalidNameFormat,

    /// Attempt to perform an action not allowed for the current role.
    InsufficientPermissions,

    /// User account is blocked or inactive.
    UserIsInactive,

    /// Class with the specified ID was not found.
    ClassNotFound,
    /// Invalid class letter (not 'б', 'в', or 'и').
    InvalidClassLetter,
    /// Class with the same (graduation_year, class_letter) already exists (unique violation).
    ClassAlreadyExists,

    /// Graduation year is out of acceptable bounds (e.g., < 1900 or > 2200).
    InvalidGraduationYear,

    /// Subject with the specified ID was not found.
    SubjectNotFound,
    /// Subject with the same name already exists (unique violation).
    SubjectAlreadyExists,
    /// Must be non-empty and max 100 characters (DB constraint).
    InvalidSubjectNameFormat,

    /// StudentGroup with the specified ID was not found.
    StudentGroupNotFound,
    /// Must be non-empty and max 100 characters (DB constraint).
    InvalidStudentGroupNameFormat,
    /// StudentGroup with the same (name) already exists (unique violation).
    StudentGroupAlreadyExists,

    /// Homework with the specified ID was not found.
    HomeworkNotFound,
    /// The lesson instance a homework refers to does not exist
    /// (FK violation on `homeworks.lesson_instance_id` during create).
    LessonInstanceNotFound,
    /// The homework a file is attached to does not exist
    /// (FK violation on `homework_files.homework_id` during `add_file`/`create_with_files`).
    HomeworkFileParentNotFound,
    /// Homework file with the specified ID was not found.
    HomeworkFileNotFound,
    /// A homework for this lesson instance already exists (unique violation on lesson_instance_id).
    HomeworkAlreadyExists,
    /// Homework text content must be non-empty if provided (empty/whitespace-only is rejected).
    InvalidHomeworkTextFormat,
    /// Homework file metadata must be non-empty and within DB limits (storage_key ≤ 500, file_name ≤ 255, mime_type ≤ 100 chars).
    InvalidHomeworkFileFormat,
    /// Homework file size must be non-negative (DB CHECK size_bytes >= 0).
    InvalidHomeworkFileSize,
    /// Unknown homework_status value in the database (cannot be parsed into HomeworkStatus).
    InvalidHomeworkStatus,
    /// Lesson with the specified ID was not found.
    LessonNotFound,
    /// Lesson with the same name already exists (unique violation).
    LessonAlreadyExists,
    // Lesson references a non-existent class, group, subject, or teacher.
    /// Raised when a foreign key constraint is violated during save or
    /// teacher assignment — the lesson data itself is structurally valid,
    /// but one of its references points to a missing entity.
    InvalidLessonReference,
    // Cabinet with the specified ID was not found
    CabinetNotFound,
    // Cabinet with the same number already exists (unique violation)
    CabinetAlreadyExists,
    // Cabinet number must be a number from 100 to 999
    InvalidCabinetNumber,
    // Cabinet description must be a string (size <= 256)
    InvalidCabinetDescription,
    // Cabinet capacity must be a natural number
    InvalidCabinetCapacity,

    // authentication failed
    InvalidCredentials,
    // LessonTemplate with the specified ID was not found.
    LessonTemplateNotFound,
    // A lesson cannot have two templates with the same (lesson_id, day, start_time, end_time, parity)
    // (unique violation on idx_lesson_templates_no_dup).
    LessonTemplateAlreadyExists,
    // Template end_time must be strictly after start_time (DB CHECK chk_template_time).
    InvalidLessonTemplateTime,
    // A new/updated ACTIVE template would overlap another ACTIVE template of the
    // same lesson at a parity-conflicting slot: Every conflicts with all parities,
    // Odd/Odd and Even/Even conflict; Odd/Even twins are the only allowed overlap.
    LessonTemplateSlotConflict,
    // Template references a non-existent lesson or cabinet (FK violation).
    InvalidLessonTemplateReference,
    // Unknown day_of_week value in the database (cannot be parsed into DayOfWeek).
    InvalidDayOfWeek,
    // Unknown week_parity value in the database (cannot be parsed into WeekParity).
    InvalidWeekParity,

    // ScheduleWeek with the specified start date was not found.
    ScheduleWeekNotFound,
    // Unknown schedule_weeks.status value (cannot be parsed into WeekStatus).
    InvalidWeekStatus,
    // Unknown lesson_instances.status value (cannot be parsed into LessonInstanceStatus).
    InvalidLessonInstanceStatus,
    // A template cannot produce two instances in the same week (unique violation
    // on idx_lesson_instances_unique).
    LessonInstanceAlreadyExists,
    // lesson_date must fall within [week_start_date, week_start_date + 7).
    InvalidLessonInstanceDate,

    // Event with the specified ID was not found.
    EventNotFound,
    // Event title must be non-empty and at most 255 chars (DB VARCHAR(255)).
    InvalidEventTitle,
    // Event end_time must be strictly after start_time (DB CHECK chk_event_time).
    InvalidEventTime,
    // No attendance row for the (event_id, student_id) pair (remove_attendee).
    EventAttendeeNotFound,

    // PlusnikSheet with the specified ID was not found.
    PlusnikSheetNotFound,
    // Sheet name must be non-empty and at most 255 chars.
    InvalidPlusnikSheetName,
    // Unknown sheet_status value in the database.
    InvalidSheetStatus,
    // Cannot delete a sheet that has plusnik records (FK ON DELETE RESTRICT).
    PlusnikSheetHasRecords,

    // PlusnikTask with the specified ID was not found.
    PlusnikTaskNotFound,
    // Task number must be non-empty and at most 20 chars.
    InvalidTaskNumber,
    // Two tasks with the same number in one sheet (unique index violation).
    PlusnikTaskAlreadyExists,
    // Cannot delete a task that has plusnik records (FK ON DELETE RESTRICT).
    PlusnikTaskHasRecords,

    // PlusnikRecord with the specified ID was not found.
    PlusnikRecordNotFound,
    // A record violates the chk_revoked_has_reviewer CHECK (revoked_at without revoked_by).
    InvalidPlusnikRecord,
    // An active plus for this (student_id, task_id) already exists.
    PlusnikRecordAlreadyExists,
    // task_id does not belong to sheet_id (trigger check_task_belongs_to_sheet).
    TaskNotInSheet,

    // Class Settings.rs
    // Invalid OrgEmailDomain
    InvalidOrgEmailDomain,
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Note: These are for logging. The presentation layer should map
        // these to localized Russian messages for the end user.
        match self {
            DomainError::InternalError => write!(f, "Internal error"),
            DomainError::UserNotFound => write!(f, "User not found"),
            DomainError::LoginAlreadyExists => write!(f, "Login already exists"),
            DomainError::InvalidLoginFormat => write!(f, "Invalid login format"),
            DomainError::InvalidNameFormat => write!(f, "Invalid name format"),
            DomainError::InvalidEmailFormat => write!(f, "Invalid email format"),
            DomainError::InsufficientPermissions => write!(f, "Insufficient permissions"),
            DomainError::UserIsInactive => write!(f, "User account is inactive"),
            DomainError::ClassNotFound => write!(f, "Class not found"),
            DomainError::InvalidClassLetter => write!(f, "Invalid class letter"),
            DomainError::ClassAlreadyExists => write!(f, "Class already exists"),
            DomainError::InvalidGraduationYear => write!(f, "Invalid graduation year"),
            DomainError::SubjectNotFound => write!(f, "Subject not found"),
            DomainError::SubjectAlreadyExists => write!(f, "Subject already exists"),
            DomainError::InvalidSubjectNameFormat => write!(f, "Invalid subject name format"),
            DomainError::StudentGroupNotFound => write!(f, "Student group not found"),
            DomainError::InvalidStudentGroupNameFormat => {
                write!(f, "Invalid student group name format")
            }
            DomainError::StudentGroupAlreadyExists => write!(f, "Student group already exists"),
            DomainError::HomeworkNotFound => write!(f, "Homework not found"),
            DomainError::LessonInstanceNotFound => write!(f, "Lesson instance not found"),
            DomainError::HomeworkFileParentNotFound => {
                write!(f, "Homework file parent homework not found")
            }
            DomainError::HomeworkFileNotFound => write!(f, "Homework file not found"),
            DomainError::HomeworkAlreadyExists => write!(f, "Homework already exists"),
            DomainError::InvalidHomeworkTextFormat => write!(f, "Invalid homework text format"),
            DomainError::InvalidHomeworkFileFormat => write!(f, "Invalid homework file format"),
            DomainError::InvalidHomeworkFileSize => write!(f, "Invalid homework file size"),
            DomainError::InvalidHomeworkStatus => write!(f, "Invalid homework status"),
            DomainError::LessonNotFound => write!(f, "Lesson not found"),
            DomainError::LessonAlreadyExists => write!(f, "Lesson already exists"),
            DomainError::InvalidLessonReference => write!(f, "Invalid lesson references"),
            DomainError::CabinetNotFound => write!(f, "Cabinet not found"),
            DomainError::CabinetAlreadyExists => write!(f, "Cabinet already exists"),
            DomainError::InvalidCabinetNumber => write!(f, "Invalid cabinet number"),
            DomainError::InvalidCabinetDescription => write!(f, "Invalid cabinet description"),
            DomainError::InvalidCabinetCapacity => write!(f, "Invalid cabinet capacity"),
            DomainError::InvalidCredentials => write!(f, "Authentication failed"),
            DomainError::LessonTemplateNotFound => write!(f, "Lesson template not found"),
            DomainError::LessonTemplateAlreadyExists => {
                write!(f, "Lesson template already exists")
            }
            DomainError::InvalidLessonTemplateTime => write!(f, "Invalid lesson template time"),
            DomainError::LessonTemplateSlotConflict => {
                write!(f, "Lesson template slot conflict")
            }
            DomainError::InvalidLessonTemplateReference => {
                write!(f, "Invalid lesson template references")
            }
            DomainError::InvalidDayOfWeek => write!(f, "Invalid day of week"),
            DomainError::InvalidWeekParity => write!(f, "Invalid week parity"),
            DomainError::ScheduleWeekNotFound => write!(f, "Schedule week not found"),
            DomainError::InvalidWeekStatus => write!(f, "Invalid week status"),
            DomainError::InvalidLessonInstanceStatus => {
                write!(f, "Invalid lesson instance status")
            }
            DomainError::LessonInstanceAlreadyExists => {
                write!(f, "Lesson instance already exists")
            }
            DomainError::InvalidLessonInstanceDate => write!(f, "Invalid lesson instance date"),
            DomainError::EventNotFound => write!(f, "Event not found"),
            DomainError::InvalidEventTitle => write!(f, "Invalid event title"),
            DomainError::InvalidEventTime => write!(f, "Invalid event time"),
            DomainError::EventAttendeeNotFound => write!(f, "Event attendee not found"),
            DomainError::PlusnikSheetNotFound => write!(f, "Plusnik sheet not found"),
            DomainError::InvalidPlusnikSheetName => write!(f, "Invalid plusnik sheet name"),
            DomainError::InvalidSheetStatus => write!(f, "Invalid sheet status"),
            DomainError::PlusnikSheetHasRecords => {
                write!(f, "Plusnik sheet has records, cannot delete")
            }
            DomainError::PlusnikTaskNotFound => write!(f, "Plusnik task not found"),
            DomainError::InvalidTaskNumber => write!(f, "Invalid task number"),
            DomainError::PlusnikTaskAlreadyExists => write!(f, "Plusnik task already exists"),
            DomainError::PlusnikTaskHasRecords => {
                write!(f, "Plusnik task has records, cannot delete")
            }
            DomainError::PlusnikRecordNotFound => write!(f, "Plusnik record not found"),
            DomainError::InvalidPlusnikRecord => write!(f, "Invalid plusnik record"),
            DomainError::PlusnikRecordAlreadyExists => {
                write!(f, "Plusnik record already exists")
            }
            DomainError::TaskNotInSheet => write!(f, "Task does not belong to sheet"),
            DomainError::InvalidOrgEmailDomain => write!(f, "Invalid ORG_EMAIL_DOMAIN"),
        }
    }
}

impl std::error::Error for DomainError {}
