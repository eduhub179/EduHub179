-- 0005_create_plusnik.sql

-- ============================================
-- 1. SHEET STATUSES
-- draft     — draft, visible only to the creating teacher
-- published — published, visible to students
-- archived  — archived, hidden from the active list, but history is preserved
-- ============================================
CREATE TYPE sheet_status AS ENUM ('draft', 'published', 'archived');


-- ============================================
-- 2. PROBLEM SHEETS (SPECIAL MATH / PLUSNIK)
-- A sheet is tied to a lesson (lesson_id), not to a teacher.
-- If several teachers teach the lesson, the sheet is shared —
-- all teachers can award pluses for problems from this sheet.
-- ============================================
CREATE TABLE plusnik_sheets
(
    sheet_id   UUID PRIMARY KEY      DEFAULT gen_random_uuid(),

    -- Lesson the sheet belongs to
    lesson_id  UUID         NOT NULL REFERENCES lessons (lesson_id) ON DELETE RESTRICT,

    -- Sheet author (the teacher who created it)
    created_by UUID         NOT NULL REFERENCES users (user_id) ON DELETE RESTRICT,

    -- Sheet title: "Листок 12: Производные" (Sheet 12: Derivatives)
    name       VARCHAR(255) NOT NULL,

    -- Date the sheet was issued to students
    issue_date DATE         NOT NULL,

    -- Submission deadline (optional)
    -- Nothing automatic happens when the deadline passes —
    -- it is just an informational field for display to students and teachers
    deadline   TIMESTAMPTZ NULL,

    -- Sheet status (draft / published / archived)
    status     sheet_status NOT NULL DEFAULT 'draft',

    -- Timestamps
    created_at TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Fast lookup of all sheets of a concrete lesson
CREATE INDEX idx_plusnik_sheets_lesson
    ON plusnik_sheets (lesson_id) WHERE status = 'published';

-- Fast lookup of a teacher's sheets (for editing)
CREATE INDEX idx_plusnik_sheets_created_by
    ON plusnik_sheets (created_by);

-- Fast lookup of a teacher's drafts (to continue work)
CREATE INDEX idx_plusnik_sheets_drafts
    ON plusnik_sheets (created_by, created_at DESC) WHERE status = 'draft';

-- Fast lookup of sheets by issue date (for UI sorting)
CREATE INDEX idx_plusnik_sheets_issue_date
    ON plusnik_sheets (lesson_id, issue_date DESC) WHERE status = 'published';

-- Fast lookup of sheets with a deadline (for reminders)
CREATE INDEX idx_plusnik_sheets_deadline
    ON plusnik_sheets (deadline) WHERE status = 'published' AND deadline IS NOT NULL;


-- ============================================
-- 3. PROBLEMS WITHIN A SHEET
-- Each problem is a separate row. Numbers are short: "1а", "2б*", "3".
-- sort_order defines the display order and allows
-- adding/removing problems anywhere (O(n) for n < 50 = ~1 ms).
-- ============================================
CREATE TABLE plusnik_tasks
(
    task_id     UUID PRIMARY KEY     DEFAULT gen_random_uuid(),

    -- Sheet the problem belongs to
    sheet_id    UUID        NOT NULL REFERENCES plusnik_sheets (sheet_id) ON DELETE CASCADE,

    -- Problem number: "1а", "1б", "2", "3а*", "10"
    task_number VARCHAR(20) NOT NULL,

    -- Display order of problems within the sheet
    -- Updated when problems are added/removed from the middle
    sort_order  INT         NOT NULL,

    -- Timestamps
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Two problems with the same number cannot exist in one sheet
CREATE UNIQUE INDEX idx_plusnik_tasks_unique
    ON plusnik_tasks (sheet_id, task_number);

-- Fast lookup of all problems of a concrete sheet in the correct order
CREATE INDEX idx_plusnik_tasks_sheet_order
    ON plusnik_tasks (sheet_id, sort_order);


-- ============================================
-- 4. PLUSNIK RECORDS (WHO SOLVED WHICH PROBLEM)
-- Each record is one "plus" for a concrete problem.
--
-- Change history is stored directly in the record:
-- - granted_at / granted_by  — who awarded and when
-- - revoked_at / revoked_by  — who revoked and when
-- Revoking does not delete the row, it fills in revoked_at.
-- This settles disputes over "who awarded and when" (§11.4 of the master document).
-- ============================================
CREATE TABLE plusnik_records
(
    record_id      UUID PRIMARY KEY     DEFAULT gen_random_uuid(),

    -- Student who received the plus
    student_id     UUID        NOT NULL REFERENCES users (user_id) ON DELETE RESTRICT,

    -- Sheet the problem belongs to
    sheet_id       UUID        NOT NULL REFERENCES plusnik_sheets (sheet_id) ON DELETE RESTRICT,

    -- Concrete problem (required — a plus is awarded only for a problem)
    task_id        UUID        NOT NULL REFERENCES plusnik_tasks (task_id) ON DELETE RESTRICT,

    -- Teacher who awarded the plus
    granted_by     UUID        NOT NULL REFERENCES users (user_id) ON DELETE RESTRICT,

    -- When the plus was awarded
    granted_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- When the plus was revoked (NULL if active)
    revoked_at     TIMESTAMPTZ NULL,

    -- Who revoked the plus (NULL if active)
    revoked_by     UUID NULL REFERENCES users(user_id) ON DELETE RESTRICT,

    -- Comment on revocation (optional)
    revoke_comment TEXT NULL
);

-- Two active pluses cannot be awarded for the same problem to one student
-- Partial index: only for active (non-revoked) records
CREATE UNIQUE INDEX idx_plusnik_records_active_unique
    ON plusnik_records (student_id, task_id) WHERE revoked_at IS NULL;

-- Fast lookup of all active records of a student (for the student dashboard)
CREATE INDEX idx_plusnik_records_student_active
    ON plusnik_records (student_id, granted_at DESC) WHERE revoked_at IS NULL;

-- Fast lookup of all records of a student including revoked (for history)
CREATE INDEX idx_plusnik_records_student_all
    ON plusnik_records (student_id, granted_at DESC);

-- Fast lookup of all active records of a concrete sheet (for the teacher's matrix)
CREATE INDEX idx_plusnik_records_sheet_active
    ON plusnik_records (sheet_id) WHERE revoked_at IS NULL;

-- Fast lookup of all active records of a concrete problem (for statistics)
CREATE INDEX idx_plusnik_records_task_active
    ON plusnik_records (task_id) WHERE revoked_at IS NULL;

-- Fast lookup of records awarded by a teacher (for action history)
CREATE INDEX idx_plusnik_records_granted_by
    ON plusnik_records (granted_by, granted_at DESC);

-- If a plus is revoked, the revoker must be specified
ALTER TABLE plusnik_records
    ADD CONSTRAINT chk_revoked_has_reviewer CHECK (
        revoked_at IS NULL OR revoked_by IS NOT NULL
        );

-- A plus cannot be revoked in the future
ALTER TABLE plusnik_records
    ADD CONSTRAINT chk_revoke_not_future CHECK (
        revoked_at IS NULL OR revoked_at <= NOW()
        );


-- ============================================
-- 5. TRIGGER: CHECK THAT A PROBLEM BELONGS TO THE SHEET
-- Guarantees that task_id belongs to the same sheet as sheet_id.
-- Cannot be expressed with a regular FOREIGN KEY, so we use a trigger.
-- ============================================
CREATE
OR REPLACE FUNCTION check_task_belongs_to_sheet()
RETURNS TRIGGER AS $$
BEGIN
    IF
NOT EXISTS (
        SELECT 1 FROM plusnik_tasks
        WHERE task_id = NEW.task_id AND sheet_id = NEW.sheet_id
    ) THEN
        RAISE EXCEPTION 'task_id % does not belong to sheet_id %',
            NEW.task_id, NEW.sheet_id;
END IF;
RETURN NEW;
END;
$$
LANGUAGE plpgsql;

CREATE TRIGGER trigger_check_task_belongs_to_sheet
    BEFORE INSERT OR
UPDATE ON plusnik_records
    FOR EACH ROW
    EXECUTE FUNCTION check_task_belongs_to_sheet();


-- ============================================
-- 6. TRIGGERS FOR UPDATING updated_at
-- ============================================
CREATE TRIGGER trigger_plusnik_sheets_updated_at
    BEFORE UPDATE
    ON plusnik_sheets
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
