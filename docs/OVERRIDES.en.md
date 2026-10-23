# Lesson Overrides — Implementation

> Status: **implemented** (folded into `lesson_instances` architecture).
> Decision log from 2026-08-16 (Max + Alan); superseded the earlier separate-table design.

## 1. Why overrides exist

A teacher is absent or busy → another teacher covers that one lesson. The slot (day/time)
stays; only the content (subject, teacher, cabinet) changes. Subject swaps between two slots
are also possible but rare.

## 2. The model

Three layers, unchanged:

| Layer | What it is | How often it changes |
|---|---|---|
| `lessons` | class/group + subject + teachers (via `lesson_teachers`) | rarely |
| `lesson_templates` | day + time rhythm + parity, `is_active` | sometimes (school-wide schedule change) |
| `lesson_instances` | concrete lesson on a concrete date, `status` | each time (per week) |

**An override is not a separate table.** It is an instance-level operation:

1. **Cancel** the original instance: `status = 'cancelled'`.
2. **Create** a replacement lesson (same class/group, new teacher/subject if needed).
3. **Create** a replacement template for that lesson (same day/time slot).
4. **Create** a replacement instance for this week, `status = 'scheduled'`.

The original instance stays in the DB (history, audit) and appears in the schedule greyed out
(`get_student_schedule_for_date` returns cancelled rows with their status). The replacement
instance appears as a normal scheduled lesson.

## 3. Why not a separate table

The earlier design proposed a `lesson_overrides` table. It was rejected in favor of the
instance-level approach because:

- **Simpler.** No new table, no new repo, no new aggregate. Reuses existing
  `lessons` + `lesson_templates` + `lesson_instances` machinery.
- **No schema bloat.** The `is_override` column on `lesson_templates` (leftover from the
  old substitution flow) was removed — it was always `FALSE` and served no purpose.
- **History is preserved.** The cancelled original instance is the audit trail.
  "What was here before the override?" → look at the cancelled instance.
- **Teacher availability check already supports it.** `check_teacher_available` has an
  `p_exclude_instance_id` parameter — it skips the cancelled instance when checking if the
  substitute teacher is free.

## 4. Flow

**Substitute a teacher for one lesson:**

1. Check the substitute teacher is available:
   ```sql
   SELECT check_teacher_available(
       'substitute_teacher_id',
       '2026-09-07'::DATE,  -- week_start_date
       'mon'::day_of_week,
       '10:50'::TIME,
       '11:35'::TIME,
       'original_instance_id'::UUID  -- exclude the instance being replaced
   );
   ```
2. Cancel the original instance:
   ```sql
   UPDATE lesson_instances SET status = 'cancelled' WHERE instance_id = 'original_instance_id';
   ```
3. Create the replacement lesson + teachers + template + instance (if the replacement
   lesson doesn't already exist, create it; otherwise reuse it).

**Restore:** set the replacement instance to `cancelled` and the original back to `scheduled`.

### Example — one-off subject swap between two slots

"This week: Algebra instead of Literature on Monday, Literature instead of Algebra on
Thursday."

- Cancel Monday's Literature instance + Thursday's Algebra instance.
- Create replacement instances pointing at the existing Algebra and Literature lessons
  (same class, same teachers — reuse the lesson rows).
- Both slots stay in place; only the content changes.

For a **permanent** arrangement, edit the templates instead — Monday's template points at
Algebra, Thursday's at Literature. This is the "templates change sometimes" layer, not an
override.

## 5. Teacher availability check

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

- **Week has instances** (generated): scans `lesson_instances` for that week with
  `status = 'scheduled'`. Cancelled instances (the original being replaced) don't block.
  `p_exclude_instance_id` also skips the instance being replaced (double safety).
- **Week has no instances yet** (lazy generation): falls back to active templates.
  Clean because overrides never touch templates.

## 6. Homework interaction

Homework is tied to `lesson_instances`.

- **Same subject** (teacher substitution): homework stays on the original instance; the
  substitute works with it normally.
- **Subject changed:** homework relocates to the next scheduled instance of the original
  lesson. If that instance doesn't exist yet (lazy generation), create it on demand.

## 7. Display — nothing auto-shadows

Every activity a student belongs to is shown; overlaps are marked; the student decides.

- Cancelled lessons appear in `get_student_schedule_for_date` with `status = 'cancelled'`
  so the client can render them greyed.
- Replacement instances appear as normal scheduled lessons.
- Events the student attends appear as separate rows. Overlap marking is client-side.
- A lesson replaced by a mandatory event is cancelled via instance status — the view shows
  "cancelled" + the event. No shadowing needed.

Teachers are different: overlaps are **prevented** by the availability check, not displayed
as a choice. Known gap: event organizers are not yet covered by
`check_teacher_available` — a future UI feature, no schema impact.

## 8. What was removed

- `lesson_templates.is_override` column — dropped from migration 0003. It was always `FALSE`
  and served no purpose with the instance-level approach.
- The proposed `lesson_overrides` table — never created, not needed.

## 9. Substitute teacher and note from the admin panel (`lesson_substitutions`, 2026-10-23)

The flow in section 4 doesn't work for the most common case — **same subject, another
teacher**: the replacement lesson can't be created because `lessons` is unique by
`(class_id, subject_id)` / `(group_id, subject_id)` and `lesson_templates` by
`(lesson_id, day, start_time, end_time, parity)`. Also a note for students
("in the assembly hall") has nowhere to live.

So the admin panel (`PATCH /api/admin/lessons/{instance_id}`) does this:

- **Cancel** and **cabinet change** — as before, on the instance itself (`status`, `cabinet_id`).
- **Substitute teacher** and **note** — a row in `lesson_substitutions` (migration 0012,
  one row per instance, deleted with it). An empty substitution (no teacher, no note) is deleted.
- The schedule (`ScheduleService`) shows the substitute in `teachers`, the usual teachers
  in `usual_teachers`, the note in `note`; the lesson also appears in the substitute's schedule.
- Homework stays on the same instance. `copy_week` doesn't copy substitutions.

The section 4 flow remains for **subject changes** (swapping subjects between slots).
Known gap: `check_teacher_available` doesn't consider `lesson_substitutions` yet, and the
admin panel doesn't check that the substitute is free.

## 10. Timetable editor in the admin panel (`TimetableService`, 2026-10-23)

The «Расписание» tab edits the **regular** timetable; «Замены и отмены» edits single
lessons. Code: `application/src/use_cases/admin/timetable_service.rs`, routes
`presentation/src/routes/timetable.rs`.

- **Courses** (`lessons`) and their teachers: `POST /api/admin/courses`,
  `PUT /api/admin/courses/{id}/teachers`. A course teacher changes for every week, past
  ones included; a one-day change is a substitution (section 9).
- **Slots** (`lesson_templates`): `POST /api/admin/slots`, `PUT|DELETE /api/admin/slots/{id}`.
  History is never rewritten: a changed slot becomes a new template and the old one is
  deactivated (`is_active = FALSE`). Already created weeks change only with `apply_from`:
  from that date their lessons move to the new template. A lesson with homework never
  changes its subject (left as is, counted in `kept`); a removed slot's lesson with
  homework becomes `cancelled`, without homework it is deleted.
- **One-off lesson**: `POST /api/admin/lessons` adds a lesson to one date. It gets an
  **inactive** template, so `generate_from_templates` and `copy_week` never carry it to
  other weeks. A cancelled lesson in the same slot is restored instead of duplicated.
- **Moving a lesson**: `POST /api/admin/lessons/{id}/move` cancels the original, adds a
  one-off lesson at the new place and moves the homework with it (the section 4 process,
  done automatically).
- `copy_week` now skips lessons of inactive templates (one-off and removed slots).

Known gaps: no check that the class, teacher or cabinet is free at that time (overlap is
checked only within one course); week parity is still ignored when generating weeks (as
before in `generate_from_templates`); a group course shows in a class timetable only if
the group has students of that class.
