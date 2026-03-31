use super::*;

pub async fn oauth2_authorization(
    Extension(state): Extension<OperationalState>,
    Path(registration_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Some(client) = state
        .oauth2_clients
        .iter()
        .find(|client| client.registration_id == registration_id)
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Ok(auth_url) = AuthUrl::new(client.authorization_uri.clone()) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Ok(token_url) = TokenUrl::new(client.token_uri.clone()) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let base_url = request_base_url(&headers);
    let Ok(redirect_url) = RedirectUrl::new(format!(
        "{base_url}/login/oauth2/code/{}",
        client.registration_id
    )) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let oauth_client = BasicClient::new(ClientId::new(client.client_id.clone()))
        .set_client_secret(ClientSecret::new(client.client_secret.clone()))
        .set_auth_uri(auth_url)
        .set_token_uri(token_url)
        .set_redirect_uri(redirect_url);

    let (url, csrf_state) = oauth_client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("openid".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .add_scope(Scope::new("email".to_string()))
        .url();

    let state_cookie = oauth2_state_cookie(client.registration_id.as_str(), csrf_state.secret());

    (
        StatusCode::FOUND,
        [
            (
                header::LOCATION,
                HeaderValue::from_str(url.as_str()).unwrap_or_else(|_| {
                    HeaderValue::from_static(
                        "/login?server_redirect=Y&error=oauth2_invalid_redirect",
                    )
                }),
            ),
            (
                header::SET_COOKIE,
                HeaderValue::from_str(state_cookie.as_str()).unwrap_or_else(|_| {
                    HeaderValue::from_static("komga-oauth2-state=; Path=/; HttpOnly; SameSite=Lax")
                }),
            ),
        ],
    )
        .into_response()
}

pub async fn oauth2_login_code(
    Extension(state): Extension<OperationalState>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    Path(registration_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<OAuth2CallbackQuery>,
) -> Response {
    let Some(client_config) = state
        .oauth2_clients
        .iter()
        .find(|client| client.registration_id == registration_id)
    else {
        return oauth2_login_error_redirect("oauth2_provider_not_found");
    };

    if let Some(error) = query.error.as_deref() {
        return oauth2_login_error_redirect(error);
    }

    let _ = query.state.as_deref();

    let Some(code) = query.code.as_deref() else {
        return oauth2_login_error_redirect("oauth2_missing_code");
    };

    let Some(received_state) = query.state.as_deref() else {
        return oauth2_login_error_redirect("oauth2_state_missing");
    };
    let Some(expected_state) = oauth2_state_from_headers(&headers, registration_id.as_str()) else {
        return oauth2_login_error_redirect("oauth2_state_missing");
    };
    if received_state != expected_state {
        return oauth2_login_error_redirect("oauth2_state_mismatch");
    }

    let base_url = request_base_url(&headers);
    let redirect_uri = format!("{base_url}/login/oauth2/code/{registration_id}");

    let token_payload = match exchange_oauth2_token(client_config, code, &redirect_uri).await {
        Ok(payload) => payload,
        Err(error) => return oauth2_login_error_redirect(error.as_str()),
    };

    let access_token = token_payload
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());

    let Some(access_token) = access_token else {
        return oauth2_login_error_redirect("oauth2_missing_access_token");
    };

    let email = resolve_oauth2_email(client_config, &token_payload, access_token)
        .await
        .or_else(|| {
            token_payload
                .get("email")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        });
    let Some(email) = email else {
        return oauth2_login_error_redirect("ERR_1024");
    };

    let allow_create = oauth2_account_creation_enabled(&state).await;
    let user = match ensure_oauth_user(state.runtime.database_file.as_path(), &email, allow_create)
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => return oauth2_login_error_redirect("ERR_1025"),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let session_token =
        session_token_for_user_with_namespace(&user, auth_db.remember_me_namespace.as_str());
    oauth2_login_success_redirect(session_token.as_str())
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
    let session_cookie = format!("KOMGA-SESSION={session_token}; Path=/; HttpOnly; SameSite=Lax");

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
            (
                HeaderName::from_static("x-auth-token"),
                HeaderValue::from_str(session_token)
                    .unwrap_or_else(|_| HeaderValue::from_static("")),
            ),
        ],
    )
        .into_response()
}

fn oauth2_state_cookie(registration_id: &str, state: &str) -> String {
    format!(
        "komga-oauth2-state-{registration_id}={state}; Path=/login/oauth2/code/{registration_id}; HttpOnly; SameSite=Lax"
    )
}

fn oauth2_state_from_headers(headers: &HeaderMap, registration_id: &str) -> Option<String> {
    let jar = CookieJar::from_headers(headers);
    let cookie_name = format!("komga-oauth2-state-{registration_id}");
    jar.get(cookie_name.as_str())
        .map(|cookie| cookie.value().to_string())
        .filter(|value| !value.trim().is_empty())
}

async fn exchange_oauth2_token(
    client: &crate::http::state::OAuth2ClientConfig,
    code: &str,
    redirect_uri: &str,
) -> Result<Value, String> {
    let http = Client::new();
    let form = reqwest::Url::parse_with_params(
        client.token_uri.as_str(),
        [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", client.client_id.as_str()),
            ("client_secret", client.client_secret.as_str()),
        ],
    )
    .ok()
    .and_then(|url| url.query().map(str::to_string))
    .ok_or_else(|| "oauth2_token_exchange_failed".to_string())?;

    let response = http
        .post(client.token_uri.as_str())
        .header(header::ACCEPT.as_str(), "application/json")
        .header(
            header::CONTENT_TYPE.as_str(),
            "application/x-www-form-urlencoded",
        )
        .body(form)
        .send()
        .await
        .map_err(|_| "oauth2_token_exchange_failed".to_string())?;

    let status = response.status();
    let payload = response
        .json::<Value>()
        .await
        .map_err(|_| "oauth2_token_invalid_response".to_string())?;

    if !status.is_success() {
        let error_code = payload
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("oauth2_token_exchange_failed");
        return Err(error_code.to_string());
    }

    Ok(payload)
}

async fn resolve_oauth2_email(
    client: &crate::http::state::OAuth2ClientConfig,
    _token_payload: &Value,
    access_token: &str,
) -> Option<String> {
    let http = Client::new();
    let candidates = oauth2_userinfo_candidates(client);
    for endpoint in candidates {
        let request = http
            .get(endpoint.as_str())
            .bearer_auth(access_token)
            .header("User-Agent", "komga-rust/runtime")
            .header(header::ACCEPT.as_str(), "application/json");

        let Ok(response) = request.send().await else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }

        let Ok(payload) = response.json::<Value>().await else {
            continue;
        };

        if let Some(email) = extract_email_from_userinfo_payload(&payload) {
            return Some(email);
        }
    }

    None
}

fn extract_email_from_userinfo_payload(payload: &Value) -> Option<String> {
    if let Some(email) = payload
        .get("email")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(email.to_string());
    }

    if let Some(email) = payload
        .get("preferred_username")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| value.contains('@'))
    {
        return Some(email.to_string());
    }

    if let Some(array) = payload.as_array() {
        let selected = array.iter().find(|entry| {
            entry
                .get("email")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
                && entry
                    .get("primary")
                    .and_then(Value::as_bool)
                    .unwrap_or(true)
        });
        if let Some(email) = selected
            .and_then(|entry| entry.get("email"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(email.to_string());
        }
    }

    None
}

fn oauth2_userinfo_candidates(client: &crate::http::state::OAuth2ClientConfig) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Ok(token_url) = reqwest::Url::parse(client.token_uri.as_str()) {
        let mut userinfo = token_url.clone();
        if let Ok(mut segments) = userinfo.path_segments_mut() {
            segments.pop_if_empty();
            segments.pop();
            segments.push("userinfo");
        }
        candidates.push(userinfo.to_string());
    }

    if let Ok(auth_url) = reqwest::Url::parse(client.authorization_uri.as_str()) {
        let mut userinfo = auth_url.clone();
        if let Ok(mut segments) = userinfo.path_segments_mut() {
            segments.pop_if_empty();
            segments.pop();
            segments.push("userinfo");
        }
        candidates.push(userinfo.to_string());

        if auth_url
            .host_str()
            .is_some_and(|host| host.contains("github.com"))
        {
            candidates.push("https://api.github.com/user/emails".to_string());
            candidates.push("https://api.github.com/user".to_string());
        }
    }

    candidates.sort();
    candidates.dedup();
    candidates
}

async fn oauth2_account_creation_enabled(state: &OperationalState) -> bool {
    if std::env::var("KOMGA_OAUTH2_ACCOUNT_CREATION")
        .ok()
        .is_some_and(|value| value.eq_ignore_ascii_case("true") || value == "1")
    {
        return true;
    }
    if std::env::var("KOMGA_OAUTH2_ACCOUNT_CREATION_ENABLED")
        .ok()
        .is_some_and(|value| value.eq_ignore_ascii_case("true") || value == "1")
    {
        return true;
    }

    let Ok(settings) = state.settings_store.load_map().await else {
        return false;
    };
    [
        "OAUTH2_ACCOUNT_CREATION",
        "oauth2AccountCreation",
        "oauth2.account.creation",
    ]
    .iter()
    .find_map(|key| settings.get(*key))
    .and_then(|value| value.as_ref())
    .is_some_and(|value| {
        value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes") || value == "1"
    })
}
