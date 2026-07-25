-- ============================================
-- 1. СТАТУСЫ ДОМАШНЕГО ЗАДАНИЯ
-- draft     — черновик, виден только создателю
-- published — опубликовано, виден всем ученикам урока
-- archived  — архивировано, скрыто из активного списка
-- ============================================
CREATE TYPE homework_status AS ENUM ('draft', 'published', 'archived');


-- ============================================
-- 2. ДОМАШНИЕ ЗАДАНИЯ
-- Под одним уроком ровно одно ДЗ (UNIQUE по lesson_id).
-- Создать ДЗ может учитель или школьник (дежурный).
--
-- Правила редактирования:
-- - Если ДЗ создал учитель → только учитель может менять
-- - Если ДЗ создал школьник и учитель не взаимодействовал → любой школьник может менять
-- - Если учитель взаимодействовал с ДЗ → только учитель может менять
--
-- Анонимность:
-- - Кто из школьников создал или редактировал — видно только админу
-- - Если учитель взаимодействовал — это видно всем
--
-- Проверка контента (хотя бы текст или файл) — на уровне приложения (use case).
-- Триггер на уровне БД не используется, чтобы избежать проблем с загрузкой файлов.
-- ============================================
CREATE TABLE homeworks
(
    homework_id           UUID PRIMARY KEY         DEFAULT gen_random_uuid(),

    -- Урок, к которому относится ДЗ (ровно одно ДЗ на урок)
    lesson_id             UUID            NOT NULL REFERENCES lessons (lesson_id) ON DELETE RESTRICT,

    -- Кто создал ДЗ (для аудита, если ученик, видно только админу)
    created_by            UUID            NOT NULL REFERENCES users (user_id) ON DELETE RESTRICT,

    -- Роль создателя (для быстрой проверки прав без JOIN)
    -- 'teacher' — создал учитель
    -- 'student' — создал школьник
    created_by_role       user_role       NOT NULL,

    -- Текстовое содержимое ДЗ (опционально — может быть только файл)
    text_content          TEXT NULL,

    -- Статус ДЗ
    status                homework_status NOT NULL DEFAULT 'draft',

    -- Флаг блокировки учителем
    -- Если true → школьники больше не могут редактировать
    -- Если false → школьники могут редактировать
    locked_by_teacher     BOOLEAN         NOT NULL DEFAULT FALSE,

    -- Кто последний редактировал (для аудита, если ученик, видно только админу)
    -- NULL при создании. Автор не считается редактором
    last_edited_by        UUID NULL REFERENCES users(user_id) ON DELETE RESTRICT,

    -- Временные метки
    created_at            TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    updated_at            TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

-- Ровно одно ДЗ на урок
CREATE UNIQUE INDEX idx_homeworks_lesson_unique
    ON homeworks (lesson_id);

-- Быстрый поиск ДЗ конкретного урока
-- Используется, когда ученик открывает список ДЗ по предмету
CREATE INDEX idx_homeworks_lesson
    ON homeworks (lesson_id) WHERE status = 'published';

-- Быстрый поиск ДЗ, с которыми взаимодействовал учитель
-- Используется для отображения статуса "учитель проверил"
CREATE INDEX idx_homeworks_teacher_interacted
    ON homeworks (lesson_id) WHERE status = 'published' AND locked_by_teacher = TRUE;


-- ============================================
-- 3. ФАЙЛЫ ДОМАШНЕГО ЗАДАНИЯ
-- Одно ДЗ может иметь несколько файлов (PDF, фото условия, сканы).
-- В базе хранятся только метаданные и ссылка на S3/MinIO,
-- сами файлы — в объектном хранилище (§5 мастер-документа).
-- ============================================
CREATE TABLE homework_files
(
    file_id     UUID PRIMARY KEY      DEFAULT gen_random_uuid(),

    -- ДЗ, к которому относится файл
    homework_id UUID         NOT NULL REFERENCES homeworks (homework_id) ON DELETE CASCADE,

    -- Путь/ключ файла в S3-хранилище (например "homeworks/2026/07/abc123.pdf")
    storage_key VARCHAR(500) NOT NULL,

    -- Оригинальное имя файла для отображения пользователю
    file_name   VARCHAR(255) NOT NULL,

    -- MIME-тип файла (application/pdf, image/jpeg и т.д.)
    mime_type   VARCHAR(100) NOT NULL,

    -- Размер файла в байтах (для отображения и лимитов)
    size_bytes  BIGINT       NOT NULL CHECK (size_bytes >= 0),

    -- Порядок отображения файлов (если их несколько)
    sort_order  INT          NOT NULL DEFAULT 0,

    -- Временные метки
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Быстрый поиск всех файлов конкретного ДЗ
-- Используется при отображении ДЗ ученику
CREATE INDEX idx_homework_files_homework
    ON homework_files (homework_id, sort_order);


-- ============================================
-- 4. ТРИГГЕРЫ ДЛЯ ОБНОВЛЕНИЯ updated_at
-- ============================================
CREATE TRIGGER trigger_homeworks_updated_at
    BEFORE UPDATE
    ON homeworks
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
