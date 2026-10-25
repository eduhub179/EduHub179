-- ============================================================================
-- FILE: 0013_add_lesson_instance_is_extra.sql
-- PURPOSE: Marks lessons an admin added to one date (a one-off lesson or a moved
--          lesson), so the schedule can highlight them like substitutions.
-- DEPENDENCIES: [0003_create_schedule.sql]
-- ============================================================================

ALTER TABLE lesson_instances
    ADD COLUMN is_extra BOOLEAN NOT NULL DEFAULT FALSE;
