use std::future::Future;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssueDeviceCodeCommand {
    pub client_id: String,
    pub scopes: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionPrincipal {
    pub user_id: String,
    pub roles: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentityAccessError {
    pub message: String,
}

pub trait SessionReadModelPort {
    fn get_principal(
        &self,
        session_id: &str,
    ) -> impl Future<Output = Result<Option<SessionPrincipal>, IdentityAccessError>>;
}

pub struct IdentityAccessUseCases<R> {
    sessions: R,
}

impl<R> IdentityAccessUseCases<R>
where
    R: SessionReadModelPort,
{
    pub fn new(sessions: R) -> Self {
        Self { sessions }
    }

    pub async fn resolve_principal(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionPrincipal>, IdentityAccessError> {
        self.sessions.get_principal(session_id).await
    }
}
