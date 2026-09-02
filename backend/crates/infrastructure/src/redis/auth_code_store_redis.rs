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
//! Зависимости: крейт `redis`, `domain` (ports, errors), внутренний `PasswordHasher`.
//! Гарантии: все методы возвращают `Result`; паники недопустимы.
use domain::errors::DomainError;
use domain::ports::auth::AuthCodeStore;
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
pub struct AuthCodeStoreRedis {
    conn: ConnectionManager,
    /// Инжектируется, как в `CredentialsStorePg`: хэширование — забота инфраструктуры.
    hasher: Arc<dyn PasswordHasher>,
}

impl AuthCodeStoreRedis {
    /// Создаёт хранилище. `url` — из `REDIS_URL` (например, `redis://127.0.0.1:6379`).
    ///
    /// Fail-safe: ошибка подключения → `Err(InternalError)`, без паник.
    pub async fn new(url: &str, hasher: Arc<dyn PasswordHasher>) -> Result<Self, DomainError> {
        let client = ::redis::Client::open(url).map_err(|_| DomainError::InternalError)?;
        let conn = ConnectionManager::new(client)
            .await
            .map_err(|_| DomainError::InternalError)?;
        Ok(Self { conn, hasher })
    }

    fn key(user_id: Uuid) -> String {
        format!("auth_code:{user_id}")
    }
}

#[async_trait::async_trait]
impl AuthCodeStore for AuthCodeStoreRedis {
    /// Сохраняет (перезаписывает) код с TTL. Хэш живёт только в этом стеке
    /// и в Redis; сырой код сразу сбрасывается.
    async fn store(
        &self,
        user_id: Uuid,
        code: &str,
        ttl_seconds: i64,
    ) -> Result<(), DomainError> {
        // Fail-safe: отрицательный TTL — ошибка конфигурации, а не паника.
        let ttl = u64::try_from(ttl_seconds).map_err(|_| DomainError::InternalError)?;
        let hash = self.hasher.hash(code)?;
        let mut conn = self.conn.clone();
        conn.set_ex::<_, _, ()>(Self::key(user_id), hash, ttl)
            .await
            .map_err(|_| DomainError::InternalError)?;
        Ok(())
    }

    /// Проверяет код и потребляет при успехе (одноразовость).
    ///
    /// Возвращает `Ok(false)` во всех fail-safe случаях: кода нет (истёк TTL),
    /// код неверный, хэш повреждён. `Ok(true)` — только точное совпадение.
    async fn verify_and_consume(&self, user_id: Uuid, code: &str) -> Result<bool, DomainError> {
        let key = Self::key(user_id);
        let mut conn = self.conn.clone();

        let stored: Option<String> = conn
            .get(&key)
            .await
            .map_err(|_| DomainError::InternalError)?;
        let stored = match stored {
            Some(s) => s,
            None => return Ok(false), // не запрашивали или истёк TTL
        };

        // Неверный код НЕ удаляем — можно повторять до TTL (ограничит rate-limit).
        if !self.hasher.verify(code, &stored)? {
            return Ok(false);
        }

        // Потребление: одноразовость.
        let deleted: i32 = conn
            .del(&key)
            .await
            .map_err(|_| DomainError::InternalError)?;
        Ok(deleted == 1)
    }
}
