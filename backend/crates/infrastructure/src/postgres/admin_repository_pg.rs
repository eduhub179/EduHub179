//! PostgreSQL implementation of `AdminRepository` (read-only lists for the admin panel).

use chrono::{DateTime, Utc};
use domain::entities::announcement::Announcement;
use domain::entities::user::User;
use domain::errors::DomainError;
use domain::repositories::admin_repository::{
    AdminAnnouncementRow, AdminRepository, AdminUserRow, UserFilter,
};
use domain::value_objects::announcement_priority::AnnouncementPriority;
use domain::value_objects::announcement_status::AnnouncementStatus;
use domain::value_objects::role::UserRole;
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
struct UserListRow {
    user_id: Uuid,
    login: String,
    role: String,
    last_name: String,
    first_name: String,
    middle_name: Option<String>,
    is_active: bool,
    class_id: Option<Uuid>,
    class_name: Option<String>,
    telegram_bound: bool,
}

impl UserListRow {
    fn into_domain(self) -> Result<AdminUserRow, DomainError> {
        let role = UserRole::from_str(&self.role).map_err(|_| DomainError::InternalError)?;
        let mut user = User::try_new(
            self.user_id,
            self.login,
            role,
            self.last_name,
            self.first_name,
            self.middle_name,
            self.class_id,
        )?;
        user.is_active = self.is_active;
        Ok(AdminUserRow {
            user,
            class_name: self.class_name,
            telegram_bound: self.telegram_bound,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct AnnouncementListRow {
    announcement_id: Uuid,
    title: Option<String>,
    content: Option<String>,
    created_by: Uuid,
    status: String,
    priority: String,
    expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    class_ids: Vec<Uuid>,
    group_ids: Vec<Uuid>,
    user_ids: Vec<Uuid>,
    author_name: String,
}

impl AnnouncementListRow {
    fn into_domain(self) -> Result<AdminAnnouncementRow, DomainError> {
        let announcement = Announcement::try_new(
            self.announcement_id,
            self.title,
            self.content,
            self.created_by,
            AnnouncementStatus::from_str(&self.status)?,
            AnnouncementPriority::from_str(&self.priority)?,
            self.expires_at,
            self.created_at,
        )?;
        Ok(AdminAnnouncementRow {
            announcement,
            class_ids: self.class_ids,
            group_ids: self.group_ids,
            user_ids: self.user_ids,
            author_name: self.author_name,
        })
    }
}

pub struct AdminRepositoryPg {
    pool: PgPool,
}

impl AdminRepositoryPg {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl AdminRepository for AdminRepositoryPg {
    async fn list_users(&self, filter: &UserFilter, limit: i64) -> Result<Vec<AdminUserRow>, DomainError> {
        // LIKE wildcards in the search text are matched literally.
        let pattern = filter.query.as_deref().map(|q| {
            let escaped = q.trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
            format!("%{escaped}%")
        });
        let rows = sqlx::query_as::<_, UserListRow>(
            r#"
            SELECT u.user_id, u.login, u.role::TEXT AS role, u.last_name, u.first_name,
                   u.middle_name, u.is_active, u.class_id,
                   c.graduation_year::TEXT || c.letter::TEXT AS class_name,
                   EXISTS (SELECT 1 FROM telegram_bindings tb
                           WHERE tb.user_id = u.user_id AND tb.status = 'active') AS telegram_bound
            FROM users u
            LEFT JOIN classes c ON c.class_id = u.class_id
            WHERE ($1::TEXT IS NULL
                   OR u.login ILIKE $1
                   OR (u.last_name || ' ' || u.first_name || ' ' || COALESCE(u.middle_name, '')) ILIKE $1)
              AND ($2::TEXT IS NULL OR u.role = $2::user_role)
              AND ($3::UUID IS NULL OR u.class_id = $3)
              AND ($4 OR u.is_active)
            ORDER BY u.role, u.last_name, u.first_name
            LIMIT $5
            "#,
        )
        .bind(pattern)
        .bind(filter.role.map(|role| role.to_string()))
        .bind(filter.class_id)
        .bind(filter.include_inactive)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| DomainError::InternalError)?;
        rows.into_iter().map(UserListRow::into_domain).collect()
    }

    async fn list_announcements(&self) -> Result<Vec<AdminAnnouncementRow>, DomainError> {
        let rows = sqlx::query_as::<_, AnnouncementListRow>(
            r#"
            SELECT a.announcement_id, a.title, a.content, a.created_by,
                   a.status::TEXT AS status, a.priority::TEXT AS priority,
                   a.expires_at, a.created_at,
                   ARRAY(SELECT class_id FROM announcement_classes ac
                         WHERE ac.announcement_id = a.announcement_id) AS class_ids,
                   ARRAY(SELECT group_id FROM announcement_groups ag
                         WHERE ag.announcement_id = a.announcement_id) AS group_ids,
                   ARRAY(SELECT user_id FROM announcement_users au
                         WHERE au.announcement_id = a.announcement_id) AS user_ids,
                   u.last_name || ' ' || u.first_name AS author_name
            FROM announcements a
            JOIN users u ON u.user_id = a.created_by
            ORDER BY a.created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|_| DomainError::InternalError)?;
        rows.into_iter().map(AnnouncementListRow::into_domain).collect()
    }
}
