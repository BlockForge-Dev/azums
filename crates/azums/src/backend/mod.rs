#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(feature = "sqlite")]
pub mod sqlite;

#[cfg(feature = "redis")]
pub use azums_redis::RedisBackend;
#[cfg(feature = "postgres")]
pub use postgres::PostgresBackend;
#[cfg(feature = "sqlite")]
pub use sqlite::{make_sqlite_pool, SqliteBackend};

pub use azums_core::StorageBackend;
