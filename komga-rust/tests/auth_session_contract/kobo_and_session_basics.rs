use super::*;
use axum::response::Response;
use base64::{Engine as _, engine::general_purpose::STANDARD};
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
async fn router_users_me_basic_auth_defaults_to_session_cookie_without_auth_token_header() {
    let paths = new_router_fixture("router-users-me-basic-defaults-to-cookie").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let basic_token = STANDARD.encode("admin@example.org:router-contract-admin-123");

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header(header::AUTHORIZATION, format!("Basic {basic_token}"))
                .body(Body::empty())
                .expect("users/me basic request should build"),
        )
        .await
        .expect("users/me basic request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers().get("x-auth-token").is_none(),
        "plain basic auth should not emit x-auth-token unless requested"
    );
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("users/me basic response should include session cookie");
    assert!(set_cookie.contains("KOMGA-SESSION="));

    let payload = response_json(response).await;
    assert_eq!(
        payload.get("email"),
        Some(&Value::String("admin@example.org".to_string()))
    );

    cleanup_router_fixture(paths);
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
async fn router_claim_rejects_invalid_email_header() {
    let paths = new_router_fixture("router-claim-invalid-email-header").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/claim")
                .header("X-Komga-Email", "user@domain")
                .header("X-Komga-Password", "password")
                .body(Body::empty())
                .expect("claim request should build"),
        )
        .await
        .expect("claim request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_claim_returns_kotlin_user_dto_shape_and_roles() {
    let paths = new_router_fixture("router-claim-kotlin-user-dto-shape").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/claim")
                .header("X-Komga-Email", "claim-admin@example.org")
                .header("X-Komga-Password", "password")
                .body(Body::empty())
                .expect("claim request should build"),
        )
        .await
        .expect("claim request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("email"),
        Some(&Value::String("claim-admin@example.org".to_string()))
    );

    let roles = payload
        .get("roles")
        .and_then(Value::as_array)
        .expect("claim response should expose roles array")
        .iter()
        .filter_map(Value::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        roles,
        std::collections::BTreeSet::from([
            "ADMIN",
            "FILE_DOWNLOAD",
            "KOBO_SYNC",
            "KOREADER_SYNC",
            "PAGE_STREAMING",
            "USER",
        ])
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_claim_returns_kotlin_already_claimed_message() {
    let paths = new_router_fixture("router-claim-already-claimed-message").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/claim")
                .header("X-Komga-Email", "first-claim@example.org")
                .header("X-Komga-Password", "password")
                .body(Body::empty())
                .expect("initial claim request should build"),
        )
        .await
        .expect("initial claim request should complete");
    assert_eq!(first_response.status(), StatusCode::OK);

    let second_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/claim")
                .header("X-Komga-Email", "second-claim@example.org")
                .header("X-Komga-Password", "password")
                .body(Body::empty())
                .expect("already-claimed request should build"),
        )
        .await
        .expect("already-claimed request should complete");

    assert_eq!(second_response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(second_response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Bad Request".to_string()))
    );
    assert_eq!(
        payload.get("message"),
        Some(&Value::String(
            "This server has already been claimed".to_string()
        ))
    );
    assert_eq!(payload.get("status"), Some(&Value::from(400)));

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

#[tokio::test]
async fn router_kobo_initialization_returns_scoped_api_token_header() {
    let paths = new_router_fixture("router-kobo-initialization-api-token").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/initialization")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo initialization request should build"),
        )
        .await
        .expect("kobo initialization request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let api_token = response
        .headers()
        .get("x-kobo-apitoken")
        .and_then(|value| value.to_str().ok())
        .expect("kobo initialization response should include x-kobo-apitoken");
    assert!(api_token.starts_with("KOMGA."));
    assert_ne!(api_token, "e30=");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_ping_does_not_accept_web_auth_fallback_when_path_token_is_invalid() {
    let paths = new_router_fixture("router-kobo-ping-path-token-only-auth").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/invalid-token/ping")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo ping request should build"),
        )
        .await
        .expect("kobo ping request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_ping_rejects_path_tokens_with_characters_outside_kotlin_regex() {
    let paths = new_router_fixture("router-kobo-ping-token-char-constraint").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-user",
        "kobo@example.org",
        "router-contract-kobo-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "bad.token", "kobo-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/bad.token/ping")
                .body(Body::empty())
                .expect("kobo ping constrained token request should build"),
        )
        .await
        .expect("kobo ping constrained token request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_login_set_cookie_returns_session_cookie_for_header_session() {
    let paths = new_router_fixture("router-login-set-cookie-session-header").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/login/set-cookie")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("login/set-cookie request should build"),
        )
        .await
        .expect("login/set-cookie request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("login/set-cookie should return set-cookie header");
    assert!(set_cookie.starts_with(&format!("KOMGA-SESSION={auth_token}")));
    assert!(set_cookie.contains("Path=/"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_logout_get_clears_session_cookie() {
    let paths = new_router_fixture("router-logout-get-clears-session-cookie").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/logout")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("logout get request should build"),
        )
        .await
        .expect("logout get request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let cookies = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .collect::<Vec<_>>();
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.contains("KOMGA-SESSION=;"))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_ping_returns_forbidden_for_valid_token_without_kobo_sync_role() {
    let paths = new_router_fixture("router-kobo-ping-forbidden-without-kobo-role").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "plain-user",
        "plain@example.org",
        "router-contract-plain-123",
        0,
        &["USER"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "plain-kobo-token", "plain-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/plain-kobo-token/ping")
                .body(Body::empty())
                .expect("kobo ping forbidden request should build"),
        )
        .await
        .expect("kobo ping forbidden request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_catch_all_returns_internal_error_for_non_json_upstream_body() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server = spawn_single_response_server(200, "text/plain", "plain-text-body").await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-kobo-catch-all-non-json-body").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-proxy-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/validkobotoken/v1/test")
                .body(Body::empty())
                .expect("kobo catch-all request should build"),
        )
        .await
        .expect("kobo catch-all request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo proxy mock server should finish");
}

#[tokio::test]
async fn router_kobo_catch_all_preserves_non_success_status_for_non_json_upstream_body() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server = spawn_single_response_server(503, "text/plain", "upstream error text").await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-kobo-catch-all-non-json-error-body").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-proxy-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/validkobotoken/v1/test")
                .body(Body::empty())
                .expect("kobo catch-all non-success request should build"),
        )
        .await
        .expect("kobo catch-all non-success request should complete");

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo proxy non-success mock server should finish");
}

#[tokio::test]
async fn router_kobo_catch_all_does_not_passthrough_error_body_or_kobo_headers() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server = spawn_single_response_server_with_headers(
        503,
        "text/plain",
        "upstream error text",
        &[("x-kobo-test", "1")],
    )
    .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-kobo-catch-all-no-error-body-passthrough").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-proxy-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/validkobotoken/v1/test")
                .body(Body::empty())
                .expect("kobo catch-all no passthrough request should build"),
        )
        .await
        .expect("kobo catch-all no passthrough request should complete");

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(response.headers().get("x-kobo-test").is_none());
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("kobo catch-all error body should be readable");
    assert!(body.is_empty());

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo proxy no-passthrough mock server should finish");
}

#[tokio::test]
async fn router_kobo_catch_all_does_not_passthrough_json_error_body_or_kobo_headers() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server = spawn_single_response_server_with_headers(
        503,
        "application/json",
        r#"{"error":"upstream-failure"}"#,
        &[("x-kobo-test", "1")],
    )
    .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-kobo-catch-all-no-json-error-passthrough").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-proxy-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/validkobotoken/v1/test")
                .body(Body::empty())
                .expect("kobo catch-all json error request should build"),
        )
        .await
        .expect("kobo catch-all json error request should complete");

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(response.headers().get("x-kobo-test").is_none());
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("kobo catch-all json error body should be readable");
    assert!(body.is_empty());

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo proxy json-error mock server should finish");
}

#[tokio::test]
async fn router_kobo_catch_all_returns_internal_error_for_transport_failure() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", "http://127.0.0.1:1");
    }

    let paths = new_router_fixture("router-kobo-catch-all-transport-failure").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-proxy-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/validkobotoken/v1/test")
                .body(Body::empty())
                .expect("kobo catch-all transport failure request should build"),
        )
        .await
        .expect("kobo catch-all transport failure request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
}

#[tokio::test]
async fn router_kobo_catch_all_preserves_success_status_for_empty_body() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server = spawn_single_response_server(204, "application/json", "").await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-kobo-catch-all-empty-success-body").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-proxy-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/validkobotoken/v1/test")
                .body(Body::empty())
                .expect("kobo catch-all empty success request should build"),
        )
        .await
        .expect("kobo catch-all empty success request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo proxy empty-success mock server should finish");
}

#[tokio::test]
async fn router_kobo_catch_all_put_returns_bad_request_for_invalid_json_body() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", "http://127.0.0.1:1");
    }

    let paths = new_router_fixture("router-kobo-catch-all-put-invalid-json-body").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-proxy-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/kobo/validkobotoken/v1/test")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"broken": }"#))
                .expect("kobo catch-all invalid json put request should build"),
        )
        .await
        .expect("kobo catch-all invalid json put request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
}

#[tokio::test]
async fn router_kobo_catch_all_put_returns_unsupported_media_type_for_text_plain_body() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", "http://127.0.0.1:1");
    }

    let paths = new_router_fixture("router-kobo-catch-all-put-text-plain-body").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-proxy-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/kobo/validkobotoken/v1/test")
                .header(header::CONTENT_TYPE, "text/plain")
                .body(Body::from("plain-text-body"))
                .expect("kobo catch-all text/plain put request should build"),
        )
        .await
        .expect("kobo catch-all text/plain put request should complete");

    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
}

#[tokio::test]
async fn router_kobo_catch_all_put_returns_bad_request_for_malformed_xml_body() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", "http://127.0.0.1:1");
    }

    let paths = new_router_fixture("router-kobo-catch-all-put-malformed-xml-body").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-proxy-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/kobo/validkobotoken/v1/test")
                .header(header::CONTENT_TYPE, "application/xml")
                .body(Body::from("<root><broken></root>"))
                .expect("kobo catch-all malformed xml put request should build"),
        )
        .await
        .expect("kobo catch-all malformed xml put request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
}

#[tokio::test]
async fn router_kobo_catch_all_put_reserializes_json_request_body_before_proxying() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server = spawn_request_body_echo_server().await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-kobo-catch-all-put-json-reserialize").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "kobo-proxy-user",
        "kobo-proxy@example.org",
        "router-contract-kobo-proxy-123",
        0,
        &["USER", "KOBO_SYNC"],
    )
    .await;
    seed_kobo_sync_api_key(&paths, "validkobotoken", "kobo-proxy-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/kobo/validkobotoken/v1/test")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{\n  \"key\" : 1,\n  \"items\" : [ 2, 3 ]\n}"))
                .expect("kobo catch-all json reserialize put request should build"),
        )
        .await
        .expect("kobo catch-all json reserialize put request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let received = payload
        .get("received")
        .and_then(Value::as_str)
        .expect("kobo catch-all echo response should include received body");
    assert!(!received.contains(' '));
    assert!(!received.contains('\n'));
    let reparsed: Value = serde_json::from_str(received)
        .expect("kobo catch-all echoed request body should remain valid json");
    assert_eq!(reparsed, json!({"key":1,"items":[2,3]}));

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo proxy request-body echo server should finish");
}
