-- 0002_create_subjects_and_classes.sql

-- ============================================
-- 1. CLASS NAMES (LETTERS)
-- ============================================
CREATE TYPE class_letter AS ENUM ('б', 'в', 'и'); --TODO: real names


-- ============================================
-- 2. SCHOOL CLASSES
-- The class number is computed analytically from graduation_year and the current date.
-- ============================================
CREATE TABLE classes
(
    class_id        UUID PRIMARY KEY      DEFAULT gen_random_uuid(),

    graduation_year INT          NOT NULL,
    letter          class_letter NOT NULL,

    -- Whether the class is active (graduates become inactive)
    is_active       BOOLEAN      NOT NULL DEFAULT TRUE,

    -- Timestamps
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Two classes with the same graduation year and letter cannot be created
CREATE UNIQUE INDEX idx_classes_year_letter
    ON classes (graduation_year, letter);

-- Fast lookup of active classes (for the admin panel)
CREATE INDEX idx_classes_active ON classes (is_active) WHERE is_active = TRUE;

-- Fast lookup by graduation year
CREATE INDEX idx_classes_graduation_year ON classes (graduation_year) WHERE is_active = TRUE;


-- ============================================
-- 3. SUBJECTS
-- ============================================
CREATE TABLE subjects
(
    subject_id UUID PRIMARY KEY     DEFAULT gen_random_uuid(),

    -- Subject name: "Алгебра" (Algebra), "Спецмат" (Special Math), "Информатика" (Informatics)
    name       VARCHAR(100) NOT NULL,

    -- Timestamps
    created_at TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Two subjects with the same name cannot be created
CREATE UNIQUE INDEX idx_subjects_name ON subjects (name);


-- ============================================
-- 4. STUDENT GROUPS
-- A group is an arbitrary subset of school students.
-- A group is NOT tied to a single class: it can unite students
-- from different classes (e.g., "Английский B1" (English B1) — students from 10а, 10б, 10в).
-- ============================================
CREATE TABLE student_groups
(
    group_id   UUID PRIMARY KEY     DEFAULT gen_random_uuid(),

    -- Unique group name: "Английский B1" (English B1), "Информатика базовая" (Basic Informatics)
    name       VARCHAR(100) NOT NULL,

    -- Timestamps
    created_at TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Two groups with the same name cannot be created
CREATE UNIQUE INDEX idx_student_groups_name ON student_groups (name);


-- ============================================
-- 5. GROUP MEMBERSHIP
-- "student ↔ group" relationship. A student can be in several groups
-- at the same time (e.g.
-- ============================================
CREATE TABLE group_members
(
    member_id  UUID PRIMARY KEY     DEFAULT gen_random_uuid(),

    -- Student and group
    student_id UUID         NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    group_id   UUID         NOT NULL REFERENCES student_groups(group_id) ON DELETE CASCADE,

    -- Timestamps
    created_at TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- A student cannot be in the same group twice
CREATE UNIQUE INDEX idx_group_members_unique
    ON group_members (student_id, group_id);

-- Fast lookup of all groups a student belongs to (for the schedule)
CREATE INDEX idx_group_members_student
    ON group_members (student_id);

-- Fast lookup of all students in a group (for the teacher when awarding pluses)
CREATE INDEX idx_group_members_group
    ON group_members (group_id);


-- ============================================
-- 6. LESSONS (class/group + subject)
-- A lesson is a pairing of (class OR group) + subject.
-- Teachers who teach the lesson are stored in a separate lesson_teachers table.
-- This allows multiple teachers to teach one lesson, while homework, pluses,
-- and schedule entries are tied to the lesson, not to a specific teacher.
-- ============================================
CREATE TABLE lessons
(
    lesson_id  UUID PRIMARY KEY     DEFAULT gen_random_uuid(),

    -- Exactly one of the two: class OR group
    -- If class_id is set — lesson for the whole class
    -- If group_id is set — lesson for a group of students (may span different classes)
    class_id   UUID         NULL REFERENCES classes(class_id) ON DELETE RESTRICT,
    group_id   UUID         NULL REFERENCES student_groups(group_id) ON DELETE RESTRICT,

    -- Subject taught in this lesson
    subject_id UUID         NOT NULL REFERENCES subjects(subject_id) ON DELETE RESTRICT,

    -- Whether the lesson is active (can be deactivated without deleting homework/plus history)
    is_active  BOOLEAN      NOT NULL DEFAULT TRUE,

    -- Timestamps
    created_at TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ  NOT NULL DEFAULT NOW(),

    -- Exactly one of class_id / group_id is set
    CONSTRAINT chk_one_entity CHECK (
        (class_id IS NOT NULL AND group_id IS NULL)
            OR
        (class_id IS NULL AND group_id IS NOT NULL)
        )
);

-- Partial unique index for class lessons
-- Guarantees that a class cannot have two lessons of the same subject
CREATE UNIQUE INDEX idx_lessons_class_unique
    ON lessons (class_id, subject_id)
    WHERE class_id IS NOT NULL;

-- Partial unique index for group lessons
-- Guarantees that a group cannot have two lessons of the same subject
CREATE UNIQUE INDEX idx_lessons_group_unique
    ON lessons (group_id, subject_id)
    WHERE group_id IS NOT NULL;

-- Fast lookup of all lessons of a specific class
CREATE INDEX idx_lessons_class
    ON lessons (class_id) WHERE is_active = TRUE AND class_id IS NOT NULL;

-- Fast lookup of all lessons of a specific group
CREATE INDEX idx_lessons_group
    ON lessons (group_id) WHERE is_active = TRUE AND group_id IS NOT NULL;


-- ============================================
-- 7. LESSON TEACHERS
-- Many-to-many relationship between lessons and teachers.
-- One lesson can be taught by several teachers (e.g., Special Math in 10б is taught by 4 teachers).
-- One teacher can teach several lessons (e.g., Ivanov teaches Special Math in 10б and 11б).
-- ============================================
CREATE TABLE lesson_teachers
(
    -- Reference to the lesson
    lesson_id  UUID NOT NULL REFERENCES lessons(lesson_id) ON DELETE CASCADE,

    -- Reference to the teacher
    teacher_id UUID NOT NULL REFERENCES users(user_id) ON DELETE RESTRICT,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Composite primary key: a teacher cannot be linked to the same lesson twice
    PRIMARY KEY (lesson_id, teacher_id)
);

-- Fast lookup of all lessons of a specific teacher
-- Used when a teacher opens their list of subjects/classes
CREATE INDEX idx_lesson_teachers_teacher
    ON lesson_teachers (teacher_id);


-- ============================================
-- 8. STUDENT-CLASS ASSIGNMENT
-- A student belongs to one class (their main class).
-- Additionally, a student can be a member of groups (group_members table).
-- ============================================
ALTER TABLE users
    ADD COLUMN class_id UUID NULL REFERENCES classes(class_id) ON DELETE SET NULL;

-- Fast lookup of students in a specific class
-- Used when a teacher searches for a student in their class to award pluses
CREATE INDEX idx_users_class
    ON users (class_id)
    WHERE role = 'student' AND is_active = TRUE;

-- Fast lookup of all school students by last name
-- Used when an admin searches for a student by last name across the school
CREATE INDEX idx_users_students
    ON users (last_name, first_name)
    WHERE role = 'student' AND is_active = TRUE;


-- ============================================
-- 9. TRIGGERS FOR UPDATING updated_at
-- ============================================
CREATE TRIGGER trigger_classes_updated_at
    BEFORE UPDATE ON classes
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER trigger_student_groups_updated_at
    BEFORE UPDATE ON student_groups
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER trigger_lessons_updated_at
    BEFORE UPDATE ON lessons
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
