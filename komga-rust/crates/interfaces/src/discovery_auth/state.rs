use axum::http::HeaderMap;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use super::context::{
    DetailAccessDenial, DetailResourceContext, DiscoveryQueryContext, to_query_context,
};
use super::principal::{DiscoveryPrincipal, principal_from_user};
use super::utils::session_token_from_headers;
use crate::identity_access::auth::resolved_request_auth_user;
use crate::state::IdentityState;

const DISCOVERY_PRINCIPAL_TTL_SECONDS: u64 = 30 * 60;
const DISCOVERY_PRINCIPAL_CACHE_MAX_ENTRIES: usize = 1024;

#[derive(Clone)]
struct CachedDiscoveryPrincipal {
    principal: DiscoveryPrincipal,
    expires_at_epoch_seconds: u64,
    inserted_at_epoch_seconds: u64,
}

#[derive(Clone, Default)]
pub struct DiscoveryAuthState {
    principals_by_session_token: Arc<RwLock<HashMap<String, CachedDiscoveryPrincipal>>>,
}

impl DiscoveryAuthState {
    pub(crate) fn register_session_principal(
        &self,
        session_token: &str,
        principal: DiscoveryPrincipal,
    ) {
        let token = session_token.trim();
        if token.is_empty() {
            return;
        }
        let now = now_epoch_seconds();
        let mut sessions = self
            .principals_by_session_token
            .write()
            .expect("discovery auth session store lock should not be poisoned");
        purge_expired_principals(&mut sessions, now);
        if sessions.len() >= DISCOVERY_PRINCIPAL_CACHE_MAX_ENTRIES {
            evict_oldest_principal(&mut sessions);
        }
        sessions.insert(
            token.to_string(),
            CachedDiscoveryPrincipal {
                principal,
                expires_at_epoch_seconds: now.saturating_add(DISCOVERY_PRINCIPAL_TTL_SECONDS),
                inserted_at_epoch_seconds: now,
            },
        );
    }

    pub(crate) fn resolve_query_context(
        &self,
        headers: &HeaderMap,
        requested_library_ids: Option<&[String]>,
    ) -> Option<DiscoveryQueryContext> {
        let session_token = session_token_from_headers(headers)?;
        let now = now_epoch_seconds();
        let principal = self
            .principals_by_session_token
            .read()
            .expect("discovery auth session store lock should not be poisoned")
            .get(&session_token)
            .and_then(|cached| {
                (cached.expires_at_epoch_seconds > now).then(|| cached.principal.clone())
            })?;

        Some(to_query_context(&principal, requested_library_ids))
    }

    pub(crate) fn resolve_detail_query_context(
        &self,
        headers: &HeaderMap,
        detail: &DetailResourceContext,
    ) -> Result<DiscoveryQueryContext, DetailAccessDenial> {
        let session_token =
            session_token_from_headers(headers).ok_or(DetailAccessDenial::Unauthorized)?;
        let now = now_epoch_seconds();
        let principal = self
            .principals_by_session_token
            .read()
            .expect("discovery auth session store lock should not be poisoned")
            .get(&session_token)
            .and_then(|cached| {
                (cached.expires_at_epoch_seconds > now).then(|| cached.principal.clone())
            })
            .ok_or(DetailAccessDenial::Unauthorized)?;

        if !principal.can_access_all_libraries() {
            let Some(library_id) = detail.library_id.as_deref() else {
                return Err(DetailAccessDenial::NotFound);
            };

            if !principal.can_access_library(library_id) {
                return Err(DetailAccessDenial::Forbidden);
            }
        }

        if principal.restrictions.is_restricted() {
            let Some(content) = detail.content.as_ref() else {
                return Err(DetailAccessDenial::NotFound);
            };

            if !principal.is_content_allowed(content.age_rating, &content.sharing_labels) {
                return Err(DetailAccessDenial::Forbidden);
            }
        }

        let requested_library_ids = detail
            .library_id
            .as_ref()
            .map(|library_id| vec![library_id.clone()]);

        Ok(to_query_context(
            &principal,
            requested_library_ids.as_deref(),
        ))
    }

    async fn request_principal(
        &self,
        identity: &IdentityState,
        headers: &HeaderMap,
    ) -> anyhow::Result<Option<DiscoveryPrincipal>> {
        let Some(user) = resolved_request_auth_user(identity, headers).await? else {
            return Ok(None);
        };
        Ok(principal_from_user(&user))
    }

    fn detail_requested_library_ids(detail: &DetailResourceContext) -> Option<Vec<String>> {
        detail
            .library_id
            .as_ref()
            .map(|library_id| vec![library_id.clone()])
    }

    pub(crate) async fn resolve_query_context_with_persistence(
        &self,
        identity: &IdentityState,
        headers: &HeaderMap,
        requested_library_ids: Option<&[String]>,
    ) -> anyhow::Result<Option<DiscoveryQueryContext>> {
        if let Some(context) = self.resolve_query_context(headers, requested_library_ids) {
            return Ok(Some(context));
        }

        let Some(principal) = self.request_principal(identity, headers).await? else {
            return Ok(None);
        };
        Ok(Some(to_query_context(&principal, requested_library_ids)))
    }

    pub(crate) async fn resolve_detail_query_context_with_persistence(
        &self,
        identity: &IdentityState,
        headers: &HeaderMap,
        detail: &DetailResourceContext,
    ) -> Result<DiscoveryQueryContext, DetailAccessDenial> {
        match self.resolve_detail_query_context(headers, detail) {
            Ok(context) => return Ok(context),
            Err(DetailAccessDenial::Unauthorized) => {}
            Err(denial) => return Err(denial),
        }

        let principal = self
            .request_principal(identity, headers)
            .await
            .map_err(|_| DetailAccessDenial::StorageFailure)?
            .ok_or(DetailAccessDenial::Unauthorized)?;

        if !principal.can_access_all_libraries() {
            let Some(library_id) = detail.library_id.as_deref() else {
                return Err(DetailAccessDenial::NotFound);
            };

            if !principal.can_access_library(library_id) {
                return Err(DetailAccessDenial::Forbidden);
            }
        }

        if principal.restrictions.is_restricted() {
            let Some(content) = detail.content.as_ref() else {
                return Err(DetailAccessDenial::NotFound);
            };

            if !principal.is_content_allowed(content.age_rating, &content.sharing_labels) {
                return Err(DetailAccessDenial::Forbidden);
            }
        }

        let requested_library_ids = Self::detail_requested_library_ids(detail);

        Ok(to_query_context(
            &principal,
            requested_library_ids.as_deref(),
        ))
    }
}

fn purge_expired_principals(sessions: &mut HashMap<String, CachedDiscoveryPrincipal>, now: u64) {
    sessions.retain(|_, cached| cached.expires_at_epoch_seconds > now);
}

fn evict_oldest_principal(sessions: &mut HashMap<String, CachedDiscoveryPrincipal>) {
    let Some(oldest_key) = sessions
        .iter()
        .min_by_key(|(_, cached)| cached.inserted_at_epoch_seconds)
        .map(|(token, _)| token.clone())
    else {
        return;
    };
    sessions.remove(&oldest_key);
}

fn now_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
