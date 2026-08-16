# Schedule Building (Weeks) — Design

> Status: **design in progress, not implemented.** Decision log from 2026-08-16 (Max + Alan).
> The `schedule_weeks` table does not exist yet. Discussion continues (copy-previous-week flow
> is a separate flagged topic).

## 1. Motivation

Instances are generated **lazily, week by week** — the schedule is unstable until the first
term ends. The **week is the unit of schedule building**: the admin "builds a week", and
(once the copy flow lands) "copies last week into this one", then publishes. A week needs to
exist as a first-class entity so the workflow, the integrity rules, and the visibility rules
have a home.

## 2. Mental model

Rows = weeks, columns = templates, cells = **lesson_instances**. This is a mental model only:

- The **rows** are real: one `schedule_weeks` row per week (`week_start_date` = Monday).
- The **columns** are NOT stored — a template "participates in a week" by having an
  instance for that week. The grid cells ARE the instances
  (`instance = (template_id, week_start_date)`).
- A week "knows" its attached templates/instances through the FK, not through stored lists:
  `SELECT ... FROM lesson_instances WHERE week_start_date = $1`.

## 3. The table

```sql
CREATE TABLE schedule_weeks
(
    week_start_date DATE PRIMARY KEY,

    -- Lifecycle: admin builds a week as draft, publishes when final
    status          VARCHAR(20) NOT NULL DEFAULT 'draft'
        CHECK (status IN ('draft', 'published')),

    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

- `lesson_instances.week_start_date` gains an FK → `schedule_weeks.week_start_date`.
  Admin workflow: **create the week → fill it with instances**. No more instances floating
  in weeks that do not exist.
- Purely additive — `lesson_instances` itself stays exactly as it is. Homework/files keep
  pointing at instances as a stable single pointer (instead of a composite `(week, template)`).

## 4. Visibility rules

- **Students see instances only in PUBLISHED weeks.** Draft weeks are admin-only.
- **The availability check sees ALL instances, drafts included** — schedule building must
  prevent conflicts before a week goes live.
- Migration backfill: every week that already has instances gets `status = 'published'`
  so nothing disappears when the gate lands.

## 5. Open / flagged

- **Copy-previous-week flow** (`copied_from`, bulk instance copy): separate discussion,
  Max has ideas. The `schedule_weeks` table is its natural anchor.
- **Editing a published week**: re-draft it, or live-edit? Decide with the copy flow.
- **Term / school-year grouping**: later.
- **Query changes** (when implemented): `get_student_schedule_for_date` gains a JOIN to
  `schedule_weeks` + `status = 'published'` filter, and returns instances including
  `cancelled` ones — greyed rows are a client-side decision (see OVERRIDES.en.md §9).
