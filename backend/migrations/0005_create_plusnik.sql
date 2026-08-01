-- 0005_create_plusnik.sql

-- ============================================
-- 1. СТАТУСЫ ЛИСТКА
-- draft     — черновик, виден только учителю-создателю
-- published — опубликован, виден ученикам
-- archived  — архивирован, скрыт из активного списка, но история сохраняется
-- ============================================
CREATE TYPE sheet_status AS ENUM ('draft', 'published', 'archived');


-- ============================================
-- 2. ЛИСТКИ ЗАДАЧ (СПЕЦМАТ / ПЛЮСНИК)
-- Листок привязывается к уроку (lesson_id), а не к учителю.
-- Если урок ведут несколько учителей — листок общий,
-- все учителя могут ставить плюсы за задачи из этого листка.
-- ============================================
CREATE TABLE plusnik_sheets
(
    sheet_id   UUID PRIMARY KEY      DEFAULT gen_random_uuid(),

    -- Урок, к которому относится листок
    lesson_id  UUID         NOT NULL REFERENCES lessons (lesson_id) ON DELETE RESTRICT,

    -- Автор листка (учитель, который его создал)
    created_by UUID         NOT NULL REFERENCES users (user_id) ON DELETE RESTRICT,

    -- Название листка: "Листок 12: Производные"
    name       VARCHAR(255) NOT NULL,

    -- Дата выдачи листка ученикам
    issue_date DATE         NOT NULL,

    -- Дедлайн сдачи листка (опционально)
    -- По завершению дедлайна ничего автоматического не происходит —
    -- это просто информационное поле для отображения ученикам и учителям
    deadline   TIMESTAMPTZ NULL,

    -- Статус листка (черновик / опубликован / архив)
    status     sheet_status NOT NULL DEFAULT 'draft',

    -- Временные метки
    created_at TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Быстрый поиск всех листков конкретного урока
CREATE INDEX idx_plusnik_sheets_lesson
    ON plusnik_sheets (lesson_id) WHERE status = 'published';

-- Быстрый поиск листков конкретного учителя (для редактирования)
CREATE INDEX idx_plusnik_sheets_created_by
    ON plusnik_sheets (created_by);

-- Быстрый поиск черновиков учителя (для продолжения работы)
CREATE INDEX idx_plusnik_sheets_drafts
    ON plusnik_sheets (created_by, created_at DESC) WHERE status = 'draft';

-- Быстрый поиск листков по дате выдачи (для сортировки в UI)
CREATE INDEX idx_plusnik_sheets_issue_date
    ON plusnik_sheets (lesson_id, issue_date DESC) WHERE status = 'published';

-- Быстрый поиск листков с дедлайном (для напоминаний)
CREATE INDEX idx_plusnik_sheets_deadline
    ON plusnik_sheets (deadline) WHERE status = 'published' AND deadline IS NOT NULL;


-- ============================================
-- 3. ЗАДАЧИ ВНУТРИ ЛИСТКА
-- Каждая задача — отдельная строка. Номера короткие: "1а", "2б*", "3".
-- sort_order определяет порядок отображения и позволяет
-- добавлять/удалять задачи из любого места (O(n) для n < 50 = ~1 мс).
-- ============================================
CREATE TABLE plusnik_tasks
(
    task_id     UUID PRIMARY KEY     DEFAULT gen_random_uuid(),

    -- Листок, к которому относится задача
    sheet_id    UUID        NOT NULL REFERENCES plusnik_sheets (sheet_id) ON DELETE CASCADE,

    -- Номер задачи: "1а", "1б", "2", "3а*", "10"
    task_number VARCHAR(20) NOT NULL,

    -- Порядок отображения задач в листке
    -- Обновляется при добавлении/удалении задач из середины
    sort_order  INT         NOT NULL,

    -- Временные метки
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Нельзя создать две задачи с одинаковым номером в одном листке
CREATE UNIQUE INDEX idx_plusnik_tasks_unique
    ON plusnik_tasks (sheet_id, task_number);

-- Быстрый поиск всех задач конкретного листка в правильном порядке
CREATE INDEX idx_plusnik_tasks_sheet_order
    ON plusnik_tasks (sheet_id, sort_order);


-- ============================================
-- 4. ЗАПИСИ ПЛЮСНИКА (КТО СДАЛ КАКУЮ ЗАДАЧУ)
-- Каждая запись — один "плюс" за конкретную задачу.
--
-- История изменений хранится прямо в записи:
-- - granted_at / granted_by  — кто и когда поставил
-- - revoked_at / revoked_by  — кто и когда отозвал
-- Отзыв не удаляет строку, а заполняет revoked_at.
-- Это снимает споры "кто и когда поставил" (§11.4 мастер-документа).
-- ============================================
CREATE TABLE plusnik_records
(
    record_id      UUID PRIMARY KEY     DEFAULT gen_random_uuid(),

    -- Ученик, которому поставлен плюс
    student_id     UUID        NOT NULL REFERENCES users (user_id) ON DELETE RESTRICT,

    -- Листок, к которому относится задача
    sheet_id       UUID        NOT NULL REFERENCES plusnik_sheets (sheet_id) ON DELETE RESTRICT,

    -- Конкретная задача (обязательно — плюс ставится только за задачу)
    task_id        UUID        NOT NULL REFERENCES plusnik_tasks (task_id) ON DELETE RESTRICT,

    -- Учитель, который поставил плюс
    granted_by     UUID        NOT NULL REFERENCES users (user_id) ON DELETE RESTRICT,

    -- Когда плюс поставлен
    granted_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Когда плюс отозван (NULL, если активен)
    revoked_at     TIMESTAMPTZ NULL,

    -- Кто отозвал плюс (NULL, если активен)
    revoked_by     UUID NULL REFERENCES users(user_id) ON DELETE RESTRICT,

    -- Комментарий при отзыве (опционально)
    revoke_comment TEXT NULL
);

-- Нельзя поставить два активных плюса за одну задачу одному ученику
-- Частичный индекс: только для активных (не отозванных) записей
CREATE UNIQUE INDEX idx_plusnik_records_active_unique
    ON plusnik_records (student_id, task_id) WHERE revoked_at IS NULL;

-- Быстрый поиск всех активных записей ученика (для дашборда ученика)
CREATE INDEX idx_plusnik_records_student_active
    ON plusnik_records (student_id, granted_at DESC) WHERE revoked_at IS NULL;

-- Быстрый поиск всех записей ученика включая отозванные (для истории)
CREATE INDEX idx_plusnik_records_student_all
    ON plusnik_records (student_id, granted_at DESC);

-- Быстрый поиск всех активных записей конкретного листка (для матрицы учителя)
CREATE INDEX idx_plusnik_records_sheet_active
    ON plusnik_records (sheet_id) WHERE revoked_at IS NULL;

-- Быстрый поиск всех активных записей конкретной задачи (для статистики)
CREATE INDEX idx_plusnik_records_task_active
    ON plusnik_records (task_id) WHERE revoked_at IS NULL;

-- Быстрый поиск записей, поставленных учителем (для истории действий)
CREATE INDEX idx_plusnik_records_granted_by
    ON plusnik_records (granted_by, granted_at DESC);

-- Если плюс отозван — обязательно указан, кто отозвал
ALTER TABLE plusnik_records
    ADD CONSTRAINT chk_revoked_has_reviewer CHECK (
        revoked_at IS NULL OR revoked_by IS NOT NULL
        );

-- Нельзя отозвать плюс в будущем
ALTER TABLE plusnik_records
    ADD CONSTRAINT chk_revoke_not_future CHECK (
        revoked_at IS NULL OR revoked_at <= NOW()
        );


-- ============================================
-- 5. ТРИГГЕР: ПРОВЕРКА ПРИНАДЛЕЖНОСТИ ЗАДАЧИ ЛИСТКУ
-- Гарантирует, что task_id принадлежит тому же листку, что и sheet_id.
-- Нельзя выразить через обычный FOREIGN KEY, поэтому используем триггер.
-- ============================================
CREATE
OR REPLACE FUNCTION check_task_belongs_to_sheet()
RETURNS TRIGGER AS $$
BEGIN
    IF
NOT EXISTS (
        SELECT 1 FROM plusnik_tasks
        WHERE task_id = NEW.task_id AND sheet_id = NEW.sheet_id
    ) THEN
        RAISE EXCEPTION 'task_id % does not belong to sheet_id %',
            NEW.task_id, NEW.sheet_id;
END IF;
RETURN NEW;
END;
$$
LANGUAGE plpgsql;

CREATE TRIGGER trigger_check_task_belongs_to_sheet
    BEFORE INSERT OR
UPDATE ON plusnik_records
    FOR EACH ROW
    EXECUTE FUNCTION check_task_belongs_to_sheet();


-- ============================================
-- 6. ТРИГГЕРЫ ДЛЯ ОБНОВЛЕНИЯ updated_at
-- ============================================
CREATE TRIGGER trigger_plusnik_sheets_updated_at
    BEFORE UPDATE
    ON plusnik_sheets
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();