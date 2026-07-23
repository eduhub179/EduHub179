-- ============================================
-- 1. НАЗВАНИЯ КЛАССОВ
-- ============================================
CREATE TYPE class_letter AS ENUM ('б', 'в', 'и'); --TODO: реальные названия


-- ============================================
-- 2. КЛАССЫ ШКОЛЫ
-- ============================================
CREATE TABLE classes
(
    id              UUID PRIMARY KEY      DEFAULT gen_random_uuid(),

    -- Класс
    graduation_year INT          NOT NULL,
    letter          class_letter NOT NULL,

    -- Активен ли класс (выпускники становятся неактивными)
    is_active       BOOLEAN      NOT NULL DEFAULT TRUE,

    -- Временные метки
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Нельзя создать два класса с одинаковым годом выпуска и буквой
CREATE UNIQUE INDEX idx_classes_year_letter
    ON classes (graduation_year, letter);

-- Быстрый поиск активных классов (для админки)
CREATE INDEX idx_classes_active ON classes (is_active) WHERE is_active = TRUE;

-- Быстрый поиск по году выпуска (например, "покажи все классы, которые выпустятся в 2027")
CREATE INDEX idx_classes_graduation_year ON classes (graduation_year) WHERE is_active = TRUE;

