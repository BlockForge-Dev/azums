#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(feature = "sqlite")]
pub mod sqlite;

#[cfg(feature = "postgres")]
pub use postgres::PostgresBackend;
#[cfg(feature = "sqlite")]
pub use sqlite::{make_sqlite_pool, SqliteBackend};
#[cfg(feature = "redis")]
pub use azums_redis::RedisBackend;

pub use azums_core::StorageBackend;
