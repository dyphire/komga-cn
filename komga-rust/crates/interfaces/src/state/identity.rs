use super::*;
use axum::extract::FromRef;

use komga_application::identity_access::IdentityAccessPort;
pub use komga_application::identity_access::{
    KoboMetadataRecord, KoreaderBookLookupError, KoreaderBookTarget, PersistedBookMediaFile,
    PersistedReadProgressRecord,
};

#[derive(Clone)]
pub struct IdentityState(Arc<dyn IdentityAccessPort>);

impl IdentityState {
    pub fn new(access: Arc<dyn IdentityAccessPort>) -> Self {
        Self(access)
    }
}

impl std::ops::Deref for IdentityState {
    type Target = dyn IdentityAccessPort + 'static;

    fn deref(&self) -> &(dyn IdentityAccessPort + 'static) {
        &*self.0
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
    pub(crate) server_settings: Arc<dyn komga_application::operational::ServerSettingsPort>,
    pub(crate) reader: Arc<dyn komga_application::media_assets::MediaReaderPort>,
    pub(crate) content: Arc<dyn komga_application::media_assets::ContentResolverPort>,
    pub(crate) progress: Arc<dyn komga_application::media_assets::ProgressWriterPort>,
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
