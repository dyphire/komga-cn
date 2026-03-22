use super::*;

#[test]
fn setup_placeholder_resolution_uses_env_driven_basic_auth() {
    let env_key = "KOMGA_COMPAT_BASIC_AUTH";
    let original = std::env::var_os(env_key);
    unsafe {
        std::env::set_var(env_key, "dXNlckBleGFtcGxlLm9yZzp0b2tlbg==");
    }

    let headers = BTreeMap::from([
        (
            "Authorization".to_string(),
            "Basic ${KOMGA_COMPAT_BASIC_AUTH}".to_string(),
        ),
        ("X-Auth-Token".to_string(), "".to_string()),
    ]);

    let resolved =
        resolve_headers(&Some(headers), &BTreeMap::new()).expect("headers should resolve");

    if let Some(value) = original {
        unsafe {
            std::env::set_var(env_key, value);
        }
    } else {
        unsafe {
            std::env::remove_var(env_key);
        }
    }

    let resolved = resolved.expect("headers should be present");

    assert_eq!(
        resolved.get("Authorization"),
        Some(&"Basic dXNlckBleGFtcGxlLm9yZzp0b2tlbg==".to_string())
    );
    assert_eq!(resolved.get("X-Auth-Token"), Some(&"".to_string()));
}

#[test]
fn apply_setup_steps_extracts_session_token_from_set_cookie_when_x_auth_token_is_missing() {
    let listener = std::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .expect("bind test listener");
    let address = listener.local_addr().expect("listener address");
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("accept request");
        let mut buffer = [0u8; 1024];
        let _ = std::io::Read::read(&mut socket, &mut buffer).expect("read request");
        let response = concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Type: application/json\r\n",
            "Set-Cookie: KOMGA-SESSION=java-cookie-token; Path=/; HttpOnly; SameSite=Lax\r\n",
            "Content-Length: 2\r\n",
            "Connection: close\r\n",
            "\r\n",
            "{}"
        );
        std::io::Write::write_all(&mut socket, response.as_bytes()).expect("write response");
    });

    let client = Client::builder()
        .build()
        .expect("reqwest client should build");
    let steps = vec![komga_compat_testkit::cases::SetupStep {
        name: "login".to_string(),
        method: "GET".to_string(),
        path: "/api/v2/users/me".to_string(),
        headers: Some(BTreeMap::from([
            (
                "Authorization".to_string(),
                "Basic YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=".to_string(),
            ),
            ("X-Auth-Token".to_string(), "".to_string()),
        ])),
        extract_headers: Some(BTreeMap::from([(
            "SESSION_TOKEN".to_string(),
            "X-Auth-Token".to_string(),
        )])),
    }];
    let mut vars = BTreeMap::new();

    apply_setup_steps(&client, &format!("http://{address}"), &steps, &mut vars)
        .expect("setup step should fall back to cookie session extraction");

    server.join().expect("server should join");

    assert_eq!(
        vars.get("SESSION_TOKEN"),
        Some(&"java-cookie-token".to_string())
    );
}

#[test]
fn apply_setup_steps_reports_non_success_status_before_missing_header() {
    let listener = std::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .expect("bind test listener");
    let address = listener.local_addr().expect("listener address");
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("accept request");
        let mut buffer = [0u8; 1024];
        let _ = std::io::Read::read(&mut socket, &mut buffer).expect("read request");
        let response = concat!(
            "HTTP/1.1 401 Unauthorized\r\n",
            "Content-Type: application/json\r\n",
            "Content-Length: 24\r\n",
            "Connection: close\r\n",
            "\r\n",
            "{\"error\":\"Unauthorized\"}"
        );
        std::io::Write::write_all(&mut socket, response.as_bytes()).expect("write response");
    });

    let client = Client::builder()
        .build()
        .expect("reqwest client should build");
    let steps = vec![komga_compat_testkit::cases::SetupStep {
        name: "login".to_string(),
        method: "GET".to_string(),
        path: "/api/v2/users/me".to_string(),
        headers: Some(BTreeMap::from([(
            "Authorization".to_string(),
            "Basic YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=".to_string(),
        )])),
        extract_headers: Some(BTreeMap::from([(
            "SESSION_TOKEN".to_string(),
            "X-Auth-Token".to_string(),
        )])),
    }];
    let mut vars = BTreeMap::new();

    let error = apply_setup_steps(&client, &format!("http://{address}"), &steps, &mut vars)
        .expect_err("non-success setup response should fail explicitly");

    server.join().expect("server should join");

    assert!(
        error
            .to_string()
            .contains("setup step 'login' returned HTTP 401"),
        "unexpected error: {error:#}"
    );
}

#[test]
#[ignore = "requires local Java server on KOMGA_COMPAT_JAVA_BASE_URL"]
fn apply_setup_steps_extracts_session_token_from_live_java_cookie_bootstrap() {
    let client = Client::builder()
        .build()
        .expect("reqwest client should build");
    let steps = vec![komga_compat_testkit::cases::SetupStep {
        name: "login".to_string(),
        method: "GET".to_string(),
        path: "/api/v2/users/me".to_string(),
        headers: Some(BTreeMap::from([
            (
                "Authorization".to_string(),
                "Basic ${KOMGA_COMPAT_BASIC_AUTH_ADMIN}".to_string(),
            ),
            ("X-Auth-Token".to_string(), "".to_string()),
        ])),
        extract_headers: Some(BTreeMap::from([(
            "SESSION_TOKEN".to_string(),
            "X-Auth-Token".to_string(),
        )])),
    }];
    let mut vars = BTreeMap::new();
    let base_url = std::env::var("KOMGA_COMPAT_JAVA_BASE_URL")
        .expect("missing KOMGA_COMPAT_JAVA_BASE_URL for live java bootstrap test");

    apply_setup_steps(&client, &base_url, &steps, &mut vars)
        .expect("live java setup step should extract a session token");

    assert!(
        vars.get("SESSION_TOKEN")
            .is_some_and(|token| !token.trim().is_empty()),
        "expected non-empty SESSION_TOKEN after live java bootstrap"
    );
}
