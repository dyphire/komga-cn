use super::*;
use komga_infrastructure::persistence::bootstrap_pool;
use komga_infrastructure::{
    identity::IdentityAccess, media::ContentResolver, persistence::DatabaseHandle,
};
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
