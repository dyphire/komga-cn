use super::*;
use axum::extract::ConnectInfo;
use axum::response::Response;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use komga_config::cli_args::RuntimeCli;
use komga_config::env_config::RuntimeConfig;
use reqwest::Url;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

type AuthenticationActivityRow = (
    bool,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

fn oauth2_runtime_env_for_paths(
    paths: &RuntimeDbPaths,
    registration_id: &str,
    authorization_uri: &str,
    token_uri: &str,
    user_info_uri: Option<&str>,
    scopes: &str,
) -> BTreeMap<String, String> {
    let registration_key = registration_id.to_ascii_uppercase();
    let provider_key = registration_id.to_ascii_uppercase();

    let mut env = BTreeMap::new();
    env.insert(
        "KOMGA_CONFIG_DIR".to_string(),
        paths.config_dir.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_DATABASE_FILE".to_string(),
        paths.main_db.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_TASKS_DB_FILE".to_string(),
        paths.tasks_db.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_RUST_RUNTIME_PROFILE".to_string(),
        "snapshot-aligned".to_string(),
    );
    env.insert(
        format!("SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_{registration_key}_CLIENT_ID"),
        format!("{registration_id}-client-id"),
    );
    env.insert(
        format!("SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_{registration_key}_CLIENT_SECRET"),
        format!("{registration_id}-client-secret"),
    );
    env.insert(
        format!("SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_{registration_key}_SCOPE"),
        scopes.to_string(),
    );
    env.insert(
        format!("SPRING_SECURITY_OAUTH2_CLIENT_PROVIDER_{provider_key}_AUTHORIZATION_URI"),
        authorization_uri.to_string(),
    );
    env.insert(
        format!("SPRING_SECURITY_OAUTH2_CLIENT_PROVIDER_{provider_key}_TOKEN_URI"),
        token_uri.to_string(),
    );
    if let Some(user_info_uri) = user_info_uri {
        env.insert(
            format!("SPRING_SECURITY_OAUTH2_CLIENT_PROVIDER_{provider_key}_USER_INFO_URI"),
            user_info_uri.to_string(),
        );
    }
    env
}

fn oauth2_runtime_config_for_base_url(
    paths: &RuntimeDbPaths,
    registration_id: &str,
    base_url: &str,
    scopes: &str,
) -> RuntimeConfig {
    let env = oauth2_runtime_env_for_paths(
        paths,
        registration_id,
        format!("{base_url}/oauth/authorize").as_str(),
        format!("{base_url}/oauth/token").as_str(),
        Some(format!("{base_url}/oauth/userinfo").as_str()),
        scopes,
    );
    RuntimeConfig::resolve_with_env(&RuntimeCli::default(), &env)
        .expect("runtime config should resolve oauth2 callback env")
}

async fn oauth2_authorization_response_for_config(
    config: &RuntimeConfig,
    registration_id: &str,
) -> Response {
    let app = build_router_with_config(config).await;
    app.oneshot(
        Request::builder()
            .method("GET")
            .uri(format!("/oauth2/authorization/{registration_id}"))
            .header(header::HOST, "komga.example")
            .body(Body::empty())
            .expect("oauth2 authorization request should build"),
    )
    .await
    .expect("oauth2 authorization request should complete")
}

async fn oauth2_callback_response_for_config(
    config: &RuntimeConfig,
    registration_id: &str,
) -> Response {
    oauth2_callback_response_for_config_with_request_metadata(config, registration_id, None, None)
        .await
}

async fn oauth2_callback_response_for_config_with_request_metadata(
    config: &RuntimeConfig,
    registration_id: &str,
    remote_addr: Option<SocketAddr>,
    user_agent: Option<&str>,
) -> Response {
    let app = build_router_with_config(config).await;
    let authorization_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/oauth2/authorization/{registration_id}"))
                .header(header::HOST, "komga.example")
                .body(Body::empty())
                .expect("oauth2 authorization request should build"),
        )
        .await
        .expect("oauth2 authorization request should complete");
    let authorization_location = authorization_response
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .expect("oauth2 authorization response should redirect");
    let state = Url::parse(authorization_location)
        .expect("oauth2 authorization location should be a valid url")
        .query_pairs()
        .find_map(|(key, value)| (key == "state").then_some(value.into_owned()))
        .expect("oauth2 authorization redirect should include state query");
    let session_cookie = authorization_response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .expect("oauth2 authorization should issue standard session cookie")
        .to_string();

    let mut callback_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/login/oauth2/code/{registration_id}?code=valid-code&state={state}"
        ))
        .header(header::HOST, "komga.example")
        .header(header::COOKIE, session_cookie);
    if let Some(user_agent) = user_agent {
        callback_request = callback_request.header(header::USER_AGENT, user_agent);
    }
    let mut callback_request = callback_request
        .body(Body::empty())
        .expect("oauth2 callback request should build");
    if let Some(remote_addr) = remote_addr {
        callback_request
            .extensions_mut()
            .insert(ConnectInfo(remote_addr));
    }

    app.oneshot(callback_request)
        .await
        .expect("oauth2 callback request should complete")
}

async fn spawn_path_response_server(routes: &[(&str, u16, &str, &str)]) -> SingleResponseServer {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("path response server should bind");
    let address = listener
        .local_addr()
        .expect("path response server should have local addr");
    let routes = routes
        .iter()
        .map(|(path, status, content_type, body)| {
            (
                (*path).to_string(),
                *status,
                (*content_type).to_string(),
                (*body).to_string(),
            )
        })
        .collect::<Vec<_>>();
    let join = tokio::spawn(async move {
        loop {
            let accept =
                tokio::time::timeout(std::time::Duration::from_millis(200), listener.accept())
                    .await;
            let (mut stream, _) = match accept {
                Ok(Ok(connection)) => connection,
                Ok(Err(error)) => {
                    panic!("path response server should accept a connection: {error}")
                }
                Err(_) => break,
            };
            let mut request = Vec::new();
            let mut chunk = [0_u8; 1024];

            loop {
                let read = stream
                    .read(&mut chunk)
                    .await
                    .expect("path response server should read request bytes");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..read]);

                let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
                else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.split_once(':').and_then(|(name, value)| {
                            name.trim()
                                .eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                    })
                    .unwrap_or(0);
                if request.len() >= header_end + 4 + content_length {
                    break;
                }
            }

            let header_end = request
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .expect("path response server should receive complete headers");
            let request_line = String::from_utf8_lossy(&request[..header_end])
                .lines()
                .next()
                .unwrap_or_default()
                .to_string();
            let path = request_line
                .split_whitespace()
                .nth(1)
                .unwrap_or("/")
                .to_string();
            let (_, status_code, content_type, body) = routes
                .iter()
                .find(|(expected_path, ..)| expected_path == &path)
                .unwrap_or_else(|| panic!("unexpected path response server request path: {path}"));
            let status_text = match status_code {
                200 => "OK",
                302 => "Found",
                400 => "Bad Request",
                401 => "Unauthorized",
                404 => "Not Found",
                500 => "Internal Server Error",
                503 => "Service Unavailable",
                _ => "OK",
            };
            let response = format!(
                "HTTP/1.1 {status_code} {status_text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("path response server should write response");
        }
    });

    SingleResponseServer {
        url: format!("http://{address}"),
        join,
    }
}

fn unsigned_jwt_token(claims: Value) -> String {
    let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(claims.to_string());
    format!("{header}.{payload}.")
}

fn oidc_token_payload(email: Option<&str>, email_verified: Option<bool>) -> String {
    let mut claims = serde_json::Map::new();
    if let Some(email) = email {
        claims.insert("email".to_string(), Value::String(email.to_string()));
    }
    if let Some(email_verified) = email_verified {
        claims.insert("email_verified".to_string(), Value::Bool(email_verified));
    }

    json!({
        "access_token": "oidc-token",
        "id_token": unsigned_jwt_token(Value::Object(claims)),
    })
    .to_string()
}

async fn oauth2_callback_response(
    paths: &RuntimeDbPaths,
    registration_id: &str,
    token_payload: &str,
) -> Response {
    let token_server = spawn_single_response_server(200, "application/json", token_payload).await;
    let config = oauth2_runtime_config_for_base_url(
        paths,
        registration_id,
        token_server.url.as_str(),
        "openid email",
    );
    let response = oauth2_callback_response_for_config(&config, registration_id).await;

    token_server
        .join
        .await
        .expect("oauth2 token mock server should finish");

    response
}

fn assert_redirect_location(response: &Response, expected_location: &str) {
    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some(expected_location)
    );
}

#[tokio::test]
async fn router_oauth2_authorization_uses_registration_configured_scope() {
    let paths = new_router_fixture("router-oauth2-authorization-scope-from-config").await;
    seed_router_contract_data(&paths).await;

    let mut env = BTreeMap::new();
    env.insert(
        "KOMGA_CONFIG_DIR".to_string(),
        paths.config_dir.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_DATABASE_FILE".to_string(),
        paths.main_db.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_TASKS_DB_FILE".to_string(),
        paths.tasks_db.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_RUST_RUNTIME_PROFILE".to_string(),
        "snapshot-aligned".to_string(),
    );
    env.insert(
        "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_GITHUB_CLIENT_ID".to_string(),
        "github-client-id".to_string(),
    );
    env.insert(
        "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_GITHUB_CLIENT_SECRET".to_string(),
        "github-client-secret".to_string(),
    );
    env.insert(
        "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_GITHUB_SCOPE".to_string(),
        "read:user".to_string(),
    );
    env.insert(
        "SPRING_SECURITY_OAUTH2_CLIENT_PROVIDER_GITHUB_AUTHORIZATION_URI".to_string(),
        "https://github.example/login/oauth/authorize".to_string(),
    );
    env.insert(
        "SPRING_SECURITY_OAUTH2_CLIENT_PROVIDER_GITHUB_TOKEN_URI".to_string(),
        "https://github.example/login/oauth/access_token".to_string(),
    );

    let config = RuntimeConfig::resolve_with_env(&RuntimeCli::default(), &env)
        .expect("runtime config should resolve oauth2 client env");
    let app = build_router_with_config(&config).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/oauth2/authorization/github")
                .header(header::HOST, "komga.example")
                .body(Body::empty())
                .expect("oauth2 authorization request should build"),
        )
        .await
        .expect("oauth2 authorization request should complete");

    assert_eq!(response.status(), StatusCode::FOUND);
    let location = response
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .expect("oauth2 authorization response should redirect");
    let url = Url::parse(location).expect("oauth2 authorization location should be a valid url");
    let scope = url
        .query_pairs()
        .find_map(|(key, value)| (key == "scope").then_some(value.into_owned()))
        .expect("oauth2 authorization redirect should include scope query");
    assert_eq!(scope, "read:user");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_oauth2_authorization_uses_session_cookie_instead_of_state_cookie() {
    let paths = new_router_fixture("router-oauth2-authorization-session-cookie-state").await;
    seed_router_contract_data(&paths).await;

    let config =
        oauth2_runtime_config_for_base_url(&paths, "github", "https://github.example", "read:user");

    let response = oauth2_authorization_response_for_config(&config, "github").await;

    assert_eq!(response.status(), StatusCode::FOUND);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("oauth2 authorization response should include session cookie");
    assert!(
        set_cookie.contains("KOMGA-SESSION="),
        "oauth2 authorization should use standard session cookie: {set_cookie}"
    );
    assert!(
        !set_cookie.contains("komga-oauth2-state-github="),
        "oauth2 authorization should not emit a dedicated oauth2 state cookie: {set_cookie}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_oauth2_authorization_includes_forwarded_prefix_in_redirect_uri() {
    let paths = new_router_fixture("router-oauth2-authorization-forwarded-prefix").await;
    seed_router_contract_data(&paths).await;

    let config =
        oauth2_runtime_config_for_base_url(&paths, "github", "https://github.example", "read:user");
    let app = build_router_with_config(&config).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/oauth2/authorization/github")
                .header(header::HOST, "komga.example")
                .header("x-forwarded-proto", "https")
                .header("x-forwarded-prefix", "/komga")
                .body(Body::empty())
                .expect("oauth2 authorization forwarded-prefix request should build"),
        )
        .await
        .expect("oauth2 authorization forwarded-prefix request should complete");

    assert_eq!(response.status(), StatusCode::FOUND);
    let location = response
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .expect("oauth2 authorization forwarded-prefix response should redirect");
    let redirect_uri = Url::parse(location)
        .expect("oauth2 authorization forwarded-prefix location should be a valid url")
        .query_pairs()
        .find_map(|(key, value)| (key == "redirect_uri").then_some(value.into_owned()))
        .expect("oauth2 authorization forwarded-prefix redirect should include redirect_uri query");
    assert_eq!(
        redirect_uri,
        "https://komga.example/komga/login/oauth2/code/github"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_oauth2_callback_expires_authorization_state_with_session_lifetime() {
    let paths = new_router_fixture("router-oauth2-callback-session-lifetime-state").await;
    seed_router_contract_data(&paths).await;

    let mut config =
        oauth2_runtime_config_for_base_url(&paths, "github", "https://github.example", "read:user");
    config.session_max_inactive_seconds = 1;
    let app = build_router_with_config(&config).await;

    let authorization_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/oauth2/authorization/github")
                .header(header::HOST, "komga.example")
                .body(Body::empty())
                .expect("oauth2 authorization session-lifetime request should build"),
        )
        .await
        .expect("oauth2 authorization session-lifetime request should complete");
    let authorization_location = authorization_response
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .expect("oauth2 authorization session-lifetime response should redirect");
    let state = Url::parse(authorization_location)
        .expect("oauth2 authorization session-lifetime location should be a valid url")
        .query_pairs()
        .find_map(|(key, value)| (key == "state").then_some(value.into_owned()))
        .expect("oauth2 authorization session-lifetime redirect should include state query");
    let session_cookie = authorization_response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .expect("oauth2 authorization session-lifetime should issue standard session cookie")
        .to_string();

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let callback_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/login/oauth2/code/github?code=valid-code&state={state}"
                ))
                .header(header::HOST, "komga.example")
                .header(header::COOKIE, session_cookie)
                .body(Body::empty())
                .expect("oauth2 callback session-lifetime request should build"),
        )
        .await
        .expect("oauth2 callback session-lifetime request should complete");

    assert_eq!(callback_response.status(), StatusCode::FOUND);
    assert_eq!(
        callback_response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("/login?server_redirect=Y&error=oauth2_state_missing")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_oauth2_callback_requires_email_verified_claim_for_oidc() {
    let paths = new_router_fixture("router-oauth2-callback-requires-email-verified-claim").await;
    seed_router_contract_data(&paths).await;

    let token_payload = oidc_token_payload(Some("admin@example.org"), None);
    let response = oauth2_callback_response(&paths, "oidc", token_payload.as_str()).await;

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("/login?server_redirect=Y&error=ERR_1027")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_oauth2_callback_rejects_unverified_email_for_oidc() {
    let paths = new_router_fixture("router-oauth2-callback-rejects-unverified-email").await;
    seed_router_contract_data(&paths).await;

    let token_server = spawn_single_response_server(
        200,
        "application/json",
        oidc_token_payload(Some("admin@example.org"), Some(false)).as_str(),
    )
    .await;
    let env = oauth2_runtime_env_for_paths(
        &paths,
        "oidc",
        token_server.url.as_str(),
        token_server.url.as_str(),
        Some(format!("{}/oauth/userinfo", token_server.url).as_str()),
        "openid email",
    );
    let config = RuntimeConfig::resolve_with_env(&RuntimeCli::default(), &env)
        .expect("runtime config should resolve oauth2 callback env");
    let response = oauth2_callback_response_for_config(&config, "oidc").await;

    token_server
        .join
        .await
        .expect("oauth2 token mock server should finish");

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("/login?server_redirect=Y&error=ERR_1026")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_oauth2_callback_allows_missing_email_verified_when_disabled_for_oidc() {
    let paths = new_router_fixture("router-oauth2-callback-allows-missing-email-verified").await;
    seed_router_contract_data(&paths).await;

    let token_server = spawn_single_response_server(
        200,
        "application/json",
        oidc_token_payload(Some("admin@example.org"), None).as_str(),
    )
    .await;
    let mut env = oauth2_runtime_env_for_paths(
        &paths,
        "oidc",
        token_server.url.as_str(),
        token_server.url.as_str(),
        Some(format!("{}/oauth/userinfo", token_server.url).as_str()),
        "openid email",
    );
    env.insert(
        "KOMGA_OIDC_EMAIL_VERIFICATION".to_string(),
        "false".to_string(),
    );
    let config = RuntimeConfig::resolve_with_env(&RuntimeCli::default(), &env)
        .expect("runtime config should resolve oauth2 callback env");

    let response = oauth2_callback_response_for_config(&config, "oidc").await;

    token_server
        .join
        .await
        .expect("oauth2 token mock server should finish");

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("/?server_redirect=Y")
    );
    assert!(
        response.headers().get("x-auth-token").is_none(),
        "disabled oidc email verification should still use cookie session semantics"
    );
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("disabled oidc email verification should include session cookie");
    assert!(set_cookie.contains("KOMGA-SESSION="));

    cleanup_router_fixture(paths);
}

pub(crate) async fn verify_oauth2_callback_success_uses_session_cookie_without_auth_token_header() {
    let paths = new_router_fixture("router-oauth2-callback-success-cookie-only").await;
    seed_router_contract_data(&paths).await;

    let token_server = spawn_single_response_server(
        200,
        "application/json",
        oidc_token_payload(Some("admin@example.org"), Some(true)).as_str(),
    )
    .await;
    let env = oauth2_runtime_env_for_paths(
        &paths,
        "oidc",
        token_server.url.as_str(),
        token_server.url.as_str(),
        Some(format!("{}/oauth/userinfo", token_server.url).as_str()),
        "openid email",
    );
    let config = RuntimeConfig::resolve_with_env(&RuntimeCli::default(), &env)
        .expect("runtime config should resolve oauth2 callback env");
    let response = oauth2_callback_response_for_config_with_request_metadata(
        &config,
        "oidc",
        Some(
            "203.0.113.27:43123"
                .parse()
                .expect("oauth2 callback socket addr should parse"),
        ),
        Some("oauth2-contract-agent"),
    )
    .await;

    token_server
        .join
        .await
        .expect("oauth2 token mock server should finish");

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("/?server_redirect=Y")
    );
    assert!(
        response.headers().get("x-auth-token").is_none(),
        "oauth2 callback success should not emit x-auth-token when using cookie session"
    );
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("oauth2 callback success should include session cookie");
    assert!(set_cookie.contains("KOMGA-SESSION="));

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for oauth2 activity assertion");
    let (ip, user_agent, source): (Option<String>, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT IP, USER_AGENT, SOURCE FROM AUTHENTICATION_ACTIVITY WHERE EMAIL = ? ORDER BY DATE_TIME DESC LIMIT 1",
    )
    .bind("admin@example.org")
    .fetch_one(&pool)
    .await
    .expect("oauth2 login should record authentication activity");
    pool.close().await;

    assert_eq!(ip.as_deref(), Some("203.0.113.27"));
    assert_eq!(user_agent.as_deref(), Some("oauth2-contract-agent"));
    assert_eq!(source.as_deref(), Some("OAuth2:oidc"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn oauth2_callback_reuses_komga_session_cookie_after_in_memory_session_refactor() {
    verify_oauth2_callback_success_uses_session_cookie_without_auth_token_header().await;
}

#[tokio::test]
async fn router_oauth2_callback_rejects_token_payload_email_for_non_oidc() {
    let paths = new_router_fixture("router-oauth2-callback-rejects-token-payload-email").await;
    seed_router_contract_data(&paths).await;

    let token_server = spawn_single_response_server(
        200,
        "application/json",
        r#"{"access_token":"oauth-token","email":"admin@example.org"}"#,
    )
    .await;
    let config =
        oauth2_runtime_config_for_base_url(&paths, "generic", token_server.url.as_str(), "profile");

    let response = oauth2_callback_response_for_config(&config, "generic").await;

    token_server
        .join
        .await
        .expect("oauth2 token mock server should finish");

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("/login?server_redirect=Y&error=ERR_1024")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_oauth2_callback_rejects_preferred_username_for_non_oidc() {
    let paths = new_router_fixture("router-oauth2-callback-rejects-preferred-username").await;
    seed_router_contract_data(&paths).await;

    let server = spawn_path_response_server(&[
        (
            "/oauth/token",
            200,
            "application/json",
            r#"{"access_token":"oauth-token"}"#,
        ),
        (
            "/oauth/userinfo",
            200,
            "application/json",
            r#"{"preferred_username":"admin@example.org"}"#,
        ),
    ])
    .await;
    let config =
        oauth2_runtime_config_for_base_url(&paths, "generic", server.url.as_str(), "profile");

    let response = oauth2_callback_response_for_config(&config, "generic").await;

    server
        .join
        .await
        .expect("oauth2 path response server should finish");

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("/login?server_redirect=Y&error=ERR_1024")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_github_oauth2_callback_rejects_unverified_primary_email() {
    let paths = new_router_fixture("router-github-oauth2-callback-rejects-unverified-email").await;
    seed_router_contract_data(&paths).await;

    let server = spawn_path_response_server(&[
        (
            "/oauth/token",
            200,
            "application/json",
            r#"{"access_token":"github-token"}"#,
        ),
        (
            "/oauth/userinfo",
            200,
            "application/json",
            r#"{"id":1,"login":"komga-user"}"#,
        ),
        (
            "/oauth/userinfo/emails",
            200,
            "application/json",
            r#"[{"email":"admin@example.org","primary":true,"verified":false}]"#,
        ),
    ])
    .await;
    let config =
        oauth2_runtime_config_for_base_url(&paths, "github", server.url.as_str(), "user:email");

    let response = oauth2_callback_response_for_config(&config, "github").await;

    server
        .join
        .await
        .expect("oauth2 path response server should finish");

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("/login?server_redirect=Y&error=ERR_1024")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_github_oauth2_callback_accepts_verified_primary_email() {
    let paths = new_router_fixture("router-github-oauth2-callback-accepts-verified-email").await;
    seed_router_contract_data(&paths).await;

    let server = spawn_path_response_server(&[
        (
            "/oauth/token",
            200,
            "application/json",
            r#"{"access_token":"github-token"}"#,
        ),
        (
            "/oauth/userinfo",
            200,
            "application/json",
            r#"{"id":1,"login":"komga-user"}"#,
        ),
        (
            "/oauth/userinfo/emails",
            200,
            "application/json",
            r#"[{"email":"admin@example.org","primary":true,"verified":true}]"#,
        ),
    ])
    .await;
    let config =
        oauth2_runtime_config_for_base_url(&paths, "github", server.url.as_str(), "user:email");

    let response = oauth2_callback_response_for_config(&config, "github").await;

    server
        .join
        .await
        .expect("oauth2 path response server should finish");

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("/?server_redirect=Y")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_oauth2_callback_respects_komga_oauth2_account_creation_config() {
    let paths = new_router_fixture("router-oauth2-callback-account-creation-config").await;
    seed_router_contract_data(&paths).await;
    std::fs::write(
        paths.config_dir.join("application.yml"),
        "komga:\n  oauth2AccountCreation: true\n",
    )
    .expect("oauth2 callback fixture should write application.yml");

    let server = spawn_path_response_server(&[
        (
            "/oauth/token",
            200,
            "application/json",
            r#"{"access_token":"oauth-token"}"#,
        ),
        (
            "/oauth/userinfo",
            200,
            "application/json",
            r#"{"email":"new-oauth-user@example.org"}"#,
        ),
    ])
    .await;
    let config =
        oauth2_runtime_config_for_base_url(&paths, "generic", server.url.as_str(), "profile");

    let response = oauth2_callback_response_for_config(&config, "generic").await;

    server
        .join
        .await
        .expect("oauth2 path response server should finish");

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("/?server_redirect=Y")
    );

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for oauth2 account creation verification");
    let shared_all_libraries = sqlx::query_scalar::<_, i64>(
        "SELECT SHARED_ALL_LIBRARIES FROM USER WHERE lower(EMAIL) = lower(?) LIMIT 1",
    )
    .bind("new-oauth-user@example.org")
    .fetch_one(&pool)
    .await
    .expect("oauth2 account creation should persist a new user");
    pool.close().await;

    assert_eq!(shared_all_libraries, 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_oauth2_callback_respects_user_info_endpoint_configuration() {
    let configured_paths =
        new_router_fixture("router-oauth2-callback-explicit-user-info-uri").await;
    seed_router_contract_data(&configured_paths).await;

    let configured_server = spawn_path_response_server(&[
        (
            "/oauth/token/custom",
            200,
            "application/json",
            r#"{"access_token":"oauth-token"}"#,
        ),
        (
            "/api/v3/profile",
            200,
            "application/json",
            r#"{"email":"admin@example.org"}"#,
        ),
    ])
    .await;
    let configured_env = oauth2_runtime_env_for_paths(
        &configured_paths,
        "custom",
        format!("{}/oauth/authorize/custom", configured_server.url).as_str(),
        format!("{}/oauth/token/custom", configured_server.url).as_str(),
        Some(format!("{}/api/v3/profile", configured_server.url).as_str()),
        "profile email",
    );
    let configured_config =
        RuntimeConfig::resolve_with_env(&RuntimeCli::default(), &configured_env)
            .expect("runtime config should resolve custom oauth2 user-info-uri env");

    let configured_response =
        oauth2_callback_response_for_config(&configured_config, "custom").await;

    configured_server
        .join
        .await
        .expect("oauth2 path response server should finish");
    assert_redirect_location(&configured_response, "/?server_redirect=Y");

    cleanup_router_fixture(configured_paths);

    let omitted_paths = new_router_fixture("router-oauth2-callback-no-userinfo-guessing").await;
    seed_router_contract_data(&omitted_paths).await;

    let omitted_server = spawn_path_response_server(&[
        (
            "/oauth/token",
            200,
            "application/json",
            r#"{"access_token":"oauth-token"}"#,
        ),
        (
            "/oauth/userinfo",
            200,
            "application/json",
            r#"{"email":"admin@example.org"}"#,
        ),
    ])
    .await;
    let omitted_env = oauth2_runtime_env_for_paths(
        &omitted_paths,
        "generic",
        format!("{}/oauth/authorize", omitted_server.url).as_str(),
        format!("{}/oauth/token", omitted_server.url).as_str(),
        None,
        "profile email",
    );
    let omitted_config = RuntimeConfig::resolve_with_env(&RuntimeCli::default(), &omitted_env)
        .expect("runtime config should resolve oauth2 callback env without user_info_uri");

    let omitted_response = oauth2_callback_response_for_config(&omitted_config, "generic").await;

    omitted_server
        .join
        .await
        .expect("oauth2 path response server should finish");
    assert_redirect_location(&omitted_response, "/login?server_redirect=Y&error=ERR_1024");

    cleanup_router_fixture(omitted_paths);
}

#[tokio::test]
async fn router_oidc_callback_accepts_id_token_claims_without_user_info_endpoint() {
    let paths = new_router_fixture("router-oidc-callback-id-token-without-userinfo").await;
    seed_router_contract_data(&paths).await;

    let token_payload = oidc_token_payload(Some("admin@example.org"), Some(true));
    let token_server =
        spawn_single_response_server(200, "application/json", token_payload.as_str()).await;
    let env = oauth2_runtime_env_for_paths(
        &paths,
        "oidc",
        token_server.url.as_str(),
        token_server.url.as_str(),
        None,
        "openid email",
    );
    let config = RuntimeConfig::resolve_with_env(&RuntimeCli::default(), &env)
        .expect("runtime config should resolve oidc callback env without user_info_uri");

    let response = oauth2_callback_response_for_config(&config, "oidc").await;

    token_server
        .join
        .await
        .expect("oauth2 token mock server should finish");

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("/?server_redirect=Y")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_oauth2_callback_records_failure_activity() {
    let paths = new_router_fixture("router-oauth2-callback-records-failure-activity").await;
    seed_router_contract_data(&paths).await;

    let config = oauth2_runtime_config_for_base_url(
        &paths,
        "oidc",
        "https://issuer.example",
        "openid email",
    );
    let app = build_router_with_config(&config).await;

    let authorization_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/oauth2/authorization/oidc")
                .header(header::HOST, "komga.example")
                .body(Body::empty())
                .expect("oauth2 authorization request should build for failure audit"),
        )
        .await
        .expect("oauth2 authorization request should complete for failure audit");
    let authorization_location = authorization_response
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .expect("oauth2 authorization should redirect for failure audit");
    let state = Url::parse(authorization_location)
        .expect("oauth2 authorization location should be a valid url for failure audit")
        .query_pairs()
        .find_map(|(key, value)| (key == "state").then_some(value.into_owned()))
        .expect("oauth2 authorization redirect should include state for failure audit");
    let session_cookie = authorization_response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .expect("oauth2 authorization should issue session cookie for failure audit")
        .to_string();

    let mut callback_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/login/oauth2/code/oidc?error=access_denied&state={state}"
        ))
        .header(header::HOST, "komga.example")
        .header(header::COOKIE, session_cookie)
        .header(header::USER_AGENT, "oauth2-failure-agent")
        .body(Body::empty())
        .expect("oauth2 callback request should build for failure audit");
    callback_request.extensions_mut().insert(ConnectInfo(
        "203.0.113.88:41000"
            .parse::<SocketAddr>()
            .expect("oauth2 failure audit socket addr should parse"),
    ));

    let response = app
        .oneshot(callback_request)
        .await
        .expect("oauth2 callback request should complete for failure audit");

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("/login?server_redirect=Y&error=access_denied")
    );

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for oauth2 failure activity verification");
    let (success, error, source, ip, user_agent, email): AuthenticationActivityRow = sqlx::query_as(
        "SELECT SUCCESS, ERROR, SOURCE, IP, USER_AGENT, EMAIL FROM AUTHENTICATION_ACTIVITY ORDER BY DATE_TIME DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .expect("oauth2 callback failure should record authentication activity");
    pool.close().await;

    assert!(!success);
    assert_eq!(error.as_deref(), Some("access_denied"));
    assert_eq!(source.as_deref(), Some("OAuth2:oidc"));
    assert_eq!(ip.as_deref(), Some("203.0.113.88"));
    assert_eq!(user_agent.as_deref(), Some("oauth2-failure-agent"));
    assert_eq!(email, None);

    cleanup_router_fixture(paths);
}
