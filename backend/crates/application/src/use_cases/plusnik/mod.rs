//! Plusnik (special math) use cases: sheets, tasks, pluses, student progress.
//!
//! Dependencies: `domain` crate (entities, repositories, errors).
//! Guarantees: all flows return `Result` and stay independent from PostgreSQL details.

pub mod plusnik_service;

pub use plusnik_service::PlusnikService;
