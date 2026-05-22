use super::*;
use komga_infrastructure::database_handle::DatabaseHandle;
use komga_infrastructure::runtime_identity_access::IdentityAccess;
use komga_infrastructure::sqlite::setup;
use std::path::PathBuf;

pub(crate) async fn test_identity_state() -> IdentityState {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test sqlite pool should connect");
    setup::bootstrap_pool(&pool)
        .await
        .expect("test sqlite pool should bootstrap");
    let handle = DatabaseHandle::single_pool(PathBuf::from(":memory:"), pool);
    IdentityState::new(Arc::new(IdentityAccess::new(handle)))
}
