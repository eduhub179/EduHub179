-- ============================================================================
-- FILE: 0012_create_lesson_substitutions.sql
-- PURPOSE: Substitute teacher and a note for one concrete lesson (instance),
--          set by an admin. Cancellation and room change stay on lesson_instances
--          (status, cabinet_id); this table holds only what lesson_instances can't.
-- DEPENDENCIES: [0001_create_users.sql, 0003_create_schedule.sql]
-- ============================================================================

CREATE TABLE lesson_substitutions
(
    -- One row per lesson instance; deleted together with the instance
    instance_id           UUID PRIMARY KEY REFERENCES lesson_instances (instance_id) ON DELETE CASCADE,
    -- Teacher who gives the lesson instead of the usual ones (NULL = teachers unchanged)
    substitute_teacher_id UUID NULL REFERENCES users (user_id) ON DELETE RESTRICT,
    -- Shown to students: "урок перенесён в актовый зал", "контрольная отменяется"
    note                  TEXT NULL CHECK (note IS NULL OR length(trim(note)) BETWEEN 1 AND 500),
    -- Admin who made the last change (audit)
    updated_by            UUID NOT NULL REFERENCES users (user_id) ON DELETE RESTRICT,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_substitution_not_empty CHECK (substitute_teacher_id IS NOT NULL OR note IS NOT NULL)
);

-- Teacher's schedule: lessons where the teacher substitutes
CREATE INDEX idx_lesson_substitutions_teacher
    ON lesson_substitutions (substitute_teacher_id)
    WHERE substitute_teacher_id IS NOT NULL;

CREATE TRIGGER trigger_lesson_substitutions_updated_at
    BEFORE UPDATE ON lesson_substitutions
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
