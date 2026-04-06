use super::*;
use axum::response::Response;
use komga_rust::config::{RuntimeCli, RuntimeConfig};
use reqwest::Url;
use std::collections::BTreeMap;

fn oauth2_runtime_env_for_paths(
    paths: &RuntimeDbPaths,
    registration_id: &str,
    authorization_uri: &str,
    token_uri: &str,
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
        scopes,
    );
    RuntimeConfig::resolve_with_env(&RuntimeCli::default(), &env)
        .expect("runtime config should resolve oauth2 callback env")
}

async fn oauth2_authorization_response_for_config(
    config: &RuntimeConfig,
    registration_id: &str,
) -> Response {
    let app = build_router_with_config(config);
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
    let app = build_router_with_config(config);
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

    app.oneshot(
        Request::builder()
            .method("GET")
            .uri(format!(
                "/login/oauth2/code/{registration_id}?code=valid-code&state={state}"
            ))
            .header(header::HOST, "komga.example")
            .header(header::COOKIE, session_cookie)
            .body(Body::empty())
            .expect("oauth2 callback request should build"),
    )
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
    let app = build_router_with_config(&config);

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
async fn router_oauth2_callback_requires_email_verified_claim_for_oidc() {
    let paths = new_router_fixture("router-oauth2-callback-requires-email-verified-claim").await;
    seed_router_contract_data(&paths).await;

    let response = oauth2_callback_response(
        &paths,
        "oidc",
        r#"{"access_token":"oidc-token","email":"admin@example.org"}"#,
    )
    .await;

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
        r#"{"access_token":"oidc-token","email":"admin@example.org","email_verified":false}"#,
    )
    .await;
    let env = oauth2_runtime_env_for_paths(
        &paths,
        "oidc",
        token_server.url.as_str(),
        token_server.url.as_str(),
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
        r#"{"access_token":"oidc-token","email":"admin@example.org"}"#,
    )
    .await;
    let mut env = oauth2_runtime_env_for_paths(
        &paths,
        "oidc",
        token_server.url.as_str(),
        token_server.url.as_str(),
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

#[tokio::test]
async fn router_oauth2_callback_success_uses_session_cookie_without_auth_token_header() {
    let paths = new_router_fixture("router-oauth2-callback-success-cookie-only").await;
    seed_router_contract_data(&paths).await;

    let token_server = spawn_single_response_server(
        200,
        "application/json",
        r#"{"access_token":"oidc-token","email":"admin@example.org","email_verified":true}"#,
    )
    .await;
    let env = oauth2_runtime_env_for_paths(
        &paths,
        "oidc",
        token_server.url.as_str(),
        token_server.url.as_str(),
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

    cleanup_router_fixture(paths);
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

    cleanup_router_fixture(paths);
}
