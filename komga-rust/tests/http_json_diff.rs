mod compat;

use compat::cases::HarnessConfig;
use compat::diff_writer::{compare_responses, write_diff_report};
use compat::normalize::{normalize_headers, normalize_json_body};
use compat::runtime::{apply_setup_steps, resolve_headers};
use compat::{ComparisonMode, NormalizedBody, NormalizedResponse};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[test]
fn p0_cases_configuration_loads() {
    let config = HarnessConfig::load_default().expect("default compat cases should load");
    let case_ids: Vec<&str> = config.cases.iter().map(|it| it.id.as_str()).collect();
    let library_case = config
        .cases
        .iter()
        .find(|it| it.id == "KOMGA-P0-LIB-01")
        .expect("library case should exist");
    let latest_case = config
        .cases
        .iter()
        .find(|it| it.id == "KOMGA-P0-BOOKS-LATEST-01")
        .expect("books latest case should exist");
    let set_cookie_case = config
        .cases
        .iter()
        .find(|it| it.id == "P0-AUTH-SETCOOKIE")
        .expect("set-cookie case should exist");
    let remember_me_case = config
        .cases
        .iter()
        .find(|it| it.id == "P0-AUTH-REMEMBERME")
        .expect("remember-me case should exist");
    let pages_case = config
        .cases
        .iter()
        .find(|it| it.id == "KOMGA-P0-BK-PAGES-01")
        .expect("book pages case should exist");
    let thumbnail_case = config
        .cases
        .iter()
        .find(|it| it.id == "KOMGA-P0-BK-THUMBNAIL-01")
        .expect("book thumbnail case should exist");
    let book_thumbnail_case = config
        .cases
        .iter()
        .find(|it| it.id == "KOMGA-P0-BK-THUMBNAIL-BOOK-01")
        .expect("book cover thumbnail case should exist");
    let read_progress_case = config
        .cases
        .iter()
        .find(|it| it.id == "KOMGA-P0-BK-READ-PROGRESS-01")
        .expect("book read-progress case should exist");
    let catalog_case = config
        .cases
        .iter()
        .find(|it| it.id == "P0-OPDS-V2-CATALOG-UNAUTH")
        .expect("opds catalog case should exist");
    let auth_case = config
        .cases
        .iter()
        .find(|it| it.id == "P0-OPDS-V2-AUTH-DOCUMENT")
        .expect("opds auth document case should exist");
    let setup = library_case
        .setup
        .as_ref()
        .expect("library case should define setup");
    let login = &setup[0];
    let set_cookie_setup = set_cookie_case
        .setup
        .as_ref()
        .expect("set-cookie case should define setup");
    let set_cookie_login = &set_cookie_setup[0];

    assert_eq!(config.output_dir, "target/compat-diff");
    assert!(case_ids.contains(&"KOMGA-P0-LIB-01"));
    assert!(case_ids.contains(&"KOMGA-P0-SERIES-01"));
    assert!(case_ids.contains(&"KOMGA-P0-BOOKS-LIST-01"));
    assert!(case_ids.contains(&"KOMGA-P0-BOOKS-LATEST-01"));
    assert!(case_ids.contains(&"KOMGA-P0-BK-PAGES-01"));
    assert!(case_ids.contains(&"KOMGA-P0-BK-THUMBNAIL-01"));
    assert!(case_ids.contains(&"KOMGA-P0-BK-THUMBNAIL-BOOK-01"));
    assert!(case_ids.contains(&"P0-AUTH-SETCOOKIE"));
    assert!(case_ids.contains(&"P0-AUTH-REMEMBERME"));
    assert_eq!(setup.len(), 1);
    assert_eq!(login.name, "login");
    assert_eq!(login.method, "GET");
    assert_eq!(login.path, "/api/v2/users/me");
    assert_eq!(
        login
            .headers
            .as_ref()
            .and_then(|headers| headers.get("Authorization")),
        Some(&"Basic ${KOMGA_COMPAT_BASIC_AUTH}".to_string())
    );
    assert_eq!(
        login
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Auth-Token")),
        Some(&"".to_string())
    );
    assert_eq!(
        login
            .extract_headers
            .as_ref()
            .and_then(|headers| headers.get("SESSION_TOKEN")),
        Some(&"X-Auth-Token".to_string())
    );
    assert_eq!(
        library_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Auth-Token")),
        Some(&"${SESSION_TOKEN}".to_string())
    );
    assert_eq!(latest_case.method, "GET");
    assert_eq!(latest_case.path, "/api/v1/books/latest?unpaged=true");
    assert_eq!(latest_case.comparison, ComparisonMode::Json);
    assert_eq!(
        latest_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Auth-Token")),
        Some(&"${SESSION_TOKEN}".to_string())
    );
    assert_eq!(
        set_cookie_login
            .headers
            .as_ref()
            .and_then(|headers| headers.get("Authorization")),
        Some(&"Basic ${KOMGA_COMPAT_BASIC_AUTH}".to_string())
    );
    assert_eq!(remember_me_case.method, "GET");
    assert_eq!(remember_me_case.path, "/api/v2/users/me?remember-me=true");
    assert_eq!(remember_me_case.comparison, ComparisonMode::Json);
    assert_eq!(
        remember_me_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("Authorization")),
        Some(&"Basic ${KOMGA_COMPAT_BASIC_AUTH}".to_string())
    );
    assert_eq!(pages_case.method, "GET");
    assert_eq!(pages_case.path, "/api/v1/books/book-1/pages");
    assert_eq!(pages_case.comparison, ComparisonMode::Json);
    assert_eq!(
        pages_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Auth-Token")),
        Some(&"${SESSION_TOKEN}".to_string())
    );
    assert_eq!(thumbnail_case.method, "GET");
    assert_eq!(thumbnail_case.path, "/api/v1/books/book-1/pages/1/thumbnail");
    assert_eq!(thumbnail_case.comparison, ComparisonMode::BinaryMetadata);
    assert_eq!(
        thumbnail_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Auth-Token")),
        Some(&"${SESSION_TOKEN}".to_string())
    );
    assert_eq!(book_thumbnail_case.method, "GET");
    assert_eq!(book_thumbnail_case.path, "/api/v1/books/book-1/thumbnail");
    assert_eq!(book_thumbnail_case.comparison, ComparisonMode::BinaryMetadata);
    assert_eq!(
        book_thumbnail_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Auth-Token")),
        Some(&"${SESSION_TOKEN}".to_string())
    );
    assert_eq!(read_progress_case.method, "PATCH");
    assert_eq!(read_progress_case.path, "/api/v1/books/book-1/read-progress");
    assert_eq!(read_progress_case.comparison, ComparisonMode::Json);
    assert_eq!(read_progress_case.body.as_deref(), Some(r#"{"completed":true}"#));
    assert_eq!(
        read_progress_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("Content-Type")),
        Some(&"application/json".to_string())
    );
    assert_eq!(
        read_progress_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Auth-Token")),
        Some(&"${SESSION_TOKEN}".to_string())
    );
    assert_eq!(catalog_case.method, "GET");
    assert_eq!(catalog_case.path, "/opds/v2/catalog");
    assert_eq!(catalog_case.comparison, ComparisonMode::Json);
    assert!(catalog_case.headers.is_none());
    assert_eq!(auth_case.method, "GET");
    assert_eq!(auth_case.path, "/opds/v2/auth");
    assert_eq!(auth_case.comparison, ComparisonMode::Json);
    assert!(auth_case.headers.is_none());
}

#[test]
fn json_normalization_is_object_order_insensitive() {
    let left = normalize_json_body(r#"{"b":2,"a":1}"#, "http://127.0.0.1:0")
        .expect("left json should parse");
    let right = normalize_json_body(r#"{"a":1,"b":2}"#, "http://127.0.0.1:0")
        .expect("right json should parse");

    assert_eq!(left, right);
}

#[tokio::test]
async fn execute_case_async_normalizes_service_local_absolute_urls() {
    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind test listener");
    let address = listener.local_addr().expect("listener address");

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept request");
        let mut buffer = [0u8; 1024];
        let _ = socket.read(&mut buffer).await.expect("read request");
        let body = format!(
            r#"{{"local":"http://{address}/opds/v2/books/book-1/manifest","external":"https://readium.org/webpub-manifest/context.jsonld"}}"#
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write response");
    });

    let client = reqwest::Client::builder()
        .build()
        .expect("reqwest client should build");
    let case = compat::cases::CaseConfig {
        id: "URL-NORMALIZATION".to_string(),
        method: "GET".to_string(),
        path: "/opds/v2/books/book-1/manifest".to_string(),
        body: None,
        comparison: ComparisonMode::Json,
        headers: None,
        setup: None,
    };

    let normalized = execute_case_async(&client, &format!("http://{address}"), &case, &BTreeMap::new())
        .await
        .expect("case should execute");

    assert_eq!(
        normalized.body,
        NormalizedBody::Json(serde_json::json!({
            "external": "https://readium.org/webpub-manifest/context.jsonld",
            "local": "http://komga.local/opds/v2/books/book-1/manifest",
        }))
    );

    let _ = server.await;
}

#[test]
fn diff_report_is_written_as_json() {
    let mut allowlist = BTreeSet::new();
    allowlist.insert("content-type".to_string());

    let left = NormalizedResponse {
        status: 200,
        headers: BTreeMap::from([(
            "content-type".to_string(),
            vec!["application/json".to_string()],
        )]),
        body: NormalizedBody::Json(serde_json::json!({"ok": true})),
    };
    let right = NormalizedResponse {
        status: 401,
        headers: BTreeMap::from([(
            "content-type".to_string(),
            vec!["application/json".to_string()],
        )]),
        body: NormalizedBody::Json(serde_json::json!({"ok": false})),
    };

    let report = compare_responses("TEST-CASE", &left, &right, &allowlist, ComparisonMode::Json);
    assert!(!report.matches);

    let output_root = temp_output_root();
    write_diff_report(&output_root, &report).expect("diff report should be written");

    let written =
        fs::read_to_string(output_root.join("TEST-CASE.json")).expect("report file should exist");
    assert!(written.contains("TEST-CASE"));
    assert!(written.contains("status mismatch"));
}

#[test]
fn binary_metadata_comparison_ignores_body_differences() {
    let allowlist = BTreeSet::from([
        "content-type".to_string(),
        "content-disposition".to_string(),
    ]);

    let left = NormalizedResponse {
        status: 200,
        headers: BTreeMap::from([
            ("content-type".to_string(), vec!["image/jpeg".to_string()]),
            (
                "content-disposition".to_string(),
                vec!["inline; filename=page-1.jpg".to_string()],
            ),
        ]),
        body: NormalizedBody::Text("java-binary-placeholder".to_string()),
    };
    let right = NormalizedResponse {
        status: 200,
        headers: BTreeMap::from([
            ("content-type".to_string(), vec!["image/jpeg".to_string()]),
            (
                "content-disposition".to_string(),
                vec!["inline; filename=page-1.jpg".to_string()],
            ),
        ]),
        body: NormalizedBody::Text("rust-binary-placeholder".to_string()),
    };

    let report = compare_responses(
        "BINARY-CASE",
        &left,
        &right,
        &allowlist,
        ComparisonMode::BinaryMetadata,
    );

    assert!(report.matches);
}

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

    let resolved = resolve_headers(&Some(headers), &BTreeMap::new()).expect("headers should resolve");

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
    assert_eq!(
        resolved.get("X-Auth-Token"),
        Some(&"".to_string())
    );
}

#[test]
#[ignore = "requires running Java and Rust servers via KOMGA_COMPAT_JAVA_BASE_URL and KOMGA_COMPAT_RUST_BASE_URL"]
fn live_http_json_diff_smoke() {
    let config = HarnessConfig::load_default().expect("default compat cases should load");
    let java_base =
        std::env::var("KOMGA_COMPAT_JAVA_BASE_URL").expect("missing KOMGA_COMPAT_JAVA_BASE_URL");
    let rust_base =
        std::env::var("KOMGA_COMPAT_RUST_BASE_URL").expect("missing KOMGA_COMPAT_RUST_BASE_URL");
    let allowlist: BTreeSet<String> = config.header_allowlist.iter().cloned().collect();
    let client = Client::builder()
        .build()
        .expect("reqwest client should build");

    for case_id in [
        "P0-AUTH-SETCOOKIE",
        "P0-AUTH-REMEMBERME",
        "P0-OPDS-V2-CATALOG-UNAUTH",
        "P0-OPDS-V2-AUTH-DOCUMENT",
    ] {
        let case = config
            .cases
            .iter()
            .find(|it| it.id == case_id)
            .expect("auth case should exist");

        let mut java_vars = BTreeMap::new();
        let mut rust_vars = BTreeMap::new();
        apply_setup_steps(
            &client,
            &java_base,
            case.setup.as_deref().unwrap_or(&[]),
            &mut java_vars,
        )
        .expect("java setup should execute");
        apply_setup_steps(
            &client,
            &rust_base,
            case.setup.as_deref().unwrap_or(&[]),
            &mut rust_vars,
        )
        .expect("rust setup should execute");

        let java = execute_case(&client, &java_base, case, &java_vars)
            .expect("java case should execute");
        let rust = execute_case(&client, &rust_base, case, &rust_vars)
            .expect("rust case should execute");
        let report = compare_responses(&case.id, &java, &rust, &allowlist, case.comparison);

        if !report.matches {
            write_diff_report(&PathBuf::from(&config.output_dir), &report)
                .expect("report should be written on mismatch");
        }

        assert!(report.matches, "compat diff failed for {}", case.id);
    }
}

#[test]
#[ignore = "requires running Java and Rust servers via KOMGA_COMPAT_JAVA_BASE_URL and KOMGA_COMPAT_RUST_BASE_URL"]
fn live_seeded_localdb_data_diff_smoke() {
    let config = seeded_localdb_smoke_harness_config();
    let java_base =
        std::env::var("KOMGA_COMPAT_JAVA_BASE_URL").expect("missing KOMGA_COMPAT_JAVA_BASE_URL");
    let rust_base =
        std::env::var("KOMGA_COMPAT_RUST_BASE_URL").expect("missing KOMGA_COMPAT_RUST_BASE_URL");
    let client = Client::builder()
        .build()
        .expect("reqwest client should build");
    let allowlist: BTreeSet<String> = config.header_allowlist.iter().cloned().collect();

    let java_vars = seeded_localdb_session_vars(&client, &java_base);
    let rust_vars = seeded_localdb_session_vars(&client, &rust_base);

    for case in &config.cases {
        let java = execute_case(&client, &java_base, case, &java_vars)
            .expect("java seeded-localdb case should execute");
        let rust = execute_case(&client, &rust_base, case, &rust_vars)
            .expect("rust seeded-localdb case should execute");
        let report = compare_responses(&case.id, &java, &rust, &allowlist, case.comparison);

        if !report.matches {
            write_diff_report(&PathBuf::from(&config.output_dir), &report)
                .expect("report should be written on mismatch");
        }

        assert!(report.matches, "compat diff failed for {}", case.id);
    }
}

#[test]
#[ignore = "requires running Java and Rust servers via KOMGA_COMPAT_JAVA_BASE_URL and KOMGA_COMPAT_RUST_BASE_URL"]
fn live_seeded_localdb_binary_manifest_diff_smoke() {
    let config = seeded_localdb_binary_manifest_smoke_harness_config();
    let java_base =
        std::env::var("KOMGA_COMPAT_JAVA_BASE_URL").expect("missing KOMGA_COMPAT_JAVA_BASE_URL");
    let rust_base =
        std::env::var("KOMGA_COMPAT_RUST_BASE_URL").expect("missing KOMGA_COMPAT_RUST_BASE_URL");
    let client = Client::builder()
        .build()
        .expect("reqwest client should build");
    let allowlist: BTreeSet<String> = config.header_allowlist.iter().cloned().collect();

    let java_vars = seeded_localdb_session_vars(&client, &java_base);
    let rust_vars = seeded_localdb_session_vars(&client, &rust_base);

    for case in &config.cases {
        let java = execute_case(&client, &java_base, case, &java_vars)
            .expect("java seeded-localdb case should execute");
        let rust = execute_case(&client, &rust_base, case, &rust_vars)
            .expect("rust seeded-localdb case should execute");
        let report = compare_responses(&case.id, &java, &rust, &allowlist, case.comparison);

        if !report.matches {
            write_diff_report(&PathBuf::from(&config.output_dir), &report)
                .expect("report should be written on mismatch");
        }

        assert!(report.matches, "compat diff failed for {}", case.id);
    }
}

#[tokio::test]
async fn rust_http_json_diff_smoke_against_self() {
    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind test listener");
    let address = listener.local_addr().expect("listener address");
    let server = tokio::spawn(async move {
        komga_rust::app::serve(listener)
            .await
            .expect("server should run");
    });

    let config = smoke_harness_config();
    let base_url = format!("http://{address}");
    let allowlist: BTreeSet<String> = config.header_allowlist.iter().cloned().collect();
    let client = reqwest::Client::builder()
        .build()
        .expect("reqwest client should build");

    for case in &config.cases {
        let java = execute_case_async(&client, &base_url, case, &BTreeMap::new())
            .await
            .expect("self case should execute");
        let rust = execute_case_async(&client, &base_url, case, &BTreeMap::new())
            .await
            .expect("self case should execute");
        let report = compare_responses(&case.id, &java, &rust, &allowlist, case.comparison);

        assert!(report.matches, "compat diff failed for {}", case.id);
        assert!(
            report.differences.is_empty(),
            "unexpected diffs for {}",
            case.id
        );
    }

    server.abort();
    let _ = server.await;
}

fn execute_case(
    client: &Client,
    base_url: &str,
    case: &compat::cases::CaseConfig,
    vars: &BTreeMap<String, String>,
) -> anyhow::Result<NormalizedResponse> {
    let mut request = match case.method.as_str() {
        "GET" => client.get(format!(
            "{}{}",
            base_url.trim_end_matches('/'),
            case.path.as_str()
        )),
        "PATCH" => client.patch(format!(
            "{}{}",
            base_url.trim_end_matches('/'),
            case.path.as_str()
        )),
        other => anyhow::bail!("unsupported method in skeleton: {other}"),
    };

    if let Some(body) = &case.body {
        request = request.body(body.clone());
    }

    if let Some(headers) = resolve_headers(&case.headers, vars)? {
        let mut header_map = HeaderMap::new();
        for (name, value) in &headers {
            header_map.insert(
                HeaderName::from_bytes(name.as_bytes())?,
                HeaderValue::from_str(value)?,
            );
        }
        request = request.headers(header_map);
    }

    let response = request.send()?;
    let status = response.status().as_u16();
    let headers = normalize_headers(response.headers(), &case.header_allowlist());
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|it| it.to_str().ok())
        .map(str::to_owned);
    let body_text = response.text()?;
    let body = if case.comparison == ComparisonMode::BinaryMetadata {
        NormalizedBody::Empty
    } else if content_type.as_deref().unwrap_or_default().contains("json") {
        NormalizedBody::Json(normalize_json_body(&body_text, base_url)?)
    } else if body_text.is_empty() {
        NormalizedBody::Empty
    } else {
        NormalizedBody::Text(body_text)
    };

    Ok(NormalizedResponse {
        status,
        headers,
        body,
    })
}

async fn execute_case_async(
    client: &reqwest::Client,
    base_url: &str,
    case: &compat::cases::CaseConfig,
    vars: &BTreeMap<String, String>,
) -> anyhow::Result<NormalizedResponse> {
    let mut request = match case.method.as_str() {
        "GET" => client.get(format!(
            "{}{}",
            base_url.trim_end_matches('/'),
            case.path.as_str()
        )),
        "PATCH" => client.patch(format!(
            "{}{}",
            base_url.trim_end_matches('/'),
            case.path.as_str()
        )),
        other => anyhow::bail!("unsupported method in skeleton: {other}"),
    };

    if let Some(body) = &case.body {
        request = request.body(body.clone());
    }

    if let Some(headers) = resolve_headers(&case.headers, vars)? {
        let mut header_map = HeaderMap::new();
        for (name, value) in &headers {
            header_map.insert(
                HeaderName::from_bytes(name.as_bytes())?,
                HeaderValue::from_str(value)?,
            );
        }
        request = request.headers(header_map);
    }

    let response = request.send().await?;
    let status = response.status().as_u16();
    let headers = normalize_headers(response.headers(), &case.header_allowlist());
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|it| it.to_str().ok())
        .map(str::to_owned);
    let body_text = response.text().await?;
    let body = if case.comparison == ComparisonMode::BinaryMetadata {
        NormalizedBody::Empty
    } else if content_type.as_deref().unwrap_or_default().contains("json") {
        NormalizedBody::Json(normalize_json_body(&body_text, base_url)?)
    } else if body_text.is_empty() {
        NormalizedBody::Empty
    } else {
        NormalizedBody::Text(body_text)
    };

    Ok(NormalizedResponse {
        status,
        headers,
        body,
    })
}

fn smoke_harness_config() -> HarnessConfig {
    HarnessConfig {
        output_dir: "target/compat-diff-smoke".to_string(),
        header_allowlist: vec!["content-type".to_string()],
        cases: vec![
            smoke_case("SMOKE-LIBRARIES", "/api/v1/libraries"),
            smoke_case("SMOKE-BOOKS", "/api/v1/books"),
        ],
    }
}

fn seeded_localdb_smoke_harness_config() -> HarnessConfig {
    HarnessConfig {
        output_dir: "target/compat-diff-seeded-localdb-smoke".to_string(),
        header_allowlist: vec!["content-type".to_string()],
        cases: vec![
            seeded_localdb_smoke_case("SEEDED-LOCALDB-LIBRARIES", "/api/v1/libraries", ComparisonMode::Json),
            seeded_localdb_smoke_case("SEEDED-LOCALDB-SERIES", "/api/v1/series", ComparisonMode::Json),
            seeded_localdb_smoke_case("SEEDED-LOCALDB-BOOKS", "/api/v1/books", ComparisonMode::Json),
            seeded_localdb_smoke_case(
                "KOMGA-P0-BOOKS-LATEST-01",
                "/api/v1/books/latest?unpaged=true",
                ComparisonMode::Json,
            ),
            seeded_localdb_smoke_case(
                "KOMGA-P0-BK-PAGES-01",
                "/api/v1/books/book-1/pages",
                ComparisonMode::Json,
            ),
        ],
    }
}

fn seeded_localdb_binary_manifest_smoke_harness_config() -> HarnessConfig {
    HarnessConfig {
        output_dir: "target/compat-diff-seeded-localdb-binary-manifest-smoke".to_string(),
        header_allowlist: vec!["content-type".to_string(), "content-disposition".to_string()],
        cases: vec![
            seeded_localdb_smoke_case(
                "SEEDED-LOCALDB-BINARY-PAGE",
                "/api/v1/books/book-1/pages/1",
                ComparisonMode::BinaryMetadata,
            ),
            seeded_localdb_smoke_case(
                "SEEDED-LOCALDB-BINARY-FILE",
                "/api/v1/books/book-1/file",
                ComparisonMode::BinaryMetadata,
            ),
            seeded_localdb_smoke_case(
                "KOMGA-P0-BK-THUMBNAIL-01",
                "/api/v1/books/book-1/pages/1/thumbnail",
                ComparisonMode::BinaryMetadata,
            ),
            seeded_localdb_smoke_case(
                "SEEDED-LOCALDB-OPDS-MANIFEST",
                "/opds/v2/books/book-1/manifest",
                ComparisonMode::Json,
            ),
        ],
    }
}

fn smoke_case(id: &str, path: &str) -> compat::cases::CaseConfig {
    compat::cases::CaseConfig {
        id: id.to_string(),
        method: "GET".to_string(),
        path: path.to_string(),
        body: None,
        comparison: ComparisonMode::Json,
        headers: Some(BTreeMap::from([(
            "X-Auth-Token".to_string(),
            "smoke-token".to_string(),
        )])),
        setup: None,
    }
}

fn seeded_localdb_smoke_case(
    id: &str,
    path: &str,
    comparison: ComparisonMode,
) -> compat::cases::CaseConfig {
    compat::cases::CaseConfig {
        id: id.to_string(),
        method: "GET".to_string(),
        path: path.to_string(),
        body: None,
        comparison,
        headers: Some(BTreeMap::from([(
            "X-Auth-Token".to_string(),
            "${SESSION_TOKEN}".to_string(),
        )])),
        setup: None,
    }
}

fn seeded_localdb_session_vars(client: &Client, base_url: &str) -> BTreeMap<String, String> {
    let basic_auth =
        std::env::var("KOMGA_COMPAT_BASIC_AUTH").expect("missing KOMGA_COMPAT_BASIC_AUTH");
    let response = client
        .get(format!("{}{}", base_url.trim_end_matches('/'), "/api/v2/users/me"))
        .header("Authorization", format!("Basic {basic_auth}"))
        .header("X-Auth-Token", "")
        .send()
        .expect("login request should send");

    assert!(response.status().is_success(), "login should succeed");

    let token = response
        .headers()
        .get("X-Auth-Token")
        .expect("login response should include X-Auth-Token")
        .to_str()
        .expect("X-Auth-Token should be valid UTF-8")
        .to_string();

    BTreeMap::from([("SESSION_TOKEN".to_string(), token)])
}

fn temp_output_root() -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_millis();
    let path = std::env::temp_dir().join(format!("komga-compat-diff-{millis}"));
    fs::create_dir_all(&path).expect("temp output root should be creatable");
    path
}
