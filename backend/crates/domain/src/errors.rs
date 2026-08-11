//! Domain-level errors.
//!
//! Guarantees: All possible business logic errors are enumerated here.
//! This ensures at compile time that no erroneous state is ignored
//! (using `Result` is mandatory). No panics are allowed.

use std::fmt;

/// Base domain error. Contains no implementation details (e.g., SQL errors).
#[derive(Debug, Clone, PartialEq)]
pub enum DomainError {
    /// User with the specified ID or email was not found.
    UserNotFound,

    /// Uniqueness violation (e.g., registration with an existing email).
    EmailAlreadyExists,

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
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Note: These are for logging. The presentation layer should map
        // these to localized Russian messages for the end user.
        match self {
            DomainError::UserNotFound => write!(f, "User not found"),
            DomainError::EmailAlreadyExists => write!(f, "Email already exists"),
            DomainError::InvalidEmailFormat => write!(f, "Invalid email format"),
            DomainError::InvalidNameFormat => write!(f, "Invalid name format"),
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
            DomainError::InvalidStudentGroupNameFormat => write!(f, "Invalid student group name format"),
            DomainError::StudentGroupAlreadyExists => write!(f, "Student group already exists"),
            DomainError::HomeworkNotFound => write!(f, "Homework not found"),
            DomainError::LessonInstanceNotFound => write!(f, "Lesson instance not found"),
            DomainError::HomeworkFileParentNotFound => write!(f, "Homework file parent homework not found"),
            DomainError::HomeworkFileNotFound => write!(f, "Homework file not found"),
            DomainError::HomeworkAlreadyExists => write!(f, "Homework already exists"),
            DomainError::InvalidHomeworkTextFormat => write!(f, "Invalid homework text format"),
            DomainError::InvalidHomeworkFileFormat => write!(f, "Invalid homework file format"),
            DomainError::InvalidHomeworkFileSize => write!(f, "Invalid homework file size"),
            DomainError::InvalidHomeworkStatus => write!(f, "Invalid homework status"),
            DomainError::LessonNotFound => write!(f, "Lesson not found"),
            DomainError::LessonAlreadyExists => write!(f, "Lesson already exists"),
            DomainError::InvalidLessonReference => write!(f, "Invalid lesson references"),
        }
    }
}

impl std::error::Error for DomainError {}
