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
