use komga_compat_testkit::cases::{CaseConfig, HarnessConfig};
use komga_compat_testkit::diff_writer::{compare_responses, write_diff_report};
use komga_compat_testkit::normalize::{normalize_headers, normalize_json_body, normalize_xml_body};
use komga_compat_testkit::runtime::{apply_setup_steps, resolve_headers};
use komga_compat_testkit::{ComparisonMode, NormalizedBody, NormalizedResponse};
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
    let required_case_ids = [
        "KOMGA-P0-LIB-01-ADMIN",
        "KOMGA-P0-LIB-01-USER",
        "KOMGA-P0-LIB-01-LIMITED",
        "P1-AUTH-APIKEY-UPPER",
        "P1-AUTH-APIKEY-LOWER",
        "P1-AUTH-APIKEY-INVALID",
        "P0-OPDS-V1-SERIES",
        "P1-BK-READ-PROGRESS-DELETE",
        "P1-BK-READ-PROGRESS-404",
        "P1-BK-PROGRESSION-VALID",
        "P1-BK-PROGRESSION-INVALID",
        "P1-SEARCH-QUERY",
        "P1-SEARCH-ORDERING",
        "P1-SEARCH-OWNERSHIP-SHADOW",
    ];

    for id in required_case_ids {
        assert!(
            case_ids.contains(&id),
            "missing required compatibility case id: {id}"
        );
        assert_eq!(
            config.cases.iter().filter(|it| it.id == id).count(),
            1,
            "case id should appear exactly once: {id}"
        );
        assert_eq!(
            PathBuf::from(&config.output_dir).join(format!("{id}.json")),
            PathBuf::from("target/compat-diff").join(format!("{id}.json")),
            "diff evidence path contract changed for {id}"
        );
    }

    let library_case = config
        .cases
        .iter()
        .find(|it| it.id == "KOMGA-P0-LIB-01")
        .expect("library case should exist");
    let library_admin_case = config
        .cases
        .iter()
        .find(|it| it.id == "KOMGA-P0-LIB-01-ADMIN")
        .expect("library admin case should exist");
    let library_user_case = config
        .cases
        .iter()
        .find(|it| it.id == "KOMGA-P0-LIB-01-USER")
        .expect("library user case should exist");
    let library_limited_case = config
        .cases
        .iter()
        .find(|it| it.id == "KOMGA-P0-LIB-01-LIMITED")
        .expect("library limited case should exist");
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
    let api_key_upper_case = config
        .cases
        .iter()
        .find(|it| it.id == "P1-AUTH-APIKEY-UPPER")
        .expect("api-key upper-case header case should exist");
    let api_key_lower_case = config
        .cases
        .iter()
        .find(|it| it.id == "P1-AUTH-APIKEY-LOWER")
        .expect("api-key lower-case header case should exist");
    let api_key_invalid_case = config
        .cases
        .iter()
        .find(|it| it.id == "P1-AUTH-APIKEY-INVALID")
        .expect("api-key invalid case should exist");
    let read_progress_delete_case = config
        .cases
        .iter()
        .find(|it| it.id == "P1-BK-READ-PROGRESS-DELETE")
        .expect("read-progress delete case should exist");
    let read_progress_missing_case = config
        .cases
        .iter()
        .find(|it| it.id == "P1-BK-READ-PROGRESS-404")
        .expect("read-progress 404 case should exist");
    let progression_valid_case = config
        .cases
        .iter()
        .find(|it| it.id == "P1-BK-PROGRESSION-VALID")
        .expect("book progression valid case should exist");
    let progression_invalid_case = config
        .cases
        .iter()
        .find(|it| it.id == "P1-BK-PROGRESSION-INVALID")
        .expect("book progression invalid case should exist");
    let search_query_case = config
        .cases
        .iter()
        .find(|it| it.id == "P1-SEARCH-QUERY")
        .expect("search query case should exist");
    let search_ordering_case = config
        .cases
        .iter()
        .find(|it| it.id == "P1-SEARCH-ORDERING")
        .expect("search ordering case should exist");
    let search_ownership_case = config
        .cases
        .iter()
        .find(|it| it.id == "P1-SEARCH-OWNERSHIP-SHADOW")
        .expect("search ownership case should exist");
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
    let opds_v1_series_case = config
        .cases
        .iter()
        .find(|it| it.id == "P0-OPDS-V1-SERIES")
        .expect("opds v1 series case should exist");
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
    assert!(config.header_allowlist.contains(&"set-cookie".to_string()));
    assert!(
        config
            .header_allowlist
            .contains(&"x-auth-token".to_string())
    );
    assert!(
        config
            .header_allowlist
            .contains(&"www-authenticate".to_string())
    );
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
    assert_eq!(library_admin_case.method, "GET");
    assert_eq!(library_admin_case.path, "/api/v1/libraries");
    assert_eq!(library_admin_case.comparison, ComparisonMode::Json);
    assert_eq!(library_admin_case.setup.as_ref().map(Vec::len), Some(1));
    assert_eq!(
        library_admin_case
            .setup
            .as_ref()
            .and_then(|steps| steps[0].headers.as_ref())
            .and_then(|headers| headers.get("Authorization")),
        Some(&"Basic ${KOMGA_COMPAT_BASIC_AUTH_ADMIN}".to_string())
    );
    assert_eq!(library_user_case.method, "GET");
    assert_eq!(library_user_case.path, "/api/v1/libraries");
    assert_eq!(library_user_case.comparison, ComparisonMode::Json);
    assert_eq!(library_user_case.setup.as_ref().map(Vec::len), Some(1));
    assert_eq!(
        library_user_case
            .setup
            .as_ref()
            .and_then(|steps| steps[0].headers.as_ref())
            .and_then(|headers| headers.get("Authorization")),
        Some(&"Basic ${KOMGA_COMPAT_BASIC_AUTH_USER}".to_string())
    );
    assert_eq!(library_limited_case.method, "GET");
    assert_eq!(library_limited_case.path, "/api/v1/libraries");
    assert_eq!(library_limited_case.comparison, ComparisonMode::Json);
    assert_eq!(library_limited_case.setup.as_ref().map(Vec::len), Some(1));
    assert_eq!(
        library_limited_case
            .setup
            .as_ref()
            .and_then(|steps| steps[0].headers.as_ref())
            .and_then(|headers| headers.get("Authorization")),
        Some(&"Basic ${KOMGA_COMPAT_BASIC_AUTH_LIMITED}".to_string())
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
    assert_eq!(
        thumbnail_case.path,
        "/api/v1/books/book-1/pages/1/thumbnail"
    );
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
    assert_eq!(
        book_thumbnail_case.comparison,
        ComparisonMode::BinaryMetadata
    );
    assert_eq!(
        book_thumbnail_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Auth-Token")),
        Some(&"${SESSION_TOKEN}".to_string())
    );
    assert_eq!(read_progress_case.method, "PATCH");
    assert_eq!(
        read_progress_case.path,
        "/api/v1/books/book-1/read-progress"
    );
    assert_eq!(read_progress_case.comparison, ComparisonMode::Json);
    assert_eq!(
        read_progress_case.body.as_deref(),
        Some(r#"{"completed":true}"#)
    );
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
    assert_eq!(api_key_upper_case.method, "GET");
    assert_eq!(api_key_upper_case.path, "/api/v2/users/me");
    assert_eq!(api_key_upper_case.comparison, ComparisonMode::Json);
    assert_eq!(
        api_key_upper_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-API-Key")),
        Some(&"${KOMGA_COMPAT_API_KEY}".to_string())
    );
    assert!(api_key_upper_case.setup.is_none());
    assert_eq!(api_key_lower_case.method, "GET");
    assert_eq!(api_key_lower_case.path, "/api/v2/users/me");
    assert_eq!(api_key_lower_case.comparison, ComparisonMode::Json);
    assert_eq!(
        api_key_lower_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("x-api-key")),
        Some(&"${KOMGA_COMPAT_API_KEY}".to_string())
    );
    assert!(api_key_lower_case.setup.is_none());
    assert_eq!(api_key_invalid_case.method, "GET");
    assert_eq!(api_key_invalid_case.path, "/api/v2/users/me");
    assert_eq!(api_key_invalid_case.comparison, ComparisonMode::Json);
    assert_eq!(
        api_key_invalid_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("x-api-key")),
        Some(&"${KOMGA_COMPAT_API_KEY_INVALID}".to_string())
    );
    assert!(api_key_invalid_case.setup.is_none());
    assert_eq!(read_progress_delete_case.method, "DELETE");
    assert_eq!(
        read_progress_delete_case.path,
        "/api/v1/books/book-1/read-progress"
    );
    assert_eq!(read_progress_delete_case.comparison, ComparisonMode::Json);
    assert_eq!(
        read_progress_delete_case.setup.as_ref().map(Vec::len),
        Some(1)
    );
    assert_eq!(read_progress_missing_case.method, "DELETE");
    assert_eq!(
        read_progress_missing_case.path,
        "/api/v1/books/book-missing/read-progress"
    );
    assert_eq!(read_progress_missing_case.comparison, ComparisonMode::Json);
    assert_eq!(
        read_progress_missing_case.setup.as_ref().map(Vec::len),
        Some(1)
    );
    assert_eq!(progression_valid_case.method, "PATCH");
    assert_eq!(
        progression_valid_case.path,
        "/api/v1/books/book-1/progression"
    );
    assert_eq!(progression_valid_case.comparison, ComparisonMode::Json);
    assert_eq!(progression_valid_case.setup.as_ref().map(Vec::len), Some(1));
    assert_eq!(
        progression_valid_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("Content-Type")),
        Some(&"application/json".to_string())
    );
    assert_eq!(
        progression_valid_case.body.as_deref(),
        Some(
            r#"{"modified":"2024-01-01T00:00:00Z","device":"compat-client","locator":{"href":"OEBPS/chapter-1.xhtml","type":"application/xhtml+xml","locations":{"progression":0.3}}}"#
        )
    );
    assert_eq!(progression_invalid_case.method, "PATCH");
    assert_eq!(
        progression_invalid_case.path,
        "/api/v1/books/book-1/progression"
    );
    assert_eq!(progression_invalid_case.comparison, ComparisonMode::Json);
    assert_eq!(
        progression_invalid_case.setup.as_ref().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        progression_invalid_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("Content-Type")),
        Some(&"application/json".to_string())
    );
    assert_eq!(
        progression_invalid_case.body.as_deref(),
        Some(
            r#"{"modified":"2024-01-01T00:00:00Z","device":"compat-client","locator":{"href":"OEBPS/chapter-1.xhtml","type":"application/xhtml+xml","locations":{}}}"#
        )
    );
    assert_eq!(search_query_case.method, "POST");
    assert_eq!(search_query_case.path, "/api/v1/series/list");
    assert_eq!(search_query_case.comparison, ComparisonMode::Json);
    assert_eq!(search_query_case.setup.as_ref().map(Vec::len), Some(1));
    assert_eq!(
        search_query_case.body.as_deref(),
        Some(r#"{"fullTextSearch":"series"}"#)
    );
    assert_eq!(search_ordering_case.method, "POST");
    assert_eq!(
        search_ordering_case.path,
        "/api/v1/series/list?page=0&size=20&sort=metadata.titleSort,asc"
    );
    assert_eq!(search_ordering_case.comparison, ComparisonMode::Json);
    assert_eq!(search_ordering_case.setup.as_ref().map(Vec::len), Some(1));
    assert_eq!(
        search_ordering_case.body.as_deref(),
        Some(r#"{"fullTextSearch":"series"}"#)
    );
    assert_eq!(search_ownership_case.method, "POST");
    assert_eq!(search_ownership_case.path, "/api/v1/series/list");
    assert_eq!(search_ownership_case.comparison, ComparisonMode::Json);
    assert_eq!(search_ownership_case.setup.as_ref().map(Vec::len), Some(1));
    assert_eq!(
        search_ownership_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership")),
        Some(&"shadow-java-writer".to_string())
    );
    let search_ownership_allowlist = search_ownership_case.header_allowlist();
    assert!(
        search_ownership_allowlist.contains("x-komga-compat-search-ownership"),
        "search ownership marker header should be diff-allowlisted at case level"
    );
    assert_eq!(
        search_ownership_case.body.as_deref(),
        Some(r#"{"fullTextSearch":"series","ownership":"shadow"}"#)
    );
    assert_eq!(catalog_case.method, "GET");
    assert_eq!(catalog_case.path, "/opds/v2/catalog");
    assert_eq!(catalog_case.comparison, ComparisonMode::Json);
    assert!(catalog_case.headers.is_none());
    assert_eq!(auth_case.method, "GET");
    assert_eq!(auth_case.path, "/opds/v2/auth");
    assert_eq!(auth_case.comparison, ComparisonMode::Json);
    assert!(auth_case.headers.is_none());
    assert_eq!(opds_v1_series_case.method, "GET");
    assert_eq!(opds_v1_series_case.path, "/opds/v1.2/series");
    assert_eq!(opds_v1_series_case.comparison, ComparisonMode::Xml);
    assert_eq!(opds_v1_series_case.setup.as_ref().map(Vec::len), Some(1));
    assert_eq!(
        opds_v1_series_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Auth-Token")),
        Some(&"${SESSION_TOKEN}".to_string())
    );
    assert_eq!(
        opds_v1_series_case
            .setup
            .as_ref()
            .and_then(|steps| steps[0].headers.as_ref())
            .and_then(|headers| headers.get("Authorization")),
        Some(&"Basic ${KOMGA_COMPAT_BASIC_AUTH_USER}".to_string())
    );
}

#[test]
fn live_http_json_diff_includes_library_role_cases() {
    let case_ids = live_http_json_case_ids();

    assert!(case_ids.contains(&"KOMGA-P0-LIB-01-ADMIN"));
    assert!(case_ids.contains(&"KOMGA-P0-LIB-01-USER"));
    assert!(case_ids.contains(&"KOMGA-P0-LIB-01-LIMITED"));
}

#[test]
fn live_http_json_diff_includes_api_key_parity_cases() {
    let case_ids = live_http_json_case_ids();

    assert!(case_ids.contains(&"P1-AUTH-APIKEY-UPPER"));
    assert!(case_ids.contains(&"P1-AUTH-APIKEY-LOWER"));
    assert!(case_ids.contains(&"P1-AUTH-APIKEY-INVALID"));
}

#[test]
fn seeded_localdb_smoke_includes_t10_read_progress_and_progression_cases() {
    let config = seeded_localdb_smoke_harness_config();
    let case_ids: Vec<&str> = config.cases.iter().map(|it| it.id.as_str()).collect();

    assert!(case_ids.contains(&"KOMGA-P0-BK-READ-PROGRESS-01"));
    assert!(case_ids.contains(&"P1-BK-READ-PROGRESS-DELETE"));
    assert!(case_ids.contains(&"P1-BK-READ-PROGRESS-404"));
    assert!(case_ids.contains(&"P1-BK-PROGRESSION-VALID"));
    assert!(case_ids.contains(&"P1-BK-PROGRESSION-INVALID"));
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
    let case = CaseConfig {
        id: "URL-NORMALIZATION".to_string(),
        method: "GET".to_string(),
        path: "/opds/v2/books/book-1/manifest".to_string(),
        body: None,
        comparison: ComparisonMode::Json,
        headers: None,
        setup: None,
    };

    let normalized = execute_case_async(
        &client,
        &format!("http://{address}"),
        &case,
        &BTreeMap::new(),
    )
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

    for case_id in live_http_json_case_ids() {
        let case = config
            .cases
            .iter()
            .find(|it| it.id == *case_id)
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

        let java =
            execute_case(&client, &java_base, case, &java_vars).expect("java case should execute");
        let rust =
            execute_case(&client, &rust_base, case, &rust_vars).expect("rust case should execute");
        let report = compare_responses(&case.id, &java, &rust, &allowlist, case.comparison);

        if !report.matches {
            write_diff_report(&PathBuf::from(&config.output_dir), &report)
                .expect("report should be written on mismatch");
        }

        assert!(report.matches, "compat diff failed for {}", case.id);
    }
}

fn live_http_json_case_ids() -> &'static [&'static str] {
    &[
        "KOMGA-P0-LIB-01-ADMIN",
        "KOMGA-P0-LIB-01-USER",
        "KOMGA-P0-LIB-01-LIMITED",
        "P0-AUTH-SETCOOKIE",
        "P0-AUTH-REMEMBERME",
        "P1-AUTH-APIKEY-UPPER",
        "P1-AUTH-APIKEY-LOWER",
        "P1-AUTH-APIKEY-INVALID",
        "P0-OPDS-V2-CATALOG-UNAUTH",
        "P0-OPDS-V2-AUTH-DOCUMENT",
        "P0-OPDS-V1-SERIES",
    ]
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
    case: &CaseConfig,
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
    } else if case.comparison == ComparisonMode::Xml
        || content_type.as_deref().unwrap_or_default().contains("xml")
    {
        NormalizedBody::Text(normalize_xml_body(&body_text, base_url))
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
    case: &CaseConfig,
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
    } else if case.comparison == ComparisonMode::Xml
        || content_type.as_deref().unwrap_or_default().contains("xml")
    {
        NormalizedBody::Text(normalize_xml_body(&body_text, base_url))
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
            seeded_localdb_smoke_case(
                "SEEDED-LOCALDB-LIBRARIES",
                "/api/v1/libraries",
                ComparisonMode::Json,
            ),
            seeded_localdb_smoke_case(
                "SEEDED-LOCALDB-SERIES",
                "/api/v1/series",
                ComparisonMode::Json,
            ),
            seeded_localdb_smoke_case(
                "SEEDED-LOCALDB-BOOKS",
                "/api/v1/books",
                ComparisonMode::Json,
            ),
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
            seeded_localdb_smoke_case(
                "KOMGA-P0-BK-READ-PROGRESS-01",
                "/api/v1/books/book-1/read-progress",
                ComparisonMode::Json,
            ),
            seeded_localdb_smoke_case(
                "P1-BK-READ-PROGRESS-DELETE",
                "/api/v1/books/book-1/read-progress",
                ComparisonMode::Json,
            ),
            seeded_localdb_smoke_case(
                "P1-BK-READ-PROGRESS-404",
                "/api/v1/books/book-missing/read-progress",
                ComparisonMode::Json,
            ),
            seeded_localdb_smoke_case(
                "P1-BK-PROGRESSION-VALID",
                "/api/v1/books/book-1/progression",
                ComparisonMode::Json,
            ),
            seeded_localdb_smoke_case(
                "P1-BK-PROGRESSION-INVALID",
                "/api/v1/books/book-1/progression",
                ComparisonMode::Json,
            ),
            seeded_localdb_smoke_case(
                "P0-OPDS-V1-SERIES",
                "/opds/v1.2/series",
                ComparisonMode::Xml,
            ),
        ],
    }
}

fn seeded_localdb_binary_manifest_smoke_harness_config() -> HarnessConfig {
    HarnessConfig {
        output_dir: "target/compat-diff-seeded-localdb-binary-manifest-smoke".to_string(),
        header_allowlist: vec![
            "content-type".to_string(),
            "content-disposition".to_string(),
        ],
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

fn smoke_case(id: &str, path: &str) -> CaseConfig {
    CaseConfig {
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

fn seeded_localdb_smoke_case(id: &str, path: &str, comparison: ComparisonMode) -> CaseConfig {
    CaseConfig {
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
        .get(format!(
            "{}{}",
            base_url.trim_end_matches('/'),
            "/api/v2/users/me"
        ))
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
