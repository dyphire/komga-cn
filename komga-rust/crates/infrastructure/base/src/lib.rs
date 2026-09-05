pub mod database;
pub mod file_io;
mod riir_database;
mod shared;
pub mod sqlite;
pub mod stored_paths;
pub mod unit_of_work;

pub use database::DatabaseHandle;
pub use riir_database::RiirDatabase;
pub use shared::random_hex_token;
pub use sqlite::{
    DEFAULT_MAX_CONNECTIONS, SharedSqlitePoolSnapshot, SqliteTempPool, WRITE_MAX_CONNECTIONS,
    bootstrap_pool, bootstrap_tasks_pool, close_all_shared_pools, connect_main_write_context,
    connect_read_pool, connect_shared_pool, connect_task_pool, connect_task_write_pool,
    connect_test_pool, connect_write_pool, default_read_max_connections,
    evict_shared_pools_for_paths, file_backed_connect_options, reject_or_quarantine_pool_topology,
    shared_pool_snapshots_for_paths,
};
pub use stored_paths::{
    resolve_library_item_path, resolve_optional_library_item_path, resolve_rooted_path,
    resolve_stored_path,
};
pub use unit_of_work::{SqlitePersistenceConnection, SqlitePersistenceContext, SqliteUnitOfWork};
