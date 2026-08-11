-- 0001_create_users.sql

-- ============================================================================
-- FILE: 0001_create_users.sql
-- PURPOSE: Create the base users and roles tables.
-- DEPENDENCIES: None.
-- MASTER DOCUMENT: Section 2.1
-- ============================================================================


-- roles in MVP
CREATE TYPE user_role AS ENUM ('student', 'teacher', 'admin');

-- Users table
CREATE TABLE users
(
    user_id       UUID PRIMARY KEY             DEFAULT gen_random_uuid(),

    -- Authentication
    email         VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NULL,

    -- Role
    role          user_role           NOT NULL,

    -- Name (split for search and sorting)
    last_name     VARCHAR(100)        NOT NULL,
    first_name    VARCHAR(100)        NOT NULL,
    middle_name   VARCHAR(100) NULL,

    -- State
    is_active     BOOLEAN             NOT NULL DEFAULT TRUE,

    -- Timestamps
    created_at    TIMESTAMPTZ         NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ         NOT NULL DEFAULT NOW()
);

-- INDEXES

-- 1. For teachers: search students in a specific class by last name TODO: currently only by last name, fix!
CREATE INDEX idx_users_class_last_name ON users (last_name) WHERE role = 'student' AND is_active = TRUE;
-- 2. Filter active users by role
CREATE INDEX idx_users_role_active ON users (role, is_active) WHERE is_active = TRUE;


-- Trigger for updated_at
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
