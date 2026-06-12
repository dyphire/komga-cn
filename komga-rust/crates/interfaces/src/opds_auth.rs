use std::ops::Deref;

use axum::Json;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use komga_application::identity_access::AuthUser;

use crate::identity_access::auth::resolved_auth_user;
use crate::request_urls::{opds_auth_json, request_base_url};
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
        let user = match resolved_auth_user(&identity, &parts.headers) {
            Ok(Some(user)) => user,
            Ok(None) => return Err(opds_v1_basic_unauthorized_response()),
            Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response()),
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
        let user = match resolved_auth_user(&identity, &parts.headers) {
            Ok(Some(user)) => user,
            Ok(None) => return Err(opds_catalog_unauthorized_response(&parts.headers)),
            Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        };
        Ok(Self(user))
    }
}

pub(crate) async fn opds_auth(headers: HeaderMap) -> Response {
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/opds-authentication+json"),
        )],
        Json(opds_auth_json(&headers)),
    )
        .into_response()
}

pub(crate) fn opds_v1_basic_unauthorized_response() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Basic realm=\"Realm\""),
        )],
    )
        .into_response()
}

pub(crate) fn opds_catalog_unauthorized_response(headers: &HeaderMap) -> Response {
    let base_url = request_base_url(headers);
    let auth_href = format!("{base_url}/opds/v2/auth");
    let link = format!(
        "<{}>; rel=\"http://opds-spec.org/auth/document\"; type=\"application/opds-authentication+json\"",
        auth_href,
    );

    (
        StatusCode::UNAUTHORIZED,
        [
            (
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static("Basic realm=\"Realm\""),
            ),
            (header::LINK, HeaderValue::from_str(&link).unwrap()),
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/opds-authentication+json;charset=UTF-8"),
            ),
        ],
        Json(opds_auth_json(headers)),
    )
        .into_response()
}
