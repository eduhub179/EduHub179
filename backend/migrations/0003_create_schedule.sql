-- 0003_create_schedule.sql


-- ============================================
-- 1. ДНИ НЕДЕЛИ
-- ============================================
CREATE TYPE day_of_week AS ENUM ('пн', 'вт', 'ср', 'чт', 'пт', 'сб');


-- ============================================
-- 2. ПЕРИОДИЧНОСТЬ УРОКОВ
-- every — каждую неделю
-- odd   — только по нечётным неделям
-- even  — только по чётным неделям
-- ============================================
CREATE TYPE week_parity AS ENUM ('every', 'odd', 'even');


-- ============================================
-- 3. КАБИНЕТЫ
-- Кабинет — это 3-значное число (этаж + номер на этаже).
-- Храним как отдельную сущность для защиты от опечаток
-- и добавления метаданных (оборудование, вместимость).
-- ============================================
CREATE TABLE cabinets
(
    cabinet_id  UUID PRIMARY KEY     DEFAULT gen_random_uuid(),

    -- Номер кабинета (3-значное число)
    number      INT         NOT NULL UNIQUE CHECK (number BETWEEN 100 AND 999),

    -- Этаж (вычисляется из number)
    floor       INT         NOT NULL GENERATED ALWAYS AS (number / 100) STORED,

    -- Описание (опционально): "Химическая лаборатория", "Компьютерный класс"
    description VARCHAR(255) NULL,

    -- Вместимость (опционально)
    capacity    INT NULL CHECK (capacity > 0),

    -- Временные метки
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Быстрый поиск кабинетов на конкретном этаже
CREATE INDEX idx_cabinets_floor ON cabinets (floor);


-- ============================================
-- 4. ШАБЛОНЫ УРОКОВ
-- Шаблон урока — это "предмет-время-кабинет", который действителен всегда.
-- Создаётся один раз, используется в расписании.
-- Изменение шаблона автоматически обновляет все слоты расписания.
--
-- Флаги:
-- - is_active: TRUE — используется в текущем расписании, участвует в проверке занятости
--              FALSE — архивирован, не участвует в проверке
-- - is_override: TRUE — шаблон для замены (учитель заболел, замена кабинета)
--                FALSE — обычный шаблон
-- ============================================
CREATE TABLE lesson_templates
(
    template_id UUID PRIMARY KEY     DEFAULT gen_random_uuid(),

    -- Урок (класс/группа + предмет + учителя)
    lesson_id   UUID        NOT NULL REFERENCES lessons (lesson_id) ON DELETE RESTRICT,

    -- Когда проходит урок
    day         day_of_week NOT NULL,
    start_time  TIME        NOT NULL,
    end_time    TIME        NOT NULL,

    -- Периодичность (каждую неделю / чётные / нечётные)
    parity      week_parity NOT NULL DEFAULT 'every',

    -- Где проходит урок
    cabinet_id  UUID NULL REFERENCES cabinets(cabinet_id) ON DELETE SET NULL,

    -- Активен ли шаблон (используется в текущем расписании)
    is_active   BOOLEAN     NOT NULL DEFAULT TRUE,

    -- Это замена (TRUE) или обычный шаблон (FALSE)
    is_override BOOLEAN     NOT NULL DEFAULT FALSE,

    -- Комментарий (для замен: "Иванов заболел, замена Петровым")
    comment     TEXT NULL,

    -- Временные метки
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Конец должен быть после начала
    CONSTRAINT chk_template_time CHECK (end_time > start_time)
);

-- Один и тот же урок не может иметь два шаблона с одинаковыми параметрами
CREATE UNIQUE INDEX idx_lesson_templates_no_dup
    ON lesson_templates (lesson_id, day, start_time, end_time, parity);

-- Быстрый поиск активных шаблонов конкретного урока
-- Используется при проверке занятости учителя
CREATE INDEX idx_lesson_templates_lesson_active
    ON lesson_templates (lesson_id, day, start_time, end_time) WHERE is_active = TRUE;

-- Быстрый поиск активных шаблонов в конкретный день
-- Используется при составлении расписания
CREATE INDEX idx_lesson_templates_day_active
    ON lesson_templates (day, start_time, end_time) WHERE is_active = TRUE;

-- Быстрый поиск всех шаблонов конкретного урока (включая архивные)
CREATE INDEX idx_lesson_templates_lesson
    ON lesson_templates (lesson_id);


-- ============================================
-- 5. РАСПИСАНИЕ УРОКОВ (НА КОНКРЕТНЫЕ НЕДЕЛИ)
-- Каждая запись — это слот расписания на конкретную неделю.
-- Ссылается на lesson_template (шаблон урока).
-- ============================================
CREATE TABLE schedule_slots
(
    slot_id         UUID PRIMARY KEY     DEFAULT gen_random_uuid(),

    -- Шаблон урока (может быть обычным или заменой)
    template_id     UUID        NOT NULL REFERENCES lesson_templates (template_id) ON DELETE RESTRICT,

    -- Начало недели, к которой относится этот слот
    week_start_date DATE        NOT NULL,

    -- Статус слота
    status          VARCHAR(20) NOT NULL DEFAULT 'scheduled'
        CHECK (status IN ('scheduled', 'cancelled')),

    -- Временные метки
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Один шаблон не может быть дважды в одну неделю
CREATE UNIQUE INDEX idx_schedule_slots_no_dup
    ON schedule_slots (template_id, week_start_date);

-- Быстрый поиск слотов на конкретную неделю
CREATE INDEX idx_schedule_slots_week
    ON schedule_slots (week_start_date);

-- Быстрый поиск слотов конкретного шаблона
CREATE INDEX idx_schedule_slots_template
    ON schedule_slots (template_id);


-- ============================================
-- 6. КОНКРЕТНЫЕ УРОКИ (НА КОНКРЕТНЫЕ ДАТЫ)
-- lesson_instance — это конкретный урок на конкретную дату.
-- ДЗ привязывается к lesson_instance, а не к lesson.
-- Это позволяет:
-- - Иметь разные ДЗ для разных уроков одного предмета
-- - Архивировать ДЗ после урока
-- - Сохранить историю
-- ============================================
CREATE TABLE lesson_instances
(
    instance_id UUID PRIMARY KEY     DEFAULT gen_random_uuid(),

    -- Слот расписания (ссылка на template + дату недели)
    slot_id     UUID        NOT NULL REFERENCES schedule_slots (slot_id) ON DELETE RESTRICT,

    -- Дата урока (вычисляется из week_start_date + day, но храним для удобства)
    lesson_date DATE        NOT NULL,

    -- Статус урока
    status      VARCHAR(20) NOT NULL DEFAULT 'scheduled'
        CHECK (status IN ('scheduled', 'completed', 'cancelled')),

    -- Временные метки
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Нельзя создать два урока на одну дату для одного слота
CREATE UNIQUE INDEX idx_lesson_instances_unique
    ON lesson_instances (slot_id, lesson_date);

-- Быстрый поиск уроков на конкретную дату
CREATE INDEX idx_lesson_instances_date
    ON lesson_instances (lesson_date);

-- Быстрый поиск уроков конкретного слота
CREATE INDEX idx_lesson_instances_slot
    ON lesson_instances (slot_id);


-- ============================================
-- 7. СОБЫТИЯ (ЛЕКЦИИ, МЕРОПРИЯТИЯ, ФАКУЛЬТАТИВЫ)
-- Событие — это разовое или периодическое мероприятие,
-- которое не является уроком, но занимает время учеников.
-- В отличие от групп, события не меняют структуру классов.
-- ============================================
CREATE TABLE events
(
    event_id     UUID PRIMARY KEY      DEFAULT gen_random_uuid(),

    -- Название события
    title        VARCHAR(255) NOT NULL,

    -- Описание (опционально)
    description  TEXT NULL,

    -- Время начала и окончания (конкретная дата + время)
    start_time   TIMESTAMPTZ  NOT NULL,
    end_time     TIMESTAMPTZ  NOT NULL,

    -- Где проходит (опционально)
    cabinet_id   UUID NULL REFERENCES cabinets(cabinet_id) ON DELETE SET NULL,

    -- Кто организует/ведёт
    organizer_id UUID         NOT NULL REFERENCES users (user_id) ON DELETE RESTRICT,

    -- Конец должен быть после начала
    CONSTRAINT chk_event_time CHECK (end_time > start_time),

    -- Временные метки
    created_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Быстрый поиск событий на конкретную дату
CREATE INDEX idx_events_date
    ON events (start_time);

-- Быстрый поиск событий конкретного организатора
CREATE INDEX idx_events_organizer
    ON events (organizer_id);


-- ============================================
-- 8. УЧАСТНИКИ СОБЫТИЙ
-- Связь ученик ↔ событие. Ученик может участвовать
-- в нескольких событиях, событие может включать
-- учеников из разных классов.
-- ============================================
CREATE TABLE event_attendees
(
    attendee_id UUID PRIMARY KEY     DEFAULT gen_random_uuid(),
    event_id    UUID        NOT NULL REFERENCES events (event_id) ON DELETE CASCADE,
    student_id  UUID        NOT NULL REFERENCES users (user_id) ON DELETE CASCADE,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Ученик не может быть дважды в одном событии
CREATE UNIQUE INDEX idx_event_attendees_unique
    ON event_attendees (event_id, student_id);

-- Быстрый поиск всех событий ученика
-- Используется при отображении расписания ученика
CREATE INDEX idx_event_attendees_student
    ON event_attendees (student_id);

-- Быстрый поиск всех участников события
CREATE INDEX idx_event_attendees_event
    ON event_attendees (event_id);


-- ============================================
-- 9. ФУНКЦИЯ: ПРОВЕРКА ЗАНЯТОСТИ УЧИТЕЛЯ
-- Проверяет, занят ли учитель в указанное время.
-- Работает только с активными шаблонами (is_active = TRUE).
-- Используется админом при составлении расписания.
-- ============================================
CREATE
OR REPLACE FUNCTION check_teacher_available(
    p_teacher_id UUID,
    p_day day_of_week,
    p_start_time TIME,
    p_end_time TIME,
    p_exclude_template_id UUID DEFAULT NULL
) RETURNS BOOLEAN AS $$
BEGIN
RETURN NOT EXISTS (SELECT 1
                   FROM lesson_templates lt
                            JOIN lessons l ON lt.lesson_id = l.lesson_id
                            JOIN lesson_teachers lte ON lte.lesson_id = l.lesson_id
                   WHERE lte.teacher_id = p_teacher_id
                     AND lt.is_active = TRUE
                     AND lt.day = p_day
                     AND lt.start_time < p_end_time
                     AND lt.end_time > p_start_time
                     AND lt.template_id != COALESCE(p_exclude_template_id, '00000000-0000-0000-0000-000000000000'::UUID));
END;
$$
LANGUAGE plpgsql;


-- ============================================
-- 10. ФУНКЦИЯ: ПОЛНОЕ РАСПИСАНИЕ УЧЕНИКА НА ДАТУ
-- Возвращает все уроки и события ученика на конкретную дату,
-- учитывая, что события "перекрывают" уроки.
-- ============================================
CREATE
OR REPLACE FUNCTION get_student_schedule_for_date(
    p_student_id UUID,
    p_date DATE
) RETURNS TABLE (
    start_time TIME,
    end_time TIME,
    title VARCHAR(255),
    is_event BOOLEAN,
    cabinet_id UUID
) AS $$
BEGIN
RETURN QUERY
-- События (имеют приоритет)
SELECT e.start_time::TIME, e.end_time::TIME, e.title,
       TRUE AS is_event,
       e.cabinet_id
FROM events e
         JOIN event_attendees ea ON ea.event_id = e.event_id
WHERE ea.student_id = p_student_id
  AND e.start_time::DATE = p_date

UNION ALL

-- Уроки (только если не перекрываются событиями)
SELECT lt.start_time,
       lt.end_time,
       s.name AS title,
       FALSE  AS is_event,
       lt.cabinet_id
FROM lesson_instances li
         JOIN schedule_slots ss ON li.slot_id = ss.slot_id
         JOIN lesson_templates lt ON ss.template_id = lt.template_id
         JOIN lessons l ON lt.lesson_id = l.lesson_id
         JOIN subjects s ON l.subject_id = s.subject_id
WHERE li.lesson_date = p_date
  AND li.status = 'scheduled'
  AND (
    -- Урок для класса ученика
    l.class_id = (SELECT class_id FROM users WHERE user_id = p_student_id)
        OR
        -- Урок для группы, в которой состоит ученик
    l.group_id IN (SELECT gm.group_id FROM group_members gm WHERE gm.student_id = p_student_id)
    )
  -- Исключаем уроки, которые перекрываются событиями
  AND NOT EXISTS (SELECT 1
                  FROM events e
                           JOIN event_attendees ea ON ea.event_id = e.event_id
                  WHERE ea.student_id = p_student_id
                    AND e.start_time::DATE = p_date
    AND e.start_time::TIME < lt.end_time
    AND e.end_time::TIME > lt.start_time);
END;
$$
LANGUAGE plpgsql;


-- ============================================
-- 11. ТРИГГЕРЫ ДЛЯ ОБНОВЛЕНИЯ updated_at
-- ============================================
CREATE TRIGGER trigger_lesson_templates_updated_at
    BEFORE UPDATE
    ON lesson_templates
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER trigger_schedule_slots_updated_at
    BEFORE UPDATE
    ON schedule_slots
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER trigger_lesson_instances_updated_at
    BEFORE UPDATE
    ON lesson_instances
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER trigger_events_updated_at
    BEFORE UPDATE
    ON events
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();