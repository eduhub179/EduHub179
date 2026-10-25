-- ============================================================================
-- FILE: 0009_create_web_sessions.sql
-- PURPOSE: Secure storage for browser cookie sessions.
-- DEPENDENCIES: [0001_create_users.sql]
-- ============================================================================

-- Session records for web auth.
-- Cookie should contain only an opaque random value; DB stores its hash.
CREATE TABLE web_sessions
(
    session_id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Owner of the session.
    user_id             UUID        NOT NULL REFERENCES users (user_id) ON DELETE CASCADE,

    -- SHA-256 (or stronger) hash of the cookie session token.
    -- Raw token must never be stored in DB.
    session_token_hash  VARCHAR(255) NOT NULL,

    -- Optional: hash of CSRF token for double-submit strategy.
    csrf_token_hash     VARCHAR(255) NULL,

    -- Security/audit metadata.
    ip_address          INET NULL,
    user_agent          TEXT NULL,

    -- Session lifecycle.
    is_persistent       BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at          TIMESTAMPTZ NOT NULL,
    revoked_at          TIMESTAMPTZ NULL,
    revoke_reason       VARCHAR(255) NULL,

    -- Basic sanity checks.
    CONSTRAINT chk_web_sessions_token_hash_not_blank
        CHECK (length(trim(session_token_hash)) > 0),
    CONSTRAINT chk_web_sessions_csrf_hash_not_blank
        CHECK (csrf_token_hash IS NULL OR length(trim(csrf_token_hash)) > 0),
    CONSTRAINT chk_web_sessions_expiration_after_creation
        CHECK (expires_at > created_at),
    CONSTRAINT chk_web_sessions_revocation_after_creation
        CHECK (revoked_at IS NULL OR revoked_at >= created_at)
);

-- One token hash = one active session identity.
CREATE UNIQUE INDEX idx_web_sessions_token_hash_unique
    ON web_sessions (session_token_hash);

-- Fast lookup for active sessions of one user (e.g., "logout from all devices").
CREATE INDEX idx_web_sessions_user_active
    ON web_sessions (user_id, expires_at DESC)
    WHERE revoked_at IS NULL;

-- Fast cleanup scan for expired/revoked sessions.
CREATE INDEX idx_web_sessions_expiration_cleanup
    ON web_sessions (expires_at, revoked_at);

-- Optional read path when validating CSRF-bound session flows.
CREATE INDEX idx_web_sessions_csrf_hash
    ON web_sessions (csrf_token_hash)
    WHERE csrf_token_hash IS NOT NULL;

CREATE TRIGGER trigger_web_sessions_updated_at
    BEFORE UPDATE
    ON web_sessions
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
