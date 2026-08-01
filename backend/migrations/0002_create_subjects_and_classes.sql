-- 0002_create_subjects_and_classes.sql

-- ============================================
-- 1. НАЗВАНИЯ КЛАССОВ (БУКВЫ)
-- ============================================
CREATE TYPE class_letter AS ENUM ('б', 'в', 'и'); --TODO: реальные названия


-- ============================================
-- 2. КЛАССЫ ШКОЛЫ
-- Номер класса вычисляется аналитически из graduation_year и текущей даты.
-- ============================================
CREATE TABLE classes
(
    class_id        UUID PRIMARY KEY      DEFAULT gen_random_uuid(),

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

-- Быстрый поиск по году выпуска
CREATE INDEX idx_classes_graduation_year ON classes (graduation_year) WHERE is_active = TRUE;


-- ============================================
-- 3. ПРЕДМЕТЫ
-- ============================================
CREATE TABLE subjects
(
    subject_id UUID PRIMARY KEY     DEFAULT gen_random_uuid(),

    -- Название предмета: "Алгебра", "Спецмат", "Информатика"
    name       VARCHAR(100) NOT NULL,

    -- Временные метки
    created_at TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Нельзя создать два предмета с одинаковым названием
CREATE UNIQUE INDEX idx_subjects_name ON subjects (name);


-- ============================================
-- 4. ГРУППЫ УЧЕНИКОВ
-- Группа — это произвольное подмножество учеников школы.
-- Группа НЕ привязана к одному классу: она может объединять учеников
-- из разных классов (например, "Английский B1" — ученики из 10а, 10б, 10в).
-- ============================================
CREATE TABLE student_groups
(
    group_id   UUID PRIMARY KEY     DEFAULT gen_random_uuid(),

    -- Уникальное название группы: "Английский B1", "Информатика базовая"
    name       VARCHAR(100) NOT NULL,

    -- Временные метки
    created_at TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Нельзя создать две группы с одинаковым названием
CREATE UNIQUE INDEX idx_student_groups_name ON student_groups (name);


-- ============================================
-- 5. СОСТАВ ГРУПП
-- Связь "ученик ↔ группа". Ученик может состоять в нескольких группах
-- одновременно (наприм
-- ============================================
CREATE TABLE group_members
(
    member_id  UUID PRIMARY KEY     DEFAULT gen_random_uuid(),

    -- Ученик и группа
    student_id UUID         NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    group_id   UUID         NOT NULL REFERENCES student_groups(group_id) ON DELETE CASCADE,

    -- Временные метки
    created_at TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Ученик не может быть в одной группе дважды
CREATE UNIQUE INDEX idx_group_members_unique
    ON group_members (student_id, group_id);

-- Быстрый поиск всех групп, в которых состоит ученик (для расписания)
CREATE INDEX idx_group_members_student
    ON group_members (student_id);

-- Быстрый поиск всех учеников группы (для учителя при выставлении плюсов)
CREATE INDEX idx_group_members_group
    ON group_members (group_id);


-- ============================================
-- 6. УРОКИ (класс/группа + предмет)
-- Урок — это связка (класс ИЛИ группа) + предмет.
-- Учителя, ведущие урок, хранятся в отдельной таблице lesson_teachers.
-- Это позволяет нескольким учителям вести один урок, при этом ДЗ, плюсы,
-- расписание привязываются к уроку, а не к конкретному учителю.
-- ============================================
CREATE TABLE lessons
(
    lesson_id  UUID PRIMARY KEY     DEFAULT gen_random_uuid(),

    -- Ровно одно из двух: класс ИЛИ группа
    -- Если class_id заполнен — урок для всего класса
    -- Если group_id заполнен — урок для группы учеников (может быть из разных классов)
    class_id   UUID         NULL REFERENCES classes(class_id) ON DELETE RESTRICT,
    group_id   UUID         NULL REFERENCES student_groups(group_id) ON DELETE RESTRICT,

    -- Предмет, который изучается на этом уроке
    subject_id UUID         NOT NULL REFERENCES subjects(subject_id) ON DELETE RESTRICT,

    -- Активен ли урок (можно деактивировать без удаления истории ДЗ/плюсов)
    is_active  BOOLEAN      NOT NULL DEFAULT TRUE,

    -- Временные метки
    created_at TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ  NOT NULL DEFAULT NOW(),

    -- Ровно одно из class_id / group_id заполнено
    CONSTRAINT chk_one_entity CHECK (
        (class_id IS NOT NULL AND group_id IS NULL)
            OR
        (class_id IS NULL AND group_id IS NOT NULL)
        )
);

-- Частичный уникальный индекс для уроков класса
-- Гарантирует, что в одном классе не будет двух уроков одного предмета
CREATE UNIQUE INDEX idx_lessons_class_unique
    ON lessons (class_id, subject_id)
    WHERE class_id IS NOT NULL;

-- Частичный уникальный индекс для уроков группы
-- Гарантирует, что в одной группе не будет двух уроков одного предмета
CREATE UNIQUE INDEX idx_lessons_group_unique
    ON lessons (group_id, subject_id)
    WHERE group_id IS NOT NULL;

-- Быстрый поиск всех уроков конкретного класса
CREATE INDEX idx_lessons_class
    ON lessons (class_id) WHERE is_active = TRUE AND class_id IS NOT NULL;

-- Быстрый поиск всех уроков конкретной группы
CREATE INDEX idx_lessons_group
    ON lessons (group_id) WHERE is_active = TRUE AND group_id IS NOT NULL;


-- ============================================
-- 7. УЧИТЕЛЯ УРОКОВ
-- Связь многие-ко-многим между уроками и учителями.
-- Один урок могут вести несколько учителей (например, Спецмат в 10б ведут 4 учителя).
-- Один учитель может вести несколько уроков (например, Иванов ведёт Спецмат в 10б и 11б).
-- ============================================
CREATE TABLE lesson_teachers
(
    -- Ссылка на урок
    lesson_id  UUID NOT NULL REFERENCES lessons(lesson_id) ON DELETE CASCADE,

    -- Ссылка на учителя
    teacher_id UUID NOT NULL REFERENCES users(user_id) ON DELETE RESTRICT,

    -- Временные метки
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Составной первичный ключ: один учитель не может быть привязан к одному уроку дважды
    PRIMARY KEY (lesson_id, teacher_id)
);

-- Быстрый поиск всех уроков конкретного учителя
-- Используется, когда учитель открывает список своих предметов/классов
CREATE INDEX idx_lesson_teachers_teacher
    ON lesson_teachers (teacher_id);


-- ============================================
-- 8. ПРИВЯЗКА УЧЕНИКОВ К КЛАССАМ
-- Ученик принадлежит к одному классу (основной класс).
-- Дополнительно ученик может состоять в группах (таблица group_members).
-- ============================================
ALTER TABLE users
    ADD COLUMN class_id UUID NULL REFERENCES classes(class_id) ON DELETE SET NULL;

-- Быстрый поиск учеников в конкретном классе
-- Используется, когда учитель ищет ученика в своём классе для выставления плюсов
CREATE INDEX idx_users_class
    ON users (class_id)
    WHERE role = 'student' AND is_active = TRUE;

-- Быстрый поиск всех учеников школы по фамилии
-- Используется, когда админ ищет ученика по фамилии во всей школе
CREATE INDEX idx_users_students
    ON users (last_name, first_name)
    WHERE role = 'student' AND is_active = TRUE;


-- ============================================
-- 9. ТРИГГЕРЫ ДЛЯ ОБНОВЛЕНИЯ updated_at
-- ============================================
CREATE TRIGGER trigger_classes_updated_at
    BEFORE UPDATE ON classes
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER trigger_student_groups_updated_at
    BEFORE UPDATE ON student_groups
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER trigger_lessons_updated_at
    BEFORE UPDATE ON lessons
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();