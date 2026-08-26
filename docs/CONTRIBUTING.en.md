Here is a clear, structured, and mandatory code documentation standard. It was designed around your requirements for brevity, explaining "why", fail-safe behavior, and performance.

---

# 📜 Code Documentation Standard (SQL & Rust)

**Goal:** Make the code self-documenting. Any developer should be able to understand the purpose of an entity, its constraints, and the reasons behind architectural decisions without leaving the file.
**Main rule:** Comments explain **"why"** and **"what guarantees it provides"**, not repeat the variable name. Comments must be **short**.

---

## 🗄️ Part 1. SQL Standard (Migrations)

### 1. File header (Required)
Every migration file must start with this block.
```sql
-- ============================================================================
-- FILE: 000X_<entity_name>.sql
-- PURPOSE: Briefly (1 sentence) what the file creates.
-- DEPENDENCIES: [000Y_file.sql] or "None".
-- MASTER-DOC: Section X.Y
-- ============================================================================
```

### 2. Tables and columns
*   Group logically related columns together.
*   Write a comment **above** the column or group of columns.
*   If the column name doesn't convey the full picture (format, constraints, units), this **must** be stated in the comment.

```sql
CREATE TABLE homework_files
(
    file_id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    homework_id  UUID NOT NULL REFERENCES homeworks(homework_id) ON DELETE CASCADE,
    
    -- Reference to an object in S3.
    -- IMPORTANT: Guaranteed at the application level to be only .pdf or .jpg.
    s3_url       VARCHAR(500) NOT NULL,
    
    -- File size in bytes. Needed for quotas and quick integrity checks on download.
    size_bytes   BIGINT NOT NULL CHECK (size_bytes > 0),
    
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

### 3. Indexes (Justification required)
Every index must have a comment answering two questions: **which query it speeds up** and **why this index type was chosen** (especially for partial indexes).

```sql
-- INDEX: idx_homework_files_homework
-- Purpose: Speeds up fetching all files of a specific homework when rendering the page.
-- Type: Regular B-tree, because queries match on the exact homework_id.
CREATE INDEX idx_homework_files_homework ON homework_files (homework_id);

-- INDEX: idx_users_active_students
-- Purpose: Speeds up finding students for grade entry.
-- Type: Partial (WHERE is_active = TRUE). Reduces index size and speeds up lookups,
--      because inactive users are not needed in this scenario.
CREATE INDEX idx_users_active_students ON users (last_name, first_name) 
WHERE role = 'student' AND is_active = TRUE;
```

### 4. Functions and Triggers
A brief description of the purpose, inputs, and fail-safe guarantees.

```sql
-- FUNCTION: check_teacher_available
-- Purpose: Checks whether a teacher's schedule overlaps.
-- Fail-safe: Ignores archived templates (is_active = FALSE), preventing false conflicts.
-- Returns: TRUE if the teacher is free in the given interval.
```

---

## 🦀 Part 2. Rust Standard

Given your preference for encapsulation, absence of panics (fail-safe), and care about performance, the Rust standard is built around these principles.

### 1. Module/file header (Required)
```rust
//! Module responsible for managing the lesson schedule.
//! 
//! Dependencies: `crate::users`, `crate::database`
//! Guarantees: All public methods return `Result` or `Option`; panics are not allowed.
```

### 2. Structs and Encapsulation
Use `///` to document public structs. Describe the invariants (what this struct guarantees). Fields with non-obvious purposes are commented with `//`.

```rust
/// A lesson template in the schedule.
/// 
/// Invariant: `end_time` is always strictly greater than `start_time`. 
/// Guaranteed at the validation level before saving to the DB.
pub struct LessonTemplate {
    pub id: Uuid,
    pub lesson_id: Uuid,
    
    /// Periodicity: every week, odd weeks only, or even weeks only.
    pub parity: WeekParity,
    
    is_active: bool,
}
```

### 3. Functions and Methods
Public functions are documented with `///`. You must describe how the function behaves with invalid data (fail-safe).

```rust
impl LessonTemplate {
    /// Checks whether the teacher is available at the given time.
    /// 
    /// # Fail-safe behavior
    /// Returns `Ok(true)` if the teacher is free. 
    /// Returns `Ok(false)` on any overlap.
    /// Returns `Err` only on a DB connection failure (never panics).
    pub async fn is_teacher_available(
        &self,
        db: &DatabasePool,
        teacher_id: Uuid,
    ) -> Result<bool, DbError> {
        // ...
    }
}
```

### 4. Complex logic and optimizations (Inline comments)
If low-level optimization, a persistent data structure, or a non-obvious algorithm is used, add a short `//` comment explaining **why** it is done this way.

```rust
pub fn calculate_schedule_matrix(&self, students: &[Student]) -> Matrix {
    // Use a flat array and AVX instructions instead of nested loops,
    // because profiling showed this gives a 3x performance gain
    // with more than 500 students.
    let mut matrix = Matrix::with_capacity(students.len());
    // ...
}
```

---

## ✅ Code Review Checklist (Check before merging)

Apply this checklist to yourself and your team when reviewing code:

- [ ] **SQL/Rust:** Does the file start with a header describing its purpose and dependencies?
- [ ] **SQL:** Do all non-obvious columns (formats, units, specific constraints) have a short comment *above* them?
- [ ] **SQL:** Does *every* index have a comment explaining which query it speeds up and why its type was chosen?
- [ ] **Rust:** Do public structs and methods have `///` documentation?
- [ ] **Rust:** Is the fail-safe behavior of functions described (what is returned on error, absence of panics)?
- [ ] **General:** Are comments short, to the point, and explaining "why" rather than repeating the variable name?
