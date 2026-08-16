#[derive(Clone, Debug)]
pub struct CreatedClaimedUser {
    pub id: String,
    pub email: String,
}

#[derive(Clone, Debug)]
pub enum ClaimInitialAdminUserResult {
    Created(CreatedClaimedUser),
    AlreadyClaimed,
}

#[async_trait::async_trait]
pub trait ClaimPort: Send + Sync {
    async fn load_claim_status(&self) -> anyhow::Result<bool>;
    async fn claim_initial_admin_user(
        &self,
        user_id: &str,
        email: &str,
        password_hash: &str,
    ) -> anyhow::Result<ClaimInitialAdminUserResult>;
}
