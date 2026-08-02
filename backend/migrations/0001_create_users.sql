-- 0001_create_users.sql

-- ============================================================================
-- ФАЙЛ: 0001_create_users.sql
-- НАЗНАЧЕНИЕ: Создание базовой таблицы пользователей и ролей.
-- ЗАВИСИМОСТИ: Нет.
-- МАСТЕР-ДОКУМЕНТ: Раздел 2.1
-- ============================================================================


-- роли в MVP
CREATE TYPE user_role AS ENUM ('student', 'teacher', 'admin');

-- Таблица пользователей
CREATE TABLE users
(
    user_id       UUID PRIMARY KEY             DEFAULT gen_random_uuid(),

    -- Аутентификация
    email         VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NULL,

    -- Роль
    role          user_role           NOT NULL,

    -- Имя (разделение для поиска и сортировки)
    last_name     VARCHAR(100)        NOT NULL,
    first_name    VARCHAR(100)        NOT NULL,
    middle_name   VARCHAR(100) NULL,

    -- Состояние
    is_active     BOOLEAN             NOT NULL DEFAULT TRUE,

    -- Временные метки
    created_at    TIMESTAMPTZ         NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ         NOT NULL DEFAULT NOW()
);

-- ИНДЕКСЫ

-- 1. Для учителя: поиск учеников в конкретном классе по фамилии TODO: пока только по фамилии исправить!
CREATE INDEX idx_users_class_last_name ON users (last_name) WHERE role = 'student' AND is_active = TRUE;
-- 2. Фильтрация активных пользователей по роли
CREATE INDEX idx_users_role_active ON users (role, is_active) WHERE is_active = TRUE;


-- Триггер для updated_at
CREATE
OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_users_updated_at
    BEFORE UPDATE
    ON users
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
