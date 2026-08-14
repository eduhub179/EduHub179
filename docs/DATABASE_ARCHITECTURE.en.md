# 📘 Master Document: Database Schema of the School System

**Version:** 1.0  
**Date:** 2026-08-01  
**Target audience:** developers, architects, administrators

---

## 📋 Table of Contents

1. [Introduction](#1-introduction)
2. [Entity overview](#2-entity-overview)
3. [Migration files](#3-migration-files)
4. [Table relationships](#4-table-relationships)
5. [Key architectural decisions](#5-key-architectural-decisions)
6. [Usage scenarios](#6-usage-scenarios)
7. [Migration execution order](#7-migration-execution-order)
8. [Open questions](#8-open-questions)

---

## 1. Introduction

This document describes the database schema for a school system implementing:

- **User management** (students, teachers, admins)
- **Academic process** (classes, groups, subjects, lessons)
- **Schedule** (lessons, substitutions, events, classrooms)
- **Homework** (text, files, moderation)
- **Plusnik** (a "problems × students" matrix)

The system is designed with:
- Separation of abstraction and concreteness (a lesson vs a specific lesson on a date)
- Support for multiple teachers per subject
- Flexible scheduling (regular lessons, clubs, lectures, substitutions)
- Anonymity of students when editing homework
- Full change history (who, when, what was awarded/revoked)

---

## 2. Entity overview

### 2.1 Users and roles

| Entity | Purpose |
|--------|---------|
| `users` | All system users (students, teachers, admins) |
| `user_role` (ENUM) | Roles: `student`, `teacher`, `admin` |

### 2.2 Academic structure

| Entity | Purpose |
|--------|---------|
| `classes` | School classes (graduation year + letter) |
| `subjects` | Subjects (Algebra, Spetsmat, Informatics) |
| `student_groups` | Groups of students (can unite students from different classes) |
| `group_members` | Link "student ↔ group" |
| `lessons` | Abstract lesson (class/group + subject) |
| `lesson_teachers` | Link "lesson ↔ teachers" (many-to-many) |

### 2.3 Schedule

| Entity | Purpose |
|--------|---------|
| `cabinets` | Classrooms (3-digit number, floor) |
| `lesson_templates` | Lesson templates (time + classroom + day of week) |
| `lesson_instances` | Specific lessons on specific dates (template + week + date) |
| `events` | Events (lectures, activities) |
| `event_attendees` | Event participants |
| `day_of_week` (ENUM) | Days of the week |
| `week_parity` (ENUM) | Periodicity (every week / odd / even) |

### 2.4 Homework

| Entity | Purpose |
|--------|---------|
| `homeworks` | Homework (tied to a specific lesson on a date) |
| `homework_files` | Homework files (metadata + S3 reference) |
| `homework_status` (ENUM) | Homework statuses: `draft`, `published`, `archived` |

### 2.5 Plusnik

| Entity | Purpose |
|--------|---------|
| `plusnik_sheets` | Problem worksheets (tied to an abstract lesson) |
| `plusnik_tasks` | Problems inside a worksheet |
| `plusnik_records` | Pluses (who, when, to whom, for what) |
| `sheet_status` (ENUM) | Worksheet statuses: `draft`, `published`, `archived` |

---

## 3. Migration files

### `0001_create_users.sql`

**Tables:**
- `users` — all system users
- `user_role` (ENUM) — roles

**Functions:**
- `update_updated_at_column()` — a universal function for `updated_at` triggers

**Key `users` columns:**
- `user_id` — UUID (PK)
- `email` — unique email
- `role` — user role
- `last_name`, `first_name`, `middle_name` — full name
- `is_active` — account state

---

### `0002_create_subjects_and_classes.sql`

**Tables:**
- `classes` — school classes
- `subjects` — subjects
- `student_groups` — groups of students
- `group_members` — group composition
- `lessons` — abstract lessons
- `lesson_teachers` — lesson teachers

**ENUM:**
- `class_letter` — class letters ('б', 'в', 'и')

**Key points:**
- The class number is computed from `graduation_year` and the current date
- Groups are NOT tied to a single class (they can unite students from different classes)
- Lesson = (class OR group) + subject (CHECK constraint)
- Teachers — a separate many-to-many relationship

---

### `0003_create_schedule.sql` ⚠️

**WARNING:** runs BEFORE `0004` (`homeworks`), because `homeworks` references `lesson_instances`.

**Tables:**
- `cabinets` — classrooms
- `lesson_templates` — lesson templates
- `lesson_instances` — specific lessons on dates (carries `template_id` + `week_start_date` directly — no separate slot table)
- `events` — events (lectures, activities)
- `event_attendees` — event participants

**ENUM:**
- `day_of_week` — days of the week
- `week_parity` — periodicity

**Functions:**
- `check_teacher_available()` — checks teacher availability
- `get_student_schedule_for_date()` — the student's full schedule for a date

**Key points:**
- Lesson template = (lesson + day + time + classroom + periodicity)
- The `is_active` flag in templates — for quick availability checks
- The `is_override` flag — for lesson substitutions
- Events override lessons in the student's schedule

---

### `0003_create_homework.sql` ⚠️

**WARNING:** runs AFTER `0005`, because it references `lesson_instances`.

**Tables:**
- `homeworks` — homework
- `homework_files` — homework files

**ENUM:**
- `homework_status` — homework statuses

**Key points:**
- Homework is tied to `lesson_instance_id` (a specific lesson on a date)
- Exactly one homework per lesson (UNIQUE)
- Editing rules via `created_by_role` + `locked_by_teacher`
- Student anonymity via `last_edited_by` (visible only to admins)
- Content validation — at the application level (not a trigger)

---

### `0004_create_plusnik.sql`

**Tables:**
- `plusnik_sheets` — problem worksheets
- `plusnik_tasks` — problems inside a worksheet
- `plusnik_records` — pluses (records of solved problems)

**ENUM:**
- `sheet_status` — worksheet statuses

**Functions:**
- `check_task_belongs_to_sheet()` — trigger checking that a problem belongs to a worksheet

**Key points:**
- A worksheet is tied to an abstract lesson (`lesson_id`)
- Problems are a separate table with `sort_order` (for adding/removing from the middle)
- Pluses — a journal of all changes (who awarded, who revoked)
- The "problems × students" matrix is generated on the fly via `CROSS JOIN`

---

## 4. Table relationships

```
users (students, teachers, admins)
  ├── class_id → classes (for students)
  ├── ← group_members.student_id (group membership)
  ├── ← lesson_teachers.teacher_id (for teachers)
  ├── ← homeworks.created_by
  ├── ← homeworks.last_edited_by
  ├── ← plusnik_records.student_id / granted_by / revoked_by
  └── ← events.organizer_id

classes (school classes)
  ├── ← users.class_id
  ├── ← lessons.class_id
  └── graduation_year + letter → class number (computed)

subjects (subjects)
  └── ← lessons.subject_id

student_groups (student groups)
  ├── ← group_members.group_id
  └── ← lessons.group_id

group_members (student ↔ group)
  ├── student_id → users
  └── group_id → student_groups

lessons (abstract lessons)
  ├── class_id → classes (NULL if group)
  ├── group_id → student_groups (NULL if class)
  ├── subject_id → subjects
  ├── ← lesson_teachers.lesson_id
  ├── ← lesson_templates.lesson_id
  └── ← plusnik_sheets.lesson_id

lesson_teachers (lesson ↔ teachers)
  ├── lesson_id → lessons
  └── teacher_id → users

lesson_templates (lesson templates)
  ├── lesson_id → lessons
  ├── cabinet_id → cabinets
  ├── is_active (activity flag)
  ├── is_override (substitution flag)
  └── ← lesson_instances.template_id

lesson_instances (specific lessons on dates)
  ├── template_id → lesson_templates
  ├── week_start_date
  ├── lesson_date
  ├── status (scheduled / completed / cancelled)
  └── ← homeworks.lesson_instance_id

cabinets (classrooms)
  ├── number (3-digit)
  ├── floor (computed)
  └── ← lesson_templates.cabinet_id

events (events)
  ├── cabinet_id → cabinets
  ├── organizer_id → users
  └── ← event_attendees.event_id

event_attendees (student ↔ event)
  ├── event_id → events
  └── student_id → users

homeworks (homework)
  ├── lesson_instance_id → lesson_instances
  ├── created_by → users
  ├── last_edited_by → users
  └── ← homework_files.homework_id

homework_files (homework files)
  └── homework_id → homeworks

plusnik_sheets (worksheets)
  ├── lesson_id → lessons
  ├── created_by → users
  ├── deadline (optional)
  └── ← plusnik_tasks.sheet_id
  └── ← plusnik_records.sheet_id

plusnik_tasks (problems)
  ├── sheet_id → plusnik_sheets
  ├── task_number
  ├── sort_order
  └── ← plusnik_records.task_id

plusnik_records (pluses)
  ├── student_id → users
  ├── sheet_id → plusnik_sheets
  ├── task_id → plusnik_tasks
  ├── granted_by → users
  ├── revoked_by → users (nullable)
  └── revoked_at (nullable)
```

---

## 5. Key architectural decisions

### 5.1 Separation of abstraction and concreteness

**Level 1: Abstract lesson (`lessons`)**
- What: class/group + subject
- Example: "Spetsmat in 10б"
- Used for: plusnik, teacher groups

**Level 2: Lesson template (`lesson_templates`)**
- What: lesson + day + time + classroom
- Example: "Spetsmat in 10б, Mon 10:50-11:35, room 412"
- Used for: schedule, availability checks

**Level 3: Specific lesson (`lesson_instances`)**
- What: template + week + date
- Example: "Spetsmat in 10б, 28.07.2026 (template T1, week of 28.07.2026)"
- Used for: homework, history, week-level overrides

### 5.2 Multiple teachers per lesson

One lesson can be taught by several teachers (e.g., Spetsmat in 10б is taught by 4 teachers).

**Solution:** a many-to-many relationship via `lesson_teachers`.

**Advantages:**
- Homework, pluses, schedule — shared by all teachers
- No duplication of lessons
- Easy to add/remove a teacher

### 5.3 Student groups from different classes

Groups can unite students from different classes (e.g., "English B1" — students from 10а, 10б, 10в).

**Solution:** `student_groups` is not tied to a class.

**Advantages:**
- Flexible level-based grouping
- No need to create separate classes

### 5.4 Homework content validation at the application level

**Problem:** if validated at the DB level (trigger), a race condition occurs when uploading a file.

**Solution:** validation in the use case + DTO validation.

**Advantages:**
- No file upload issues
- Clear errors
- Testability

### 5.5 Plusnik — the matrix on the fly

**We don't store** the "problems × students" matrix in the DB.

**We generate it on the fly:**
```sql
SELECT ... FROM plusnik_tasks t
CROSS JOIN users u
LEFT JOIN plusnik_records pr ON ...
```

**Advantages:**
- No data duplication
- The matrix is always up to date
- Schema simplicity

### 5.6 Events override lessons

If a student participates in an event (lecture) that overlaps with a lesson — the event is shown in the schedule.

**Solution:** the `get_student_schedule_for_date()` function with `NOT EXISTS` to exclude overlaps.

### 5.7 Lesson templates with flags

**`is_active`:**
- TRUE — the template is used in the current schedule
- FALSE — archived, doesn't participate in availability checks
- Allows quickly checking teacher availability

**`is_override`:**
- TRUE — substitution template
- FALSE — regular template
- Simplifies archiving substitutions

---

## 6. Usage scenarios

### 6.1 A student opens the day's schedule

```sql
SELECT * FROM get_student_schedule_for_date('student_id', '2026-07-28');
```

**Result:**
```
09:00-09:45  Algebra (room 412)
09:55-10:40  Russian language (room 305)
10:50-11:35  Quantum physics lecture (assembly hall)  ← the event overrides the lesson
11:55-12:40  History (room 201)
```

### 6.2 A teacher creates homework

```rust
// 1. Validation: at least text or a file
if cmd.text_content.is_none() && cmd.files.is_empty() {
    return Err(DomainError::EmptyHomework);
}

// 2. Upload files to S3
let file_records = file_storage.upload_files(cmd.files).await?;

// 3. Create the homework + files in a single transaction
let homework = homework_repository.create_with_files(cmd, file_records).await?;
```

### 6.3 A teacher awards a plus

```sql
INSERT INTO plusnik_records (student_id, sheet_id, task_id, granted_by)
VALUES ('student_id', 'sheet_id', 'task_id', 'teacher_id');
```

**Fail-safe:**
- A unique index prevents awarding two pluses for the same problem
- A trigger checks that the problem belongs to the worksheet

### 6.4 An admin builds the schedule

```sql
-- Check if the teacher is busy
SELECT check_teacher_available('teacher_id', 'пн', '10:50', '11:35');

-- Create a lesson template
INSERT INTO lesson_templates (lesson_id, day, start_time, end_time, cabinet_id)
VALUES ('lesson_id', 'пн', '10:50', '11:35', 'cabinet_id');

-- Create a lesson instance for the week
INSERT INTO lesson_instances (template_id, week_start_date, lesson_date)
VALUES ('template_id', '2026-07-27', '2026-07-27');
```

### 6.5 Lesson substitution (teacher is sick)

```sql
-- Create a substitution template
INSERT INTO lesson_templates (lesson_id, day, start_time, end_time, cabinet_id, is_override, comment)
VALUES ('new_lesson_id', 'пн', '10:50', '11:35', 'cabinet_id', TRUE, 'Ivanov is sick');

-- Point this week's lesson at the substitution template
UPDATE lesson_instances
SET template_id = 'new_template_id'
WHERE instance_id = 'instance_id';
```

### 6.6 Part of the class goes to a lecture

```sql
-- Create an event
INSERT INTO events (title, start_time, end_time, cabinet_id, organizer_id)
VALUES ('Quantum physics lecture', '2026-07-28 10:50', '2026-07-28 11:35', 'cabinet_id', 'teacher_id');

-- Add participants
INSERT INTO event_attendees (event_id, student_id) VALUES
    ('event_id', 'student_1'),
    ('event_id', 'student_2');
```

**The student's schedule:**
- If the student participates in an event → the event is shown
- Otherwise → the regular lesson is shown

---

## 7. Migration execution order

```
0001_create_users.sql              ← independent
    ↓
0002_create_subjects_and_classes.sql ← depends on 0001
    ↓
0003_create_schedule.sql           ← depends on 0002
    ↓
0004_create_homework.sql           ← depends on 0003 (lesson_instances)
    ↓
0005_create_plusnik.sql            ← depends on 0002 (lessons)
```

**Important:** `0003` and `0004` can run in any order after `0005`, but `0003` must run AFTER `0005`.

---

## 8. Open questions

### 8.1 Partial grading in the plusnik

Currently a plus is binary (yes/no). If partial grading is needed (0.5 plus) — add a `score NUMERIC(3,2)` column.

### 8.2 Comments on pluses

Currently there is only `revoke_comment` for revocations. If comments are needed when awarding — add a `comment` column.

### 8.3 Problem change history

Currently problems can be updated, but the history is not saved. If history is needed — add a `plusnik_tasks_history` table.

### 8.4 Recurring events

Currently events are one-off. If recurring events are needed (a lecture every week) — add `is_recurring` + `recurrence_pattern` columns.

### 8.5 Materialized view for availability checks

For large schools (>100 teachers) a materialized view `teacher_schedule` could be created to speed up checks.

---

## 📊 Final statistics

| Metric | Value |
|--------|-------|
| **Number of tables** | 18 |
| **Number of ENUMs** | 6 |
| **Number of functions** | 4 |
| **Number of triggers** | 10 |
| **Number of indexes** | ~50 |
| **Migration files** | 5 |

---

## 🎯 Conclusion

The database schema is designed with:
- **Flexibility** — support for different lesson types, groups, events
- **Performance** — partial indexes, denormalization where needed
- **Fail-safe** — CHECK constraints, triggers, unique indexes
- **Scalability** — separation of abstraction and concreteness
- **Simplicity** — minimum tables, clear relationships

The next step is implementing the Rust code: domain entities, repositories, use cases.
