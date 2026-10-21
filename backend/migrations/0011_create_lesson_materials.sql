-- ============================================================================
-- FILE: 0011_create_lesson_materials.sql
-- PURPOSE: Course materials: topics of a course (lesson) and files / links in them.
-- DEPENDENCIES: [0001_create_users.sql, 0002_create_subjects_and_classes.sql]
-- ============================================================================

-- ============================================
-- 1. TOPICS OF A COURSE
-- A course is a row of `lessons` (subject + class/group), e.g. "Матанализ — 2027в".
-- Topics group its materials: "Пределы", "Производные".
-- ============================================
CREATE TABLE lesson_material_topics
(
    topic_id   UUID PRIMARY KEY      DEFAULT gen_random_uuid(),

    -- RESTRICT: a course with materials is deactivated (is_active), not deleted.
    lesson_id  UUID         NOT NULL REFERENCES lessons (lesson_id) ON DELETE RESTRICT,

    -- Shown in the topic list; 1-255 chars, checked in the domain too.
    name       VARCHAR(255) NOT NULL CHECK (length(trim(name)) > 0),

    -- Display order inside the course (new topics go to the end).
    sort_order INT          NOT NULL DEFAULT 0,

    created_by UUID         NOT NULL REFERENCES users (user_id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ  NOT NULL DEFAULT NOW(),

    -- Target of the composite FK from lesson_materials: lets the DB guarantee that a
    -- material and its topic belong to the same course.
    CONSTRAINT uq_lesson_material_topics_lesson_topic UNIQUE (lesson_id, topic_id)
);

-- ИНДЕКС: idx_lesson_material_topics_name
-- Цель: запрещает две темы с одинаковым названием в одном курсе.
-- Тип: уникальный B-tree по lower(name) — «Пределы» и «пределы» считаются одной темой.
CREATE UNIQUE INDEX idx_lesson_material_topics_name
    ON lesson_material_topics (lesson_id, lower(name));

-- ИНДЕКС: idx_lesson_material_topics_order
-- Цель: список тем курса в порядке показа (GET .../material-topics).
-- Тип: B-tree (lesson_id, sort_order) — фильтр по курсу и готовая сортировка.
CREATE INDEX idx_lesson_material_topics_order
    ON lesson_material_topics (lesson_id, sort_order);


-- ============================================
-- 2. MATERIALS
-- Exactly one kind of content per row:
-- - file: object in S3 (MinIO), DB keeps only metadata (like homework_files);
-- - link: an http(s) URL (video, Google Docs, ...).
-- ============================================
CREATE TABLE lesson_materials
(
    material_id UUID PRIMARY KEY      DEFAULT gen_random_uuid(),

    -- Course of the material; duplicated from the topic so that materials without
    -- a topic still belong to a course and permission checks need no join.
    lesson_id   UUID         NOT NULL REFERENCES lessons (lesson_id) ON DELETE RESTRICT,

    -- NULL = "Без темы". Deleting a topic moves its materials there (SET NULL only
    -- for topic_id, PostgreSQL 15+), materials are never lost with a topic.
    topic_id    UUID         NULL,

    -- Card title (for files defaults to the file name) and optional description.
    title       VARCHAR(255) NOT NULL CHECK (length(trim(title)) > 0),
    description TEXT         NULL,

    -- File content: S3 object key "materials/<uuid>_<name>", original name, type, size.
    storage_key VARCHAR(500) NULL,
    file_name   VARCHAR(255) NULL,
    mime_type   VARCHAR(100) NULL,
    size_bytes  BIGINT       NULL CHECK (size_bytes >= 0),

    -- Link content: http(s) only (checked in the domain: no javascript: URLs).
    url         VARCHAR(2000) NULL,

    -- Display order inside the topic.
    sort_order  INT          NOT NULL DEFAULT 0,

    created_by  UUID         NOT NULL REFERENCES users (user_id) ON DELETE RESTRICT,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),

    -- The topic must belong to the same course (MATCH SIMPLE: no check when topic_id IS NULL).
    CONSTRAINT fk_lesson_materials_topic
        FOREIGN KEY (lesson_id, topic_id)
            REFERENCES lesson_material_topics (lesson_id, topic_id)
            ON DELETE SET NULL (topic_id),

    -- Exactly one kind of content: all file fields, or a URL.
    CONSTRAINT chk_lesson_materials_one_content CHECK (
        (url IS NULL AND storage_key IS NOT NULL AND file_name IS NOT NULL
            AND mime_type IS NOT NULL AND size_bytes IS NOT NULL)
            OR
        (url IS NOT NULL AND storage_key IS NULL AND file_name IS NULL
            AND mime_type IS NULL AND size_bytes IS NULL)
        )
);

-- ИНДЕКС: idx_lesson_materials_lesson_topic
-- Цель: материалы одной темы курса в порядке показа и подсчёт материалов по темам
--       (GROUP BY topic_id) — оба запроса страницы материалов.
-- Тип: B-tree (lesson_id, topic_id, sort_order); NULL topic_id тоже индексируется,
--      поэтому «Без темы» ищется по индексу.
CREATE INDEX idx_lesson_materials_lesson_topic
    ON lesson_materials (lesson_id, topic_id, sort_order);

CREATE TRIGGER trigger_lesson_materials_updated_at
    BEFORE UPDATE
    ON lesson_materials
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
