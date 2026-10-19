-- ============================================================================
-- FILE: 0009_create_announcements.sql
-- PURPOSE: Announcements system with rich text (MarkdownV2), file attachments,
--          and flexible targeting (classes / groups / specific users / everyone).
-- DEPENDENCIES: [0001_create_users.sql, 0002_create_subjects_and_classes.sql]
-- MASTER-DOC: Section 2.7 (Announcements)
-- ============================================================================

-- 1. LIFECYCLE STATUS
-- draft     — creator-only, not visible to recipients
-- published — visible to targeted audience
-- archived  — hidden from active lists, history preserved
CREATE TYPE announcement_status AS ENUM ('draft', 'published', 'archived');

-- 2. PRIORITY
-- low / normal / high / urgent — affects display order and push urgency
CREATE TYPE announcement_priority AS ENUM ('low', 'normal', 'high', 'urgent');

-- 3. ANNOUNCEMENTS (aggregate root)
CREATE TABLE announcements
(
    announcement_id UUID PRIMARY KEY      DEFAULT gen_random_uuid(),
    -- Title: short, plain text (no Markdown), shown in lists
    title           VARCHAR(255) NOT NULL,
    -- Body: MarkdownV2 format (Telegram-compatible).
    -- Rendered on client; stored as-is to avoid round-trip conversion.
    content         TEXT         NOT NULL,
    -- Author (immutable after creation — audit)
    created_by      UUID         NOT NULL REFERENCES users(user_id) ON DELETE RESTRICT,
    -- Lifecycle
    status          announcement_status NOT NULL DEFAULT 'draft',
    -- Display priority (urgent first)
    priority        announcement_priority NOT NULL DEFAULT 'normal',
    -- Optional expiry: after this moment the announcement is hidden
    -- (NULL = never expires)
    expires_at      TIMESTAMPTZ NULL,
    -- Timestamps
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Fast lookup of published announcements by author (creator's dashboard)
CREATE INDEX idx_announcements_created_by
    ON announcements (created_by, created_at DESC);

-- Fast lookup of published announcements for "recent" queries
CREATE INDEX idx_announcements_published
    ON announcements (created_at DESC)
    WHERE status = 'published';

-- 4. TARGETING: CLASSES (N:M)
CREATE TABLE announcement_classes
(
    announcement_id UUID NOT NULL REFERENCES announcements(announcement_id) ON DELETE CASCADE,
    class_id        UUID NOT NULL REFERENCES classes(class_id) ON DELETE CASCADE,
    PRIMARY KEY (announcement_id, class_id)
);

-- 5. TARGETING: STUDENT GROUPS (N:M)
CREATE TABLE announcement_groups
(
    announcement_id UUID NOT NULL REFERENCES announcements(announcement_id) ON DELETE CASCADE,
    group_id        UUID NOT NULL REFERENCES student_groups(group_id) ON DELETE CASCADE,
    PRIMARY KEY (announcement_id, group_id)
);

-- 6. TARGETING: SPECIFIC USERS (N:M)
CREATE TABLE announcement_users
(
    announcement_id UUID NOT NULL REFERENCES announcements(announcement_id) ON DELETE CASCADE,
    user_id         UUID NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    PRIMARY KEY (announcement_id, user_id)
);

-- 7. FILES / PHOTOS
-- Stored in S3; DB holds metadata only.
-- ON DELETE CASCADE: deleting announcement removes its files automatically.
CREATE TABLE announcement_files
(
    file_id      UUID PRIMARY KEY      DEFAULT gen_random_uuid(),
    announcement_id UUID NOT NULL REFERENCES announcements(announcement_id) ON DELETE CASCADE,
    -- S3 storage key (e.g. "announcements/2026/10/abc123.jpg")
    storage_key  VARCHAR(500) NOT NULL,
    -- Original file name for display
    file_name    VARCHAR(255) NOT NULL,
    -- MIME type (image/jpeg, application/pdf, etc.)
    mime_type    VARCHAR(100) NOT NULL,
    -- Size in bytes
    size_bytes   BIGINT       NOT NULL CHECK (size_bytes >= 0),
    -- Display order (photos first, then documents)
    sort_order   INT          NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Fast lookup of files for a specific announcement, in display order
CREATE INDEX idx_announcement_files_announcement
    ON announcement_files (announcement_id, sort_order);

-- 8. TRIGGERS
CREATE TRIGGER trigger_announcements_updated_at
    BEFORE UPDATE ON announcements
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
