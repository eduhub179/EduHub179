# Schedule Building (Weeks) — Design

> Status: **partially implemented.** Decision log from 2026-08-16 (Max + Alan).
> Migration 0006 (`feat/schedule-layer`) already delivered: `schedule_weeks`, the
> instance→week FK, the published-week gate, and the week-aware availability check.
> Still not implemented: the grid (week template, §4), the building operations (§6), and
> the published-week editing rule (§7). Copy-previous-week flow is a separate flagged topic.

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

## 4. The grid (week template) — the slot skeleton

`UNIQUE(template_id, week_start_date)` bounds instances per (template, week), but templates
themselves are unlimited — nothing stops N templates for the same class at the same day/time
(the dedup index is per-lesson, not per-class). The grid closes this hole structurally.

One global grid per school (breaks are school-wide and change only unofficially); a `grid_id`
can be added later without a breaking migration.

```sql
CREATE TABLE schedule_grid_slots
(
    slot_id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    day          day_of_week NOT NULL,
    slot_number  INT NOT NULL,          -- 1..N, order within the day
    start_time   TIME NOT NULL,
    end_time     TIME NOT NULL,
    is_break     BOOLEAN NOT NULL DEFAULT FALSE,
    CONSTRAINT chk_grid_slot_time CHECK (end_time > start_time),
    CONSTRAINT chk_grid_slot_unique UNIQUE (day, slot_number)
);
```

- `lesson_templates.slot_id UUID NULL REFERENCES schedule_grid_slots (slot_id)`:
  - **Class templates MUST reference a slot** (domain rule). `(day, start_time, end_time)`
    stay populated and must equal the slot's values — so the dedup index, the availability
    check and the schedule queries keep working unchanged.
  - **Group/club templates keep `slot_id = NULL`** — free-form times; clubs rarely fit the
    bell schedule, and overlaps with the grid are legal ("student decides", OVERRIDES §7).
- Break slots (`is_break = TRUE`) cannot host templates (domain rule).
- Same-class double-booking is prevented by a constraint trigger
  `chk_no_class_double_booking` (a unique index cannot express it — `class_id` lives on
  `lessons`): no two templates whose lessons target the same class may share a
  `(slot_id, parity)`.
- Consequence: instances per (class, week) are bounded by the number of non-break slots —
  unlimited instances per week become impossible for the regular schedule.

**Decision (2026-08-17): templates stay ONE table.** The class/group kind already lives on
`lessons` (class XOR group), and templates inherit it through their FK. Splitting templates
would force XOR columns on `lesson_instances` and duplicate the availability/schedule
machinery for no gain. If type-level separation is ever wanted, it lives in the domain (two
entity types over one table), not in the schema.

`week_parity` stays parked; nothing here blocks odd/even twin templates (the dedup index
already handles them).

## 5. Visibility rules

- **Students see instances only in PUBLISHED weeks.** Draft weeks are admin-only.
- **The availability check sees ALL instances, drafts included** — schedule building must
  prevent conflicts before a week goes live.
- Migration backfill: every week that already has instances gets `status = 'published'`
  so nothing disappears when the gate lands (done in 0006).

## 6. Building a week — operations

Both operations produce a `draft` week; the admin then adjusts (overrides, cancellations,
cabinet changes) and publishes.

### 6.1 Generate from templates

For every active template, create an instance for the target week
(`lesson_date = week_start_date + day`); day/time come from the template (which for class
templates equals the grid slot's values, §4). Baseline for the start of the year / unstable
period.

### 6.2 Copy a week

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

## 7. Open / flagged

- **Editing a published week — DECIDED (2026-08-16, hybrid):** re-draft is allowed only for
  weeks that have not started yet (all lessons still in the future); once the week has begun
  it is live-edit only — changes appear immediately, no hiding. Pure application-layer rule;
  `status` column already covers it.
- **Term / school-year grouping**: later.
- **`template.cabinet_id` role shrinks to a week-1 seed** (everything after comes from copy
  + adjustments) — ties into the deferred cabinet-column decision.
- **Query changes — DONE in 0006**: `get_student_schedule_for_date` joins `schedule_weeks`
  with a `status = 'published'` filter and returns instances including `cancelled` ones —
  greyed rows are a client-side decision (see OVERRIDES.en.md §9).
- **Parity, if ever un-parked**: `generate_from_templates` and the availability check would
  need to respect `week_parity` (odd/even weeks). The dedup index already supports odd/even
  twin templates. No feature today — column parked.
