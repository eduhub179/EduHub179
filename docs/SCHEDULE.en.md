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

    -- Provenance: which week this one was copied from (NULL = generated from templates / manual)
    copied_from     DATE NULL REFERENCES schedule_weeks (week_start_date),

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

## 5. Building a week — operations

Both operations produce a `draft` week; the admin then adjusts (overrides, cancellations,
cabinet changes) and publishes.

### 5.1 Generate from templates

For every active template, create an instance for the target week
(`lesson_date = week_start_date + day`). Baseline for the start of the year / unstable period.

### 5.2 Copy a week

`copy_week(source, target)` — the stable-period workflow:

- Target week row created as `draft` if missing; **error if the target already has instances**
  (copy into empty weeks only, no accidental merges).
- Source: previous week by default, any week selectable.
- For each source instance → new instance: same `template_id`, `week_start_date = target`,
  `lesson_date = source + 7 days`, `status = 'scheduled'` (fresh — cancellations are per-week
  and do NOT carry over), `cabinet_id` copied as a starting point (admin adjusts; the
  free-cabinet check is the guard — no automated validation yet, see OVERRIDES.en.md §9).
- **Overrides are NOT copied** — even a multi-week substitution ("Ivanov out for 3 weeks")
  is re-created per week, because the substitute teacher may vary week to week
  (decision 2026-08-16).
- Safe by construction: cells are 1:1 per (template, week) so `UNIQUE(template_id,
  week_start_date)` guards duplicates; availability is week-scoped, so no cross-week conflicts.
- `copied_from` records provenance. All in one transaction.

## 6. Open / flagged

- **Editing a published week — DECIDED (2026-08-16, hybrid):** re-draft is allowed only for
  weeks that have not started yet (all lessons still in the future); once the week has begun
  it is live-edit only — changes appear immediately, no hiding. Pure application-layer rule;
  `status` column already covers it.
- **Term / school-year grouping**: later.
- **`template.cabinet_id` role shrinks to a week-1 seed** (everything after comes from copy
  + adjustments) — ties into the deferred cabinet-column decision.
- **Query changes** (when implemented): `get_student_schedule_for_date` gains a JOIN to
  `schedule_weeks` + `status = 'published'` filter, and returns instances including
  `cancelled` ones — greyed rows are a client-side decision (see OVERRIDES.en.md §9).
- **Parity, if ever un-parked**: `generate_from_templates` and the availability check would
  need to respect `week_parity` (odd/even weeks). The dedup index already supports odd/even
  twin templates. No feature today — column parked.
