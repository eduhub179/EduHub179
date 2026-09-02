//! Redis adapters: эфемерные данные с TTL (коды входа; позже — кэш, rate-limit).
//!
//! Гарантии: все адаптеры возвращают `Result`, ошибки Redis маппятся в
//! `DomainError::InternalError` (fail-safe, без паник и без утечки деталей).
// pub mod auth_code_store_redis;
// pub use auth_code_store_redis::AuthCodeStoreRedis;
