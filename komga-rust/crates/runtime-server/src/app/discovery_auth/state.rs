use axum::http::HeaderMap;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::context::{
    DetailAccessDenial, DetailResourceContext, DiscoveryQueryContext, to_query_context,
};
use super::principal::DiscoveryPrincipal;
use super::utils::session_token_from_headers;

#[derive(Clone, Default)]
pub struct DiscoveryAuthState {
    principals_by_session_token: Arc<Mutex<HashMap<String, DiscoveryPrincipal>>>,
}

impl DiscoveryAuthState {
    pub fn register_session_principal(&self, session_token: &str, principal: DiscoveryPrincipal) {
        let token = session_token.trim();
        if token.is_empty() {
            return;
        }
        let mut sessions = self
            .principals_by_session_token
            .lock()
            .expect("discovery auth session store lock should not be poisoned");
        sessions.insert(token.to_string(), principal);
    }

    pub fn resolve_query_context(
        &self,
        headers: &HeaderMap,
        requested_library_ids: Option<&[String]>,
    ) -> Option<DiscoveryQueryContext> {
        let session_token = session_token_from_headers(headers)?;
        let principal = self
            .principals_by_session_token
            .lock()
            .expect("discovery auth session store lock should not be poisoned")
            .get(&session_token)
            .cloned()?;

        Some(to_query_context(&principal, requested_library_ids))
    }

    pub fn resolve_detail_query_context(
        &self,
        headers: &HeaderMap,
        detail: &DetailResourceContext,
    ) -> Result<DiscoveryQueryContext, DetailAccessDenial> {
        let session_token =
            session_token_from_headers(headers).ok_or(DetailAccessDenial::Unauthorized)?;
        let principal = self
            .principals_by_session_token
            .lock()
            .expect("discovery auth session store lock should not be poisoned")
            .get(&session_token)
            .cloned()
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
}
