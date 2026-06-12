use std::sync::Arc;

use axum::extract::FromRef;

use komga_application::identity_access::{
    AuthActivityPort, AuthSessionService, AuthenticationPort, DeviceSyncPort, KoboProxyPort,
    KoboStoreSyncPort, KoboSyncStatePort, SessionLifecyclePort, SessionResolverPort, UserAdminPort,
};
use komga_application::runtime_sse::RuntimeSseEventSource;

use super::app_state::{HttpAppState, OperationalState};
use super::core::AuthDatabaseState;
use crate::discovery_auth::state::DiscoveryAuthState;

#[derive(Clone)]
pub struct IdentityState {
    authentication: Arc<dyn AuthenticationPort>,
    session_resolver: Arc<dyn SessionResolverPort>,
    session_lifecycle: Arc<dyn SessionLifecyclePort>,
    auth_session: Arc<AuthSessionService>,
    user_admin: Arc<dyn UserAdminPort>,
    auth_activity: Arc<dyn AuthActivityPort>,
    device_sync: Arc<dyn DeviceSyncPort>,
    kobo_sync_state: Arc<dyn KoboSyncStatePort>,
    kobo_store_sync: Arc<dyn KoboStoreSyncPort>,
    kobo_proxy: Arc<dyn KoboProxyPort>,
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
            + KoboSyncStatePort
            + KoboProxyPort
            + KoboStoreSyncPort
            + 'static,
    {
        let authentication: Arc<dyn AuthenticationPort> = access.clone();
        let session_resolver: Arc<dyn SessionResolverPort> = access.clone();
        let session_lifecycle: Arc<dyn SessionLifecyclePort> = access.clone();
        let auth_activity: Arc<dyn AuthActivityPort> = access.clone();
        let auth_session = Arc::new(AuthSessionService::new(
            authentication.clone(),
            session_resolver.clone(),
            session_lifecycle.clone(),
            auth_activity.clone(),
        ));
        Self {
            authentication,
            session_resolver,
            session_lifecycle,
            auth_session,
            user_admin: access.clone(),
            auth_activity,
            device_sync: access.clone(),
            kobo_sync_state: access.clone(),
            kobo_store_sync: access.clone(),
            kobo_proxy: access,
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

    pub fn auth_session(&self) -> &AuthSessionService {
        &self.auth_session
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

    pub fn kobo_sync_state(&self) -> &dyn KoboSyncStatePort {
        &*self.kobo_sync_state
    }

    pub fn kobo_store_sync(&self) -> &dyn KoboStoreSyncPort {
        &*self.kobo_store_sync
    }

    pub fn kobo_proxy(&self) -> &dyn KoboProxyPort {
        &*self.kobo_proxy
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
    pub(crate) runtime_events: Arc<dyn RuntimeSseEventSource>,
    pub(crate) server_settings: Arc<dyn komga_application::operational::ServerSettingsPort>,
    pub(crate) device_progress_reader:
        Arc<dyn komga_application::identity_access::DeviceProgressReaderPort>,
    pub(crate) book_media_reader: Arc<dyn komga_application::media_assets::BookMediaReaderPort>,
    pub(crate) epub_navigation_content:
        Arc<dyn komga_application::media_assets::EpubNavigationContentPort>,
    pub(crate) content_resolver: Arc<dyn komga_application::media_assets::ContentResolverPort>,
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
            runtime_events: app.services.runtime_events.clone(),
            server_settings: app.services.server_settings.clone(),
            device_progress_reader: app.services.device_progress_reader.clone(),
            book_media_reader: app.services.book_media_reader.clone(),
            epub_navigation_content: app.services.epub_navigation_content.clone(),
            content_resolver: app.services.content_resolver.clone(),
            progress: app.services.progress_writer.clone(),
        }
    }
}
