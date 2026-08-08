-- 0004_create_homework.sql


-- ============================================
-- 1. HOMEWORK STATUSES
-- draft     — draft, visible only to the creator
-- published — published, visible to all students of the lesson
-- archived  — archived, hidden from the active list
-- ============================================
CREATE TYPE homework_status AS ENUM ('draft', 'published', 'archived');


-- ============================================
-- 2. HOMEWORK
-- Under one lesson (on a concrete date) there is exactly one homework
-- (UNIQUE on lesson_instance_id).
-- Homework can be created by a teacher or a student (the duty officer).
--
-- Editing rules:
-- - If a teacher created the homework → only the teacher can edit it
-- - If a student created it and the teacher has not interacted → any student can edit
-- - If a teacher has interacted with the homework (locked_by_teacher = true) → only the teacher can edit
--
-- Anonymity:
-- - Which student edited — visible only to the admin
-- - If a teacher has interacted — visible to everyone
--
-- Content validation (at least text or a file) is done at the application level (use case).
-- A DB-level trigger is not used to avoid problems with file uploads.
-- ============================================
CREATE TABLE homeworks
(
    homework_id        UUID PRIMARY KEY         DEFAULT gen_random_uuid(),

    -- Concrete lesson on a concrete date (exactly one homework per lesson)
    lesson_instance_id UUID            NOT NULL REFERENCES lesson_instances (instance_id) ON DELETE RESTRICT,

    -- Who created the homework (visible to everyone — this is the author)
    created_by         UUID            NOT NULL REFERENCES users (user_id) ON DELETE RESTRICT,

    -- Creator's role (for fast permission checks without JOIN)
    -- 'teacher' — created by a teacher
    -- 'student' — created by a student
    created_by_role    user_role       NOT NULL,

    -- Text content of the homework (optional — may be file-only)
    text_content       TEXT NULL,

    -- Homework status
    status             homework_status NOT NULL DEFAULT 'draft',

    -- Teacher lock flag (one-way switch)
    -- If true → students can no longer edit
    -- If false → students can edit
    locked_by_teacher  BOOLEAN         NOT NULL DEFAULT FALSE,

    -- Who last edited (for audit; if a student, visible only to the admin)
    -- NULL on creation. The author is not counted as an editor
    last_edited_by     UUID NULL REFERENCES users(user_id) ON DELETE RESTRICT,

    -- Timestamps
    created_at         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    updated_at         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

-- Exactly one homework per concrete lesson
CREATE UNIQUE INDEX idx_homeworks_instance_unique
    ON homeworks (lesson_instance_id);

-- Fast lookup of homework of a concrete lesson
CREATE INDEX idx_homeworks_instance
    ON homeworks (lesson_instance_id) WHERE status = 'published';

-- Fast lookup of homework a teacher has interacted with
CREATE INDEX idx_homeworks_teacher_interacted
    ON homeworks (lesson_instance_id) WHERE status = 'published' AND locked_by_teacher = TRUE;


-- ============================================
-- 3. HOMEWORK FILES
-- One homework can have several files (PDF, photos of the assignment, scans).
-- Only metadata and a link to S3/MinIO are stored in the DB;
-- the files themselves live in object storage (§5 of the master document).
-- ============================================
CREATE TABLE homework_files
(
    file_id     UUID PRIMARY KEY      DEFAULT gen_random_uuid(),

    -- Homework the file belongs to
    homework_id UUID         NOT NULL REFERENCES homeworks (homework_id) ON DELETE CASCADE,

    -- File path/key in S3 storage (e.g. "homeworks/2026/07/abc123.pdf")
    storage_key VARCHAR(500) NOT NULL,

    -- Original file name for display to the user
    file_name   VARCHAR(255) NOT NULL,

    -- MIME type (application/pdf, image/jpeg, etc.)
    mime_type   VARCHAR(100) NOT NULL,

    -- File size in bytes (for display and limits)
    size_bytes  BIGINT       NOT NULL CHECK (size_bytes >= 0),

    -- Display order of files (if there are several)
    sort_order  INT          NOT NULL DEFAULT 0,

    -- Timestamps
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Fast lookup of all files of a concrete homework
-- Used when displaying homework to a student
CREATE INDEX idx_homework_files_homework
    ON homework_files (homework_id, sort_order);


-- ============================================
-- 4. TRIGGERS FOR UPDATING updated_at
-- ============================================
CREATE TRIGGER trigger_homeworks_updated_at
    BEFORE UPDATE
    ON homeworks
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
