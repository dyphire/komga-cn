use super::*;
use crate::state::IdentityAccessState;
use axum::extract::State;
use axum_extra::extract::cookie::{Cookie, SameSite};
use komga_oauth::{AuthorizationSession, OAuthClientConfig};

pub async fn oauth2_authorization(
    State(app): State<IdentityAccessState>,
    Path(registration_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let state = &app.operational;
    let Some(client) = state
        .oauth2_clients
        .iter()
        .find(|client| client.registration_id == registration_id)
        .map(oauth_client_config)
    else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let base_url = request_base_url(&headers);
    let context_base_url = format!("{base_url}{}", request_context_path(&headers));
    let Ok(start) = komga_oauth::prepare_authorization(&client, context_base_url.as_str()).await
    else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let existing_session_token = oauth2_session_cookie_token(&headers);
    let session_token = existing_session_token
        .clone()
        .unwrap_or_else(komga_oauth::issue_pre_auth_session_token);
    let Ok(stored_state) = serde_json::to_string(&start.session) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    app.identity
        .session_lifecycle()
        .store_oauth2_authorization_state(
            &app.auth_db.session_runtime_key,
            &session_token,
            &client.registration_id,
            stored_state.as_str(),
        );

    let mut response = (
        StatusCode::FOUND,
        [(
            header::LOCATION,
            HeaderValue::from_str(start.authorization_url.as_str()).unwrap_or_else(|_| {
                HeaderValue::from_static("/login?server_redirect=Y&error=oauth2_invalid_redirect")
            }),
        )],
    )
        .into_response();

    if existing_session_token.is_none() {
        response.headers_mut().append(
            header::SET_COOKIE,
            HeaderValue::from_str(oauth2_session_cookie(session_token.as_str()).as_str())
                .unwrap_or_else(|_| {
                    HeaderValue::from_static("KOMGA-SESSION=; Path=/; HttpOnly; SameSite=Lax")
                }),
        );
    }

    response
}

pub async fn oauth2_login_code(
    State(app): State<IdentityAccessState>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    Path(registration_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<OAuth2CallbackQuery>,
) -> Response {
    let Some(client_config) = app
        .operational
        .oauth2_clients
        .iter()
        .find(|client| client.registration_id == registration_id)
        .map(oauth_client_config)
    else {
        return oauth2_login_error_redirect("oauth2_provider_not_found");
    };
    let client_name = client_config.client_name.as_str();

    if let Some(error) = query.error.as_deref() {
        return oauth2_login_error_response(
            &app,
            &headers,
            &connection_info,
            client_name,
            error,
            None,
        )
        .await;
    }

    let Some(code) = query.code.as_deref() else {
        return oauth2_login_error_response(
            &app,
            &headers,
            &connection_info,
            client_name,
            "oauth2_missing_code",
            None,
        )
        .await;
    };
    let Some(received_state) = query.state.as_deref() else {
        return oauth2_login_error_response(
            &app,
            &headers,
            &connection_info,
            client_name,
            "oauth2_state_missing",
            None,
        )
        .await;
    };
    let Some(session_token) = oauth2_session_cookie_token(&headers) else {
        return oauth2_login_error_response(
            &app,
            &headers,
            &connection_info,
            client_name,
            "oauth2_state_missing",
            None,
        )
        .await;
    };
    let Some(stored_state) = app
        .identity
        .session_lifecycle()
        .take_oauth2_authorization_state(
            &app.auth_db.session_runtime_key,
            &session_token,
            &registration_id,
        )
    else {
        return oauth2_login_error_response(
            &app,
            &headers,
            &connection_info,
            client_name,
            "oauth2_state_missing",
            None,
        )
        .await;
    };
    let Ok(session) = serde_json::from_str::<AuthorizationSession>(&stored_state) else {
        return oauth2_login_error_response(
            &app,
            &headers,
            &connection_info,
            client_name,
            "oauth2_state_missing",
            None,
        )
        .await;
    };

    let identity = match komga_oauth::complete_callback(
        &client_config,
        &session,
        code,
        received_state,
    )
    .await
    {
        Ok(identity) => identity,
        Err(error) => {
            return oauth2_login_error_response(
                &app,
                &headers,
                &connection_info,
                client_name,
                error.redirect_error_code(),
                None,
            )
            .await;
        }
    };

    if oauth2_client_uses_oidc(&client_config) && app.operational.oidc_email_verification {
        match identity.email_verified {
            Some(true) => {}
            Some(false) => {
                return oauth2_login_error_response(
                    &app,
                    &headers,
                    &connection_info,
                    client_name,
                    "ERR_1026",
                    Some(identity.email.as_str()),
                )
                .await;
            }
            None => {
                return oauth2_login_error_response(
                    &app,
                    &headers,
                    &connection_info,
                    client_name,
                    "ERR_1027",
                    Some(identity.email.as_str()),
                )
                .await;
            }
        }
    }

    let user = match app
        .identity
        .user_admin()
        .ensure_oauth_user(
            identity.email.as_str(),
            app.operational.oauth2_account_creation,
        )
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => {
            return oauth2_login_error_response(
                &app,
                &headers,
                &connection_info,
                client_name,
                "ERR_1025",
                Some(identity.email.as_str()),
            )
            .await;
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let source = format!("OAuth2:{client_name}");
    let _ = persisted_record_successful_authentication_activity(
        &app.identity,
        &user,
        authentication_activity_write_input(
            &authentication_activity_headers_metadata_with_remote_addr(
                &headers,
                connection_info.remote_addr(),
            ),
            source.as_str(),
            None,
            None,
        ),
    )
    .await;

    let session_token = session_token_for_user_with_runtime_key(
        &app.identity,
        &user,
        app.auth_db.session_runtime_key.as_str(),
    );
    oauth2_login_success_redirect(session_token.as_str())
}

fn oauth_client_config(client: &crate::state::OAuth2ClientConfig) -> OAuthClientConfig {
    OAuthClientConfig {
        registration_id: client.registration_id.clone(),
        client_name: client.client_name.clone(),
        client_id: client.client_id.clone(),
        client_secret: client.client_secret.clone(),
        authorization_uri: client.authorization_uri.clone(),
        token_uri: client.token_uri.clone(),
        user_info_uri: client.user_info_uri.clone(),
        issuer_uri: client.issuer_uri.clone(),
        jwk_set_uri: client.jwk_set_uri.clone(),
        redirect_uri: client.redirect_uri.clone(),
        client_authentication_method: client.client_authentication_method.clone(),
        scopes: client.scopes.clone(),
    }
}

fn oauth2_login_error_redirect(error: &str) -> Response {
    let redirect = format!("/login?server_redirect=Y&error={error}");
    (
        StatusCode::FOUND,
        [(
            header::LOCATION,
            HeaderValue::from_str(&redirect).unwrap_or_else(|_| {
                HeaderValue::from_static("/login?server_redirect=Y&error=oauth2_invalid_redirect")
            }),
        )],
    )
        .into_response()
}

fn oauth2_login_success_redirect(session_token: &str) -> Response {
    let session_cookie = oauth2_session_cookie(session_token);

    (
        StatusCode::FOUND,
        [
            (
                header::LOCATION,
                HeaderValue::from_static("/?server_redirect=Y"),
            ),
            (
                header::SET_COOKIE,
                HeaderValue::from_str(&session_cookie).unwrap_or_else(|_| {
                    HeaderValue::from_static("KOMGA-SESSION=; Path=/; HttpOnly; SameSite=Lax")
                }),
            ),
        ],
    )
        .into_response()
}

fn oauth2_client_uses_oidc(client: &OAuthClientConfig) -> bool {
    client
        .scopes
        .iter()
        .any(|scope| scope.eq_ignore_ascii_case("openid"))
}

fn oauth2_session_cookie(session_token: &str) -> String {
    Cookie::build(("KOMGA-SESSION", session_token.to_string()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build()
        .to_string()
}

fn oauth2_session_cookie_token(headers: &HeaderMap) -> Option<String> {
    let jar = CookieJar::from_headers(headers);
    jar.get("KOMGA-SESSION")
        .map(|cookie| cookie.value().to_string())
        .filter(|value| !value.trim().is_empty())
}

async fn oauth2_login_error_response(
    app: &IdentityAccessState,
    headers: &HeaderMap,
    connection_info: &RequestConnectionInfo,
    client_name: &str,
    error: &str,
    email: Option<&str>,
) -> Response {
    let source = format!("OAuth2:{client_name}");
    let input = authentication_activity_write_input(
        &authentication_activity_headers_metadata_with_remote_addr(
            headers,
            connection_info.remote_addr(),
        ),
        source.as_str(),
        None,
        None,
    );
    let _ = app
        .identity
        .auth_activity()
        .persisted_record_failed_authentication_activity(
            email,
            &input.source,
            error,
            input.ip.as_deref(),
            input.user_agent.as_deref(),
        )
        .await;

    oauth2_login_error_redirect(error)
}
