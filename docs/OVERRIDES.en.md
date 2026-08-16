# Lesson Overrides — Design

> Status: **design, not implemented.** Decision log from 2026-08-16 (Max + Alan).
> The `lesson_overrides` table does not exist yet — this doc is the spec the implementation PR will follow.
> When the migration lands, the substitution recipe in `DATABASE_ARCHITECTURE.en.md` §6.5 will be updated to match.

## 1. Why this exists

The old substitution flow (new lesson + new template with `is_override = TRUE` + repoint the
instance) had three structural problems:

1. **Week-blind availability.** `check_teacher_available(teacher, day, start, end, exclude_template_id)`
   scans *all active templates* and has no date/week parameter. A one-week substitution template
   blocked the substitute teacher at that day/time for **every** week until manually archived.
2. **No override→original link.** Nothing structurally says "this lesson replaced that one".
   "Replaced by" could not be shown; archiving was a manual ritual.
3. **Heavyweight ritual.** A one-week swap required cloning the lesson (the dedup index
   `(lesson_id, day, start_time, end_time, parity)` blocks a second template for the same lesson
   at the same slot), creating a template, and repointing the instance.

## 2. The model

Three layers stay unchanged:

| Layer | Changes | Example |
|---|---|---|
| `lessons` | extremely rarely | class/group + subject + teachers (via `lesson_teachers`) |
| `lesson_templates` | sometimes (school-wide schedule change) | day + time rhythm, `is_active` |
| `lesson_instances` | each time | concrete lesson on a concrete date, `status` |

**An override is a side-channel attached to one instance:** "this occurrence runs different
content". It swaps *what is taught and by whom*, never *when/where*:

- The slot (day/time) always comes from the instance → template chain. An override cannot move
  a lesson. Moves are the schedule's job (template edit for permanent, instance generation for
  one-off).
- The primary real-world case: another teacher comes to the class because the original one is
  absent or busy. Subject change is possible but rare.

## 3. New table

```sql
CREATE TABLE lesson_overrides
(
    override_id UUID PRIMARY KEY     DEFAULT gen_random_uuid(),
    instance_id UUID        NOT NULL REFERENCES lesson_instances (instance_id) ON DELETE CASCADE,
    lesson_id   UUID        NOT NULL REFERENCES lessons (lesson_id) ON DELETE RESTRICT, -- replacement lesson
    comment     TEXT NULL,          -- "Ivanov is sick, Petrov covers"
    created_by  UUID        NOT NULL REFERENCES users (user_id),
    revoked_at  TIMESTAMPTZ NULL,   -- NULL = active
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- One ACTIVE override per instance; unlimited revoked history
CREATE UNIQUE INDEX idx_lesson_overrides_one_active
    ON lesson_overrides (instance_id)
    WHERE revoked_at IS NULL;
```

Key properties:

- **The replacement is a real `lessons` row.** Teachers live only in `lesson_teachers`
  (M2M, not week-scoped), so a teacher swap needs a new lesson: same class/group,
  subject may change or stay, new teacher(s). All existing machinery is reused — no new M2M
  table, no duplicated validation.
- **One lesson can back many overrides.** "Petrov covers ALL of Ivanov's Monday lessons this
  week" = 1 lesson row + N override rows (one per instance).
- **Attendance always resolves from the ORIGINAL chain** (instance → template → original
  lesson's class/group). The override contributes subject + teachers only. Application-level
  rule: replacement lesson must target the same class/group. This prevents the
  class-mismatch trap.
- **No hard delete.** Restore = set `revoked_at`. History doubles as an admin reference
  ("last time Petrov covered for Ivanov") and as the audit trail.
- **No day/time fields on the override.** Slot identity is the instance's business.
- Events are **not** part of this table and the unique index does not consider them — an event
  shadows per-student (`event_attendees`), so an event + an override can legitimately coexist
  (attendees see the event, everyone else sees the overridden lesson).

## 4. Flow

**Substitute for a lesson:**
1. Check availability (week-aware, see §5).
2. Create the replacement lesson if it does not exist yet (same class/group; subject and
   teachers as needed).
3. `INSERT INTO lesson_overrides (instance_id, lesson_id, comment, created_by)`.

**Restore:** `UPDATE lesson_overrides SET revoked_at = now() WHERE override_id = ...`.

No template surgery, no instance repointing, no dedup-index workaround.

## 5. Availability check

```sql
check_teacher_available(
    p_teacher_id UUID,
    p_week_start_date DATE,
    p_day day_of_week,
    p_start_time TIME,
    p_end_time TIME,
    p_exclude_instance_id UUID DEFAULT NULL
)
```

Two layers:

- **Week has instances** (generated): scan `lesson_instances` for that week with
  `status = 'scheduled'`; effective lesson = `COALESCE(active override.lesson_id,
  template.lesson_id)`; teachers via `lesson_teachers`. Overlap on (day, time).
- **Week has no instances yet** (lazy generation): fall back to **active templates**.
  This fallback is clean again *because* overrides never touch templates — the old
  week-blind bug was caused by override templates polluting the template space.

Consequence of lazy generation: an override can only target an instance that already exists
(weeks are generated one at a time).

## 6. Homework

Homework is tied to `lesson_instances`. Rule on override:

- **Same subject** (the common case — teacher substitution): homework stays on the instance;
  the substitute works with it normally.
- **Subject changed:** homework relocates to the **next scheduled instance of the original
  lesson** (original subject + teacher). With lazy generation that target instance may not
  exist yet — create it on demand (it is the real future lesson anyway).

Deferred corner cases: end-of-term orphan (no next occurrence), revocation pull-back of
relocated homework.

## 7. Display precedence (per student)

```
event (student attends)  >  cancelled  >  active override  >  original lesson
```

## 8. Schema changes (future migration, not yet written)

- NEW `lesson_overrides` (+ partial unique index, FKs) — §3.
- `lesson_instances` + `cabinet_id UUID NULL` — cabinets move to instances (weekly
  room shuffle without touching templates; truthful "free cabinet" queries). Template
  keeps its `cabinet_id` for now; whether to drop it is a deferred memory optimization
  (archives do not need cabinet info).
- `lesson_templates`: drop `is_override` (no override templates anymore).
- `check_teacher_available`: new week-aware signature (§5).

## 9. Deferred / flagged for later

- **Cabinet column on templates**: keep both for now, decide later.
- **Events table**: keep. It serves one-off activities (lectures, olympiads, excursions):
  absolute timestamps, per-student attendee list, shadowing. Recurring extracurriculars
  (volleyball, additional informatics) are NOT events — they are templates +
  `student_groups` + `subjects`, with `week_parity` for every-other-week clubs.
  The dedup index already includes parity, so odd/even twin templates coexist.
- **Schedule generation** (copy previous week → next, then apply overrides): separate
  discussion, not designed here.
