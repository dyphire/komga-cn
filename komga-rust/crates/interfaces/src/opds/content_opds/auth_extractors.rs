use std::ops::Deref;

use axum::extract::FromRef;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::Response;

use super::auth_payload::{
    opds_catalog_unauthorized_response, opds_v1_basic_unauthorized_response,
};
use crate::identity_access::auth::{AuthUser, resolved_auth_user};
use crate::state::IdentityState;

#[derive(Clone, Debug)]
pub(crate) struct OpdsV1Authenticated(pub AuthUser);

#[derive(Clone, Debug)]
pub(crate) struct OpdsV2Authenticated(pub AuthUser);

impl Deref for OpdsV1Authenticated {
    type Target = AuthUser;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for OpdsV2Authenticated {
    type Target = AuthUser;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> FromRequestParts<S> for OpdsV1Authenticated
where
    S: Send + Sync,
    IdentityState: FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let identity = IdentityState::from_ref(state);
        let Some(user) = resolved_auth_user(&identity, &parts.headers) else {
            return Err(opds_v1_basic_unauthorized_response());
        };
        Ok(Self(user))
    }
}

impl<S> FromRequestParts<S> for OpdsV2Authenticated
where
    S: Send + Sync,
    IdentityState: FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let identity = IdentityState::from_ref(state);
        let Some(user) = resolved_auth_user(&identity, &parts.headers) else {
            return Err(opds_catalog_unauthorized_response(&parts.headers));
        };
        Ok(Self(user))
    }
}
