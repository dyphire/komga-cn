use super::*;
use komga_infrastructure_base::DatabaseHandle;
use komga_infrastructure_base::bootstrap_pool;
use komga_infrastructure_identity::IdentityAccess;
use komga_infrastructure_media_core::ContentResolver;
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) async fn test_identity_state() -> IdentityState {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test sqlite pool should connect");
    bootstrap_pool(&pool)
        .await
        .expect("test sqlite pool should bootstrap");
    let handle = DatabaseHandle::single_pool(PathBuf::from(":memory:"), pool);
    IdentityState::new(Arc::new(IdentityAccess::new(
        handle,
        Arc::new(ContentResolver),
    )))
}
