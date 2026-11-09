//! Redis-реализация порта `AuthCodeStore`.
//!
//! Модель хранения:
//! - Ключ: `auth_code:{user_id}` — один активный код на пользователя по построению
//!   (новый `SET` перезаписывает старый).
//! - Значение: Argon2id-хэш кода — сырой код в Redis не попадает (тот же
//!   `PasswordHasher`, что использует `CredentialsStorePg`).
//! - TTL: `SET ... EX` — истечением управляет движок, cleanup-джоб не нужен.
//!
//! Потребление: GET хэша → verify в Rust → DEL при успехе.
//! NOTE (осознанный компромисс): в отличие от PG-варианта (транзакция +
//! FOR UPDATE), две ОДНОВРЕМЕННЫЕ проверки с верным кодом теоретически могут
//! обе пройти до того, как ляжет DEL. Для email-кода, который вводит один
//! пользователь, окно пренебрежимо; replay ПОСЛЕ потребления невозможен.
//!
//! Перебор: после `MAX_WRONG_ATTEMPTS` неверных вводов код сгорает (счётчик
//! `{prefix}_attempts:{user_id}` живёт не дольше кода) — 6 цифр нельзя перебрать
//! за 10 минут TTL. Повторная отправка ограничена `try_acquire_send_slot`
//! (ключ `{prefix}_sent:{user_id}`, `SET NX EX`).
//!
//! Зависимости: крейт `redis`, `domain` (ports, errors), внутренний `PasswordHasher`.
//! Гарантии: все методы возвращают `Result`; паники недопустимы.
use domain::errors::DomainError;
use domain::ports::auth::{AuthCodeStore, CodeCheck};
// Префикс `::` обязателен: у крейта есть собственный модуль `redis`
// (uniform paths в edition 2018 иначе дали бы ambiguity-ошибку).
use ::redis::aio::ConnectionManager;
use ::redis::AsyncCommands;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::password_hasher::PasswordHasher;

/// Хранилище одноразовых кодов входа на Redis.
///
/// `ConnectionManager` держит реконнектящееся соединение и дёшево клонируется
/// на каждый вызов — пул нам не нужен, трафик кодов низкочастотный.
/// Wrong entries after which the code is burned and a new one must be requested.
pub const MAX_WRONG_ATTEMPTS: i64 = 5;

pub struct AuthCodeStoreRedis {
    conn: ConnectionManager,
    /// Key prefix: `auth_code` for login codes, another one for other one-time secrets
    /// (e.g. the password-setup token), so they never overwrite each other.
    prefix: String,
    /// Инжектируется, как в `CredentialsStorePg`: хэширование — забота инфраструктуры.
    hasher: Arc<dyn PasswordHasher>,
}

impl AuthCodeStoreRedis {
    /// Создаёт хранилище. `url` — из `REDIS_URL` (например, `redis://127.0.0.1:6379`).
    ///
    /// Fail-safe: ошибка подключения → `Err(InternalError)`, без паник.
    pub async fn new(url: &str, hasher: Arc<dyn PasswordHasher>) -> Result<Self, DomainError> {
        Self::with_prefix(url, hasher, "auth_code").await
    }

    /// The same store under another key prefix (`[a-z_]+`).
    pub async fn with_prefix(
        url: &str,
        hasher: Arc<dyn PasswordHasher>,
        prefix: &str,
    ) -> Result<Self, DomainError> {
        if prefix.is_empty() || !prefix.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
            return Err(DomainError::InternalError);
        }
        let client = ::redis::Client::open(url).map_err(|_| DomainError::InternalError)?;
        let conn = ConnectionManager::new(client)
            .await
            .map_err(|_| DomainError::InternalError)?;
        Ok(Self { conn, hasher, prefix: prefix.to_string() })
    }

    fn key(&self, user_id: Uuid) -> String {
        format!("{}:{user_id}", self.prefix)
    }

    fn attempts_key(&self, user_id: Uuid) -> String {
        format!("{}_attempts:{user_id}", self.prefix)
    }

    fn sent_key(&self, user_id: Uuid) -> String {
        format!("{}_sent:{user_id}", self.prefix)
    }
}

#[async_trait::async_trait]
impl AuthCodeStore for AuthCodeStoreRedis {
    /// Сохраняет (перезаписывает) код с TTL. Хэш живёт только в этом стеке
    /// и в Redis; сырой код сразу сбрасывается.
    async fn store(&self, user_id: Uuid, code: &str, ttl_seconds: u64) -> Result<(), DomainError> {
        let hash = self.hasher.hash(code)?;
        let mut conn = self.conn.clone();
        conn.set_ex::<_, _, ()>(self.key(user_id), hash, ttl_seconds)
            .await
            .map_err(|_| DomainError::InternalError)?;
        // A new code gets a fresh set of attempts.
        conn.del::<_, ()>(self.attempts_key(user_id))
            .await
            .map_err(|_| DomainError::InternalError)?;
        Ok(())
    }

    /// Проверяет код и потребляет при успехе (одноразовость).
    ///
    /// Возвращает `Ok(false)` во всех fail-safe случаях: кода нет (истёк TTL),
    /// код неверный, хэш повреждён. `Ok(true)` — только точное совпадение.
    async fn verify_and_consume(&self, user_id: Uuid, code: &str) -> Result<bool, DomainError> {
        Ok(self.check_and_consume(user_id, code).await? == CodeCheck::Valid)
    }

    /// The same check, telling a wrong code (with tries left) from a gone one.
    async fn check_and_consume(&self, user_id: Uuid, code: &str) -> Result<CodeCheck, DomainError> {
        let key = self.key(user_id);
        let mut conn = self.conn.clone();

        let stored: Option<String> = conn
            .get(&key)
            .await
            .map_err(|_| DomainError::InternalError)?;
        let stored = match stored {
            Some(s) => s,
            None => return Ok(CodeCheck::Gone), // не запрашивали, истёк TTL или сгорел
        };

        // Неверный код: считаем попытку; после MAX_WRONG_ATTEMPTS код сгорает.
        if !self.hasher.verify(code, &stored)? {
            let attempts_key = self.attempts_key(user_id);
            let attempts: i64 = conn
                .incr(&attempts_key, 1)
                .await
                .map_err(|_| DomainError::InternalError)?;
            if attempts == 1 {
                // The counter lives no longer than the code itself.
                let ttl: i64 = conn.ttl(&key).await.map_err(|_| DomainError::InternalError)?;
                conn.expire::<_, ()>(&attempts_key, ttl.max(1))
                    .await
                    .map_err(|_| DomainError::InternalError)?;
            }
            if attempts >= MAX_WRONG_ATTEMPTS {
                conn.del::<_, ()>(&[key.as_str(), attempts_key.as_str()])
                    .await
                    .map_err(|_| DomainError::InternalError)?;
                return Ok(CodeCheck::Gone);
            }
            let attempts_left = u32::try_from(MAX_WRONG_ATTEMPTS - attempts).unwrap_or(0);
            return Ok(CodeCheck::Wrong { attempts_left: Some(attempts_left) });
        }

        // Потребление: одноразовость.
        let deleted: i32 = conn
            .del(&key)
            .await
            .map_err(|_| DomainError::InternalError)?;
        conn.del::<_, ()>(self.attempts_key(user_id))
            .await
            .map_err(|_| DomainError::InternalError)?;
        Ok(if deleted == 1 { CodeCheck::Valid } else { CodeCheck::Gone })
    }

    /// `SET {prefix}_sent:{user_id} 1 NX EX cooldown`: true only for the first caller in
    /// the window.
    async fn try_acquire_send_slot(&self, user_id: Uuid, cooldown_seconds: u64) -> Result<bool, DomainError> {
        let mut conn = self.conn.clone();
        let reply: Option<String> = ::redis::cmd("SET")
            .arg(self.sent_key(user_id))
            .arg(1)
            .arg("NX")
            .arg("EX")
            .arg(cooldown_seconds.max(1))
            .query_async(&mut conn)
            .await
            .map_err(|_| DomainError::InternalError)?;
        Ok(reply.is_some())
    }

    async fn release_send_slot(&self, user_id: Uuid) -> Result<(), DomainError> {
        let mut conn = self.conn.clone();
        conn.del::<_, ()>(self.sent_key(user_id))
            .await
            .map_err(|_| DomainError::InternalError)
    }
}
