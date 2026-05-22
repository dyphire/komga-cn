use super::*;
use axum::extract::FromRef;

use komga_application::identity_access::{
    AuthActivityPort, AuthenticationPort, DeviceSyncPort, SessionLifecyclePort,
    SessionResolverPort, UserAdminPort,
};
pub use komga_application::identity_access::{
    KoboMetadataRecord, KoreaderBookLookupError, KoreaderBookTarget, PersistedReadProgressRecord,
};

#[derive(Clone)]
pub struct IdentityState {
    authentication: Arc<dyn AuthenticationPort>,
    session_resolver: Arc<dyn SessionResolverPort>,
    session_lifecycle: Arc<dyn SessionLifecyclePort>,
    user_admin: Arc<dyn UserAdminPort>,
    auth_activity: Arc<dyn AuthActivityPort>,
    device_sync: Arc<dyn DeviceSyncPort>,
}

impl IdentityState {
    pub fn new<T>(access: Arc<T>) -> Self
    where
        T: AuthenticationPort
            + SessionResolverPort
            + SessionLifecyclePort
            + UserAdminPort
            + AuthActivityPort
            + DeviceSyncPort
            + 'static,
    {
        Self {
            authentication: access.clone(),
            session_resolver: access.clone(),
            session_lifecycle: access.clone(),
            user_admin: access.clone(),
            auth_activity: access.clone(),
            device_sync: access,
        }
    }

    pub fn authentication(&self) -> &dyn AuthenticationPort {
        &*self.authentication
    }

    pub fn session_resolver(&self) -> &dyn SessionResolverPort {
        &*self.session_resolver
    }

    pub fn session_lifecycle(&self) -> &dyn SessionLifecyclePort {
        &*self.session_lifecycle
    }

    pub fn user_admin(&self) -> &dyn UserAdminPort {
        &*self.user_admin
    }

    pub fn auth_activity(&self) -> &dyn AuthActivityPort {
        &*self.auth_activity
    }

    pub fn device_sync(&self) -> &dyn DeviceSyncPort {
        &*self.device_sync
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
