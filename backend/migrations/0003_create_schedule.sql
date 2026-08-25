-- 0003_create_schedule.sql


-- ============================================
-- 1. DAYS OF THE WEEK
-- Monday — Saturday. Sunday has no lessons — only events.
-- ============================================
CREATE TYPE day_of_week AS ENUM ('mon', 'tue', 'wed', 'thu', 'fri', 'sat');


-- ============================================
-- 2. LESSON PERIODICITY
-- every — every week
-- odd   — only on odd weeks
-- even  — only on even weeks
-- ============================================
CREATE TYPE week_parity AS ENUM ('every', 'odd', 'even');


-- ============================================
-- 3. SCHEDULE WEEKS (the schedule container)
-- One row per week; the week is the unit of schedule building:
-- the admin creates the week (draft), fills it with lesson_instances,
-- then publishes it. Students see instances only in PUBLISHED weeks.
-- ============================================
-- Week lifecycle (a real PG enum, like homework_status in 0004).
CREATE TYPE week_status AS ENUM ('draft', 'published');

CREATE TABLE schedule_weeks
(
    week_start_date DATE PRIMARY KEY,

    -- draft — admin is still building the week, invisible to students
    -- published — the week is final, students can see it
    status          week_status NOT NULL DEFAULT 'draft',

    -- Which week this one was copied from (NULL = generated from templates / manual)
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
-- 4. CABINETS
-- A cabinet is a 3-digit number (floor + room number on the floor).
-- Stored as a separate entity to protect against typos
-- and to allow adding metadata (equipment, capacity).
-- ============================================
CREATE TABLE cabinets
(
    cabinet_id  UUID PRIMARY KEY     DEFAULT gen_random_uuid(),

    -- Cabinet number (3-digit number)
    number      INT         NOT NULL UNIQUE CHECK (number BETWEEN 100 AND 999),

    -- Floor (computed from number)
    floor       INT         NOT NULL GENERATED ALWAYS AS (number / 100) STORED,

    -- Description (optional): "Химическая лаборатория" (Chemistry lab), "Компьютерный класс" (Computer room)
    description VARCHAR(255) NULL,

    -- Capacity (optional)
    capacity    INT NULL CHECK (capacity > 0),

    -- Timestamps
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Fast lookup of cabinets on a specific floor
CREATE INDEX idx_cabinets_floor ON cabinets (floor);


-- ============================================
-- 5. LESSON TEMPLATES
-- A lesson template is a "subject-time-cabinet" combination that is always valid.
-- Created once, used in the schedule.
-- Changing a template automatically updates all lessons generated from it.
--
-- Flags:
-- - is_active: TRUE — used in the current schedule, participates in availability checks
--              FALSE — archived, does not participate in checks
-- ============================================
CREATE TABLE lesson_templates
(
    template_id UUID PRIMARY KEY     DEFAULT gen_random_uuid(),

    -- Lesson (class/group + subject + teachers)
    lesson_id   UUID        NOT NULL REFERENCES lessons (lesson_id) ON DELETE RESTRICT,

    -- When the lesson takes place
    day         day_of_week NOT NULL,
    start_time  TIME        NOT NULL,
    end_time    TIME        NOT NULL,

    -- Periodicity (every week / even / odd)
    parity      week_parity NOT NULL DEFAULT 'every',

    -- Where the lesson takes place
    cabinet_id  UUID NULL REFERENCES cabinets(cabinet_id) ON DELETE SET NULL,

    -- Whether the template is active (used in the current schedule)
    is_active   BOOLEAN     NOT NULL DEFAULT TRUE,

    -- Always FALSE — substitutions are handled at the instance level
    -- (lesson_instances.status = 'cancelled' and overrides), not by templates.
    is_override BOOLEAN     NOT NULL DEFAULT FALSE,

    -- Free-form comment
    comment     TEXT NULL,

    -- Timestamps
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- End must be after start
    CONSTRAINT chk_template_time CHECK (end_time > start_time)
);

-- One lesson cannot have two templates with the same parameters
CREATE UNIQUE INDEX idx_lesson_templates_no_dup
    ON lesson_templates (lesson_id, day, start_time, end_time, parity);

-- Fast lookup of active templates of a specific lesson
-- Used when checking teacher availability
CREATE INDEX idx_lesson_templates_lesson_active
    ON lesson_templates (lesson_id, day, start_time, end_time) WHERE is_active = TRUE;

-- Fast lookup of active templates on a specific day
-- Used when building the schedule
CREATE INDEX idx_lesson_templates_day_active
    ON lesson_templates (day, start_time, end_time) WHERE is_active = TRUE;

-- Fast lookup of all templates of a specific lesson (including archived)
CREATE INDEX idx_lesson_templates_lesson
    ON lesson_templates (lesson_id);


-- ============================================
-- 6. CONCRETE LESSONS (ON SPECIFIC DATES)
-- lesson_instance is a concrete lesson on a concrete date.
-- It carries the template it was generated from + the week it belongs to
-- (week_start_date) + the concrete date (lesson_date).
-- Homework is tied to lesson_instance, not to lesson.
-- This allows:
-- - Having different homework for different lessons of the same subject
-- - Archiving homework after the lesson
-- - Preserving history
--
-- NOTE: why instances are the only per-week record: a template has exactly
-- one day of the week, so "the lesson of week W" is fully determined by
-- (template, week_start_date) — one row per template per week, one status
-- to keep in sync.
-- ============================================
-- Lesson status (a real PG enum, like homework_status in 0004).
CREATE TYPE lesson_instance_status AS ENUM ('scheduled', 'completed', 'cancelled');

CREATE TABLE lesson_instances
(
    instance_id UUID PRIMARY KEY     DEFAULT gen_random_uuid(),

    -- Lesson template (regular or a replacement)
    template_id     UUID        NOT NULL REFERENCES lesson_templates (template_id) ON DELETE RESTRICT,

    -- Start of the week this lesson belongs to; the week must exist first
    week_start_date DATE        NOT NULL REFERENCES schedule_weeks (week_start_date),

    -- Lesson date (computed from week_start_date + template day; Monday — Saturday)
    lesson_date DATE        NOT NULL,

    -- Lesson status (scheduled / completed / cancelled)
    status      lesson_instance_status NOT NULL DEFAULT 'scheduled',

    -- Room for THIS week's lesson (overrides the template's room;
    -- NULL = use the template's room). Lets rooms move week to week.
    cabinet_id  UUID NULL REFERENCES cabinets(cabinet_id) ON DELETE SET NULL,

    -- Timestamps
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- A template cannot produce two lessons in the same week
CREATE UNIQUE INDEX idx_lesson_instances_unique
    ON lesson_instances (template_id, week_start_date);

-- Fast lookup of lessons on a specific date
CREATE INDEX idx_lesson_instances_date
    ON lesson_instances (lesson_date);

-- Fast lookup of lessons of a specific week
CREATE INDEX idx_lesson_instances_week
    ON lesson_instances (week_start_date);

-- Fast lookup of lessons of a specific template
CREATE INDEX idx_lesson_instances_template
    ON lesson_instances (template_id);

-- Fast lookup for free-cabinet queries
CREATE INDEX idx_lesson_instances_cabinet
    ON lesson_instances (cabinet_id)
    WHERE cabinet_id IS NOT NULL;


-- ============================================
-- 7. EVENTS (LECTURES, MEETINGS, ELECTIVES)
-- An event is a one-time or recurring activity
-- that is not a lesson but takes up students' time.
-- Unlike groups, events do not change the class structure.
-- Events may take place on any day, including Sunday.
-- ============================================
CREATE TABLE events
(
    event_id     UUID PRIMARY KEY      DEFAULT gen_random_uuid(),

    -- Event title
    title        VARCHAR(255) NOT NULL,

    -- Description (optional)
    description  TEXT NULL,

    -- Start and end time (concrete date + time)
    start_time   TIMESTAMPTZ  NOT NULL,
    end_time     TIMESTAMPTZ  NOT NULL,

    -- Where it takes place (optional)
    cabinet_id   UUID NULL REFERENCES cabinets(cabinet_id) ON DELETE SET NULL,

    -- Who leads/organizes it now (mutable — can be handed over; metadata, not attendance)
    organizer_id UUID         NOT NULL REFERENCES users (user_id) ON DELETE RESTRICT,

    -- Who created it (immutable audit for the archive — set at creation, never updated)
    created_by   UUID         NOT NULL REFERENCES users (user_id) ON DELETE RESTRICT,

    -- End must be after start
    CONSTRAINT chk_event_time CHECK (end_time > start_time),

    -- Timestamps
    created_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Fast lookup of events on a specific date
CREATE INDEX idx_events_date
    ON events (start_time);

-- Fast lookup of events of a specific organizer
CREATE INDEX idx_events_organizer
    ON events (organizer_id);


-- ============================================
-- 8. EVENT ATTENDEES (PARTICIPANTS)
-- user ↔ event relationship. Attendees are PARTICIPANTS — any user,
-- one flat list. Organizer/creator is metadata,
-- NOT an attendee by default: attendance is explicit.
-- ============================================
CREATE TABLE event_attendees
(
    attendee_id UUID PRIMARY KEY     DEFAULT gen_random_uuid(),
    event_id    UUID        NOT NULL REFERENCES events (event_id) ON DELETE CASCADE,
    user_id     UUID        NOT NULL REFERENCES users (user_id) ON DELETE CASCADE,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- A user cannot be in the same event twice
CREATE UNIQUE INDEX idx_event_attendees_unique
    ON event_attendees (event_id, user_id);

-- Fast lookup of all events of a user
-- Used when displaying a user's schedule
CREATE INDEX idx_event_attendees_user
    ON event_attendees (user_id);

-- Fast lookup of all attendees of an event
CREATE INDEX idx_event_attendees_event
    ON event_attendees (event_id);


-- ============================================
-- 9. FUNCTION: TEACHER AVAILABILITY CHECK (WEEK-AWARE)
-- Checks whether a teacher is busy in a given week at the given day/time.
-- Instance-first: if the week already has generated instances, only what is
-- actually scheduled that week counts (status = 'scheduled'; cancelled
-- lessons do not block the teacher). Weeks without instances fall back to
-- checking active templates.
-- p_exclude_instance_id — excludes one instance from the check (used by the
-- override flow when an instance is being replaced).
-- ============================================
CREATE FUNCTION check_teacher_available(
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
-- 10. FUNCTION: STUDENT SCHEDULE FOR A DATE
-- Returns the student's lessons and the events they attend on a concrete date, shown
-- together — overlaps are marked client-side, nothing is hidden.
-- - Lessons come only from PUBLISHED weeks; cancelled instances are
--   returned with their status so the client can render them greyed.
-- - Effective cabinet: instance wins, template is the fallback.
--
-- The status column is the lesson_instance_status enum cast to VARCHAR(20):
-- enum at rest (DB integrity), plain text at the API boundary (clients do
-- not need to know PG types).
-- ============================================
CREATE FUNCTION get_student_schedule_for_date(
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
-- Events the user attends (no lesson status)
SELECT e.start_time::TIME, e.end_time::TIME, e.title,
       TRUE AS is_event,
       NULL::VARCHAR(20) AS status,
       e.cabinet_id
FROM events e
         JOIN event_attendees ea ON ea.event_id = e.event_id
WHERE ea.user_id = p_student_id
  AND e.start_time::DATE = p_date

UNION ALL

-- Lessons from PUBLISHED weeks; cancelled rows included (greyed client-side).
-- Effective cabinet: instance wins, template as fallback.
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


-- ============================================
-- 11. TRIGGERS FOR UPDATING updated_at
-- ============================================
CREATE TRIGGER trigger_lesson_templates_updated_at
    BEFORE UPDATE
    ON lesson_templates
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER trigger_lesson_instances_updated_at
    BEFORE UPDATE
    ON lesson_instances
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER trigger_events_updated_at
    BEFORE UPDATE
    ON events
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
