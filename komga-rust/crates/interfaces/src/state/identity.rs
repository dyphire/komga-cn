use super::*;
use axum::extract::FromRef;

pub use komga_infrastructure::runtime_identity_access::{
    IdentityAccess, KoboMetadataRecord, KoreaderBookLookupError, KoreaderBookTarget,
    PersistedBookMediaFile, PersistedReadProgressRecord,
};

#[derive(Clone)]
pub struct IdentityState(Arc<IdentityAccess>);

impl IdentityState {
    pub fn new(access: Arc<IdentityAccess>) -> Self {
        Self(access)
    }
}

impl std::ops::Deref for IdentityState {
    type Target = IdentityAccess;

    fn deref(&self) -> &IdentityAccess {
        &self.0
    }
}

#[derive(Clone, Debug, Default)]
pub struct AuthenticationActivityWriteInput {
    pub source: String,
    pub api_key_id: Option<String>,
    pub api_key_comment: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Clone)]
pub struct IdentityAccessState {
    pub(crate) discovery_auth: DiscoveryAuthState,
    pub(crate) auth_db: AuthDatabaseState,
    pub(crate) operational: OperationalState,
    pub(crate) identity: IdentityState,
    pub(crate) server_settings:
        Arc<komga_infrastructure::sqlite::write_models::server_settings::ServerSettingsStore>,
    pub(crate) reader: komga_infrastructure::media_reader::MediaReader,
    pub(crate) content: komga_infrastructure::content_resolver::ContentResolver,
    pub(crate) progress: komga_infrastructure::progress_writer::ProgressWriter,
}

impl FromRef<Arc<HttpAppState>> for IdentityState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        app.services.identity.clone()
    }
}

impl FromRef<Arc<HttpAppState>> for IdentityAccessState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            discovery_auth: app.discovery_auth.clone(),
            auth_db: app.auth_db.clone(),
            operational: app.operational.clone(),
            identity: app.services.identity.clone(),
            server_settings: app.services.server_settings.clone(),
            reader: app.services.media_reader.clone(),
            content: app.services.content_resolver.clone(),
            progress: app.services.progress_writer.clone(),
        }
    }
}
