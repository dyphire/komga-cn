use std::ops::Deref;

use axum::extract::FromRef;
use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};

use super::{AuthUser, resolved_request_auth_user, user_has_role, user_is_admin};
use crate::state::IdentityState;

#[derive(Clone)]
pub struct Authenticated(pub AuthUser);

#[derive(Clone)]
pub struct Admin(pub AuthUser);

#[derive(Clone)]
pub struct FileDownload(pub AuthUser);

impl Deref for Authenticated {
    type Target = AuthUser;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for Admin {
    type Target = AuthUser;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for FileDownload {
    type Target = AuthUser;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S> FromRequestParts<S> for Authenticated
where
    S: Send + Sync,
    IdentityState: FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let identity = IdentityState::from_ref(state);
        let Some(user) = resolved_request_auth_user(&*identity.service, &parts.headers).await
        else {
            return Err(StatusCode::UNAUTHORIZED.into_response());
        };
        Ok(Self(user))
    }
}

impl<S> FromRequestParts<S> for Admin
where
    S: Send + Sync,
    IdentityState: FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Authenticated(user) = Authenticated::from_request_parts(parts, state).await?;
        if user_is_admin(&user) {
            Ok(Self(user))
        } else {
            Err(StatusCode::FORBIDDEN.into_response())
        }
    }
}

impl<S> FromRequestParts<S> for FileDownload
where
    S: Send + Sync,
    IdentityState: FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Authenticated(user) = Authenticated::from_request_parts(parts, state).await?;
        if user_is_admin(&user) || user_has_role(&user, "FILE_DOWNLOAD") {
            Ok(Self(user))
        } else {
            Err(StatusCode::FORBIDDEN.into_response())
        }
    }
}
