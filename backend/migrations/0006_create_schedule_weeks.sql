-- 0006_create_schedule_weeks.sql
--
-- docs/SCHEDULE.en.md + docs/OVERRIDES.en.md land here:
--   1. schedule_weeks — the week container (rows = weeks, cells = lesson_instances),
--      draft/published visibility gate.
--   2. lesson_instances.cabinet_id — cabinets move to instances (weekly room shuffle,
--      truthful free-cabinet queries).
--   3. FK: instances -> weeks (a week must exist before its instances).
--   4. check_teacher_available becomes week-aware (instance-first, template fallback).
--   5. get_student_schedule_for_date: published-week gate, nothing auto-shadows
--      (events and lessons shown together), status column for greyed cancelled rows.


-- ============================================
-- 1. WEEKS (schedule container)
-- ============================================
CREATE TABLE schedule_weeks
(
    week_start_date DATE PRIMARY KEY,

    -- Lifecycle: admin builds a week as draft, publishes when final.
    -- Students see instances only in PUBLISHED weeks (availability checks see all).
    status          VARCHAR(20) NOT NULL DEFAULT 'draft'
        CHECK (status IN ('draft', 'published')),

    -- Provenance: which week this one was copied from
    -- (NULL = generated from templates / manual).
    copied_from     DATE NULL REFERENCES schedule_weeks (week_start_date),

    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TRIGGER trigger_schedule_weeks_updated_at
    BEFORE UPDATE
    ON schedule_weeks
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();


-- ============================================
-- 2. CABINETS MOVE TO INSTANCES
-- ============================================
ALTER TABLE lesson_instances
    ADD COLUMN cabinet_id UUID NULL REFERENCES cabinets (cabinet_id) ON DELETE SET NULL;

-- Fast lookup for free-cabinet queries (future exclusion index is parked).
CREATE INDEX idx_lesson_instances_cabinet
    ON lesson_instances (cabinet_id)
    WHERE cabinet_id IS NOT NULL;


-- ============================================
-- 3. WEEKS BEFORE INSTANCES (FK)
-- Backfill: every week that already has instances becomes 'published',
-- so nothing disappears when the visibility gate lands.
-- ============================================
INSERT INTO schedule_weeks (week_start_date, status)
SELECT DISTINCT week_start_date, 'published'
FROM lesson_instances
ON CONFLICT (week_start_date) DO NOTHING;

ALTER TABLE lesson_instances
    ADD CONSTRAINT lesson_instances_week_start_date_fkey
        FOREIGN KEY (week_start_date) REFERENCES schedule_weeks (week_start_date);


-- ============================================
-- 4. WEEK-AWARE AVAILABILITY CHECK
-- Instance-first: what actually happens that week wins (status = 'scheduled').
-- Fallback: weeks without generated instances use active templates (clean now —
-- override templates no longer pollute the template space).
-- p_exclude_instance_id: reserved for the override flow (docs/OVERRIDES.en.md §5).
-- NOTE: the old 5-arg overload (no week param) is dropped explicitly — a new
-- arity would otherwise linger as a second overload.
-- ============================================
DROP FUNCTION IF EXISTS check_teacher_available(uuid, day_of_week, time, time, uuid);

CREATE
OR REPLACE FUNCTION check_teacher_available(
    p_teacher_id UUID,
    p_week_start_date DATE,
    p_day day_of_week,
    p_start_time TIME,
    p_end_time TIME,
    p_exclude_instance_id UUID DEFAULT NULL
) RETURNS BOOLEAN AS $$
DECLARE
    v_week_has_instances BOOLEAN;
BEGIN
    SELECT EXISTS (SELECT 1 FROM lesson_instances WHERE week_start_date = p_week_start_date)
    INTO v_week_has_instances;

    IF v_week_has_instances THEN
        RETURN NOT EXISTS (SELECT 1
                           FROM lesson_instances li
                                    JOIN lesson_templates lt ON li.template_id = lt.template_id
                                    JOIN lessons l ON lt.lesson_id = l.lesson_id
                                    JOIN lesson_teachers lte ON lte.lesson_id = l.lesson_id
                           WHERE lte.teacher_id = p_teacher_id
                             AND li.week_start_date = p_week_start_date
                             AND li.status = 'scheduled'
                             AND lt.day = p_day
                             AND lt.start_time < p_end_time
                             AND lt.end_time > p_start_time
                             AND li.instance_id != COALESCE(p_exclude_instance_id, '00000000-0000-0000-0000-000000000000'::UUID));
    ELSE
        RETURN NOT EXISTS (SELECT 1
                           FROM lesson_templates lt
                                    JOIN lessons l ON lt.lesson_id = l.lesson_id
                                    JOIN lesson_teachers lte ON lte.lesson_id = l.lesson_id
                           WHERE lte.teacher_id = p_teacher_id
                             AND lt.is_active = TRUE
                             AND lt.day = p_day
                             AND lt.start_time < p_end_time
                             AND lt.end_time > p_start_time);
    END IF;
END;
$$
LANGUAGE plpgsql;


-- ============================================
-- 5. STUDENT SCHEDULE — PUBLISHED GATE, NO AUTO-SHADOWING, STATUS COLUMN
-- Events and lessons are shown together; overlaps are marked client-side
-- (docs/OVERRIDES.en.md §7). Cancelled instances are returned with their status
-- so the client can render them greyed (decided 2026-08-16).
-- NOTE: RETURNS TABLE gains a column vs migration 0003 — CREATE OR REPLACE
-- cannot change the return type (PG 42P13), so the old function is dropped first.
-- ============================================
DROP FUNCTION IF EXISTS get_student_schedule_for_date(uuid, date);

CREATE
OR REPLACE FUNCTION get_student_schedule_for_date(
    p_student_id UUID,
    p_date DATE
) RETURNS TABLE (
    start_time TIME,
    end_time TIME,
    title VARCHAR(255),
    is_event BOOLEAN,
    status VARCHAR(20),
    cabinet_id UUID
) AS $$
BEGIN
RETURN QUERY
-- Events the student attends (no status — not lessons)
SELECT e.start_time::TIME, e.end_time::TIME, e.title,
       TRUE AS is_event,
       NULL::VARCHAR(20) AS status,
       e.cabinet_id
FROM events e
         JOIN event_attendees ea ON ea.event_id = e.event_id
WHERE ea.student_id = p_student_id
  AND e.start_time::DATE = p_date

UNION ALL

-- Lessons from PUBLISHED weeks; cancelled rows included (greyed client-side).
-- Effective cabinet: instance wins, template as fallback (old instances
-- created before the cabinet move have NULL and fall back to the template room).
SELECT lt.start_time,
       lt.end_time,
       s.name AS title,
       FALSE  AS is_event,
       li.status::VARCHAR(20) AS status,
       COALESCE(li.cabinet_id, lt.cabinet_id) AS cabinet_id
FROM lesson_instances li
         JOIN lesson_templates lt ON li.template_id = lt.template_id
         JOIN lessons l ON lt.lesson_id = l.lesson_id
         JOIN subjects s ON l.subject_id = s.subject_id
         JOIN schedule_weeks sw ON sw.week_start_date = li.week_start_date
WHERE li.lesson_date = p_date
  AND sw.status = 'published'
  AND (
        -- Lesson for the student's class
        l.class_id = (SELECT class_id FROM users WHERE user_id = p_student_id)
            OR
            -- Lesson for a group the student belongs to
        l.group_id IN (SELECT gm.group_id FROM group_members gm WHERE gm.student_id = p_student_id)
    );
END;
$$
LANGUAGE plpgsql;
