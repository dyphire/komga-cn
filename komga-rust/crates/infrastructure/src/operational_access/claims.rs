use komga_application::operational::{
    ClaimInitialAdminUserResult as AppClaimResult, ClaimPort, CreatedClaimedUser,
};

use crate::claims_access::{self, ClaimInitialAdminUserResult};
use crate::database_handle::DatabaseHandle;

#[derive(Clone)]
pub struct ClaimAccess {
    db: DatabaseHandle,
}

impl ClaimAccess {
    pub fn new(db: DatabaseHandle) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl ClaimPort for ClaimAccess {
    async fn load_claim_status(&self) -> anyhow::Result<bool> {
        claims_access::load_claim_status(self.db.read_pool())
            .await
            .map_err(anyhow::Error::from)
    }

    async fn claim_initial_admin_user(
        &self,
        user_id: &str,
        email: &str,
        password_hash: &str,
    ) -> anyhow::Result<AppClaimResult> {
        let result = claims_access::claim_initial_admin_user(
            self.db.write_pool(),
            user_id,
            email,
            password_hash,
        )
        .await
        .map_err(anyhow::Error::from)?;
        Ok(match result {
            ClaimInitialAdminUserResult::Created(user) => {
                AppClaimResult::Created(CreatedClaimedUser {
                    id: user.id,
                    email: user.email,
                })
            }
            ClaimInitialAdminUserResult::AlreadyClaimed => AppClaimResult::AlreadyClaimed,
        })
    }
}
