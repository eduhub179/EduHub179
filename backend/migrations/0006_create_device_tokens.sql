-- 0006_create_device_tokens.sql

-- ============================================
-- 1. DEVICE TOKEN PLATFORM
-- Used to distinguish which push provider a device belongs to.
-- ============================================
CREATE TYPE notification_platform AS ENUM ('android', 'ios', 'web');


-- ============================================
-- 2. DEVICE TOKENS
-- Stores one push token per physical device of a user.
--
-- A user may have multiple devices (phone, tablet, browser), so the same
-- user_id can appear many times with different tokens.
--
-- The token itself is a provider-issued device identifier (FCM/APNs/Web Push).
-- The application layer is responsible for re-registration and invalidation.
-- ============================================
CREATE TABLE device_tokens
(
    device_token_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- User who owns this device.
    user_id         UUID                 NOT NULL REFERENCES users (user_id) ON DELETE CASCADE,

    -- Platform for the receiving device.
    platform        notification_platform NOT NULL,

    -- Provider-issued push token.
    -- Keep it as a plain string in the DB for compatibility with FCM/APNs,
    -- but make sure the application does not log it and the DB is protected.
    token           VARCHAR(1024)        NOT NULL,

    -- Active tokens are eligible for sending push notifications.
    is_active       BOOLEAN              NOT NULL DEFAULT TRUE,

    -- Last time the app used this token to receive a notification.
    last_used_at    TIMESTAMPTZ         NULL,

    -- Timestamps
    created_at      TIMESTAMPTZ         NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ         NOT NULL DEFAULT NOW()
);

-- One user cannot register the same token twice for the same platform.
CREATE UNIQUE INDEX idx_device_tokens_user_platform_token_unique
    ON device_tokens (user_id, platform, token);

-- Fast lookup of active tokens for one user.
-- Used when sending push notifications after a homework/event is created.
CREATE INDEX idx_device_tokens_user_active
    ON device_tokens (user_id, platform) WHERE is_active = TRUE;

-- Fast lookup of all active tokens by platform for provider fan-out.
CREATE INDEX idx_device_tokens_platform_active
    ON device_tokens (platform, user_id) WHERE is_active = TRUE;

-- Prevent empty tokens from entering the system.
ALTER TABLE device_tokens
    ADD CONSTRAINT chk_device_token_not_blank CHECK (length(trim(token)) > 0);


-- ============================================
-- 3. TRIGGERS FOR UPDATING updated_at
-- ============================================
CREATE TRIGGER trigger_device_tokens_updated_at
    BEFORE UPDATE
    ON device_tokens
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
