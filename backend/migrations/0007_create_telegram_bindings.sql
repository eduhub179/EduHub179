-- ============================================================================
-- FILE: 0007_create_telegram_bindings.sql
-- PURPOSE: Mapping between Telegram chat_id and user_id for bot authorization.
-- DEPENDENCIES: [0001_create_users.sql]
-- MASTER-DOC: Section 6.7
-- ============================================================================

CREATE TABLE telegram_bindings
(
    -- Telegram chat ID (i64). One Telegram account = one row.
    -- PRIMARY KEY, not UNIQUE, because one TG account cannot be bound
    -- to two users simultaneously.
    telegram_id   BIGINT      PRIMARY KEY,
    -- Reference to the school user.
    -- ON DELETE CASCADE: when the user is deleted, the binding disappears automatically.
    user_id       UUID        NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    -- Binding status:
    --   'pending'  — user entered email, waiting for code confirmation;
    --   'active'   — code confirmed, bot can serve the user.
    status        VARCHAR(20) NOT NULL DEFAULT 'pending'
                    CHECK (status IN ('pending', 'active')),
    -- Timestamps
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- INDEX: idx_telegram_bindings_user
-- Purpose: Speeds up reverse lookup — "which TG accounts are bound to this user?"
--          Needed for account unbinding and notifications (push to TG on new homework).
CREATE INDEX idx_telegram_bindings_user ON telegram_bindings (user_id);

-- INDEX: idx_telegram_bindings_active
-- Purpose: Speeds up the bot's main hot-path — "who is writing to me?" (telegram_id → user_id).
--          Partial: only active bindings, because pending records live for
--          minutes and do not participate in message processing.
CREATE INDEX idx_telegram_bindings_active
ON telegram_bindings (telegram_id)
WHERE status = 'active';

CREATE TRIGGER trigger_telegram_bindings_updated_at
BEFORE UPDATE ON telegram_bindings
FOR EACH ROW
EXECUTE FUNCTION update_updated_at_column();
