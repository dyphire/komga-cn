use super::*;

const USER_BASIC_AUTH: &str = "dXNlckBleGFtcGxlLm9yZzp1c2Vy";

#[test]
fn discovery_diff_policy_is_strict_for_status_body_page_metadata_order_and_id_sets() {
    let allowlist = BTreeSet::from(["content-type".to_string()]);

    let base = NormalizedResponse {
        status: 200,
        headers: BTreeMap::from([(
            "content-type".to_string(),
            vec!["application/json".to_string()],
        )]),
        body: NormalizedBody::Json(serde_json::json!({
            "number": 0,
            "size": 2,
            "totalElements": 2,
            "totalPages": 1,
            "numberOfElements": 2,
            "content": [
                {"id": "book-1"},
                {"id": "book-2"}
            ]
        })),
    };

    let status_diff = NormalizedResponse {
        status: 401,
        headers: base.headers.clone(),
        body: base.body.clone(),
    };
    let status_report = compare_responses(
        "DISCOVERY-STRICT-STATUS",
        &base,
        &status_diff,
        &allowlist,
        ComparisonMode::Json,
    );
    assert!(!status_report.matches);

    let page_meta_diff = NormalizedResponse {
        status: 200,
        headers: base.headers.clone(),
        body: NormalizedBody::Json(serde_json::json!({
            "number": 1,
            "size": 2,
            "totalElements": 2,
            "totalPages": 1,
            "numberOfElements": 2,
            "content": [
                {"id": "book-1"},
                {"id": "book-2"}
            ]
        })),
    };
    let page_meta_report = compare_responses(
        "DISCOVERY-STRICT-PAGE-META",
        &base,
        &page_meta_diff,
        &allowlist,
        ComparisonMode::Json,
    );
    assert!(!page_meta_report.matches);

    let order_diff = NormalizedResponse {
        status: 200,
        headers: base.headers.clone(),
        body: NormalizedBody::Json(serde_json::json!({
            "number": 0,
            "size": 2,
            "totalElements": 2,
            "totalPages": 1,
            "numberOfElements": 2,
            "content": [
                {"id": "book-2"},
                {"id": "book-1"}
            ]
        })),
    };
    let order_report = compare_responses(
        "DISCOVERY-STRICT-ORDER",
        &base,
        &order_diff,
        &allowlist,
        ComparisonMode::Json,
    );
    assert!(!order_report.matches);

    let id_set_diff = NormalizedResponse {
        status: 200,
        headers: base.headers.clone(),
        body: NormalizedBody::Json(serde_json::json!({
            "number": 0,
            "size": 2,
            "totalElements": 2,
            "totalPages": 1,
            "numberOfElements": 2,
            "content": [
                {"id": "book-1"},
                {"id": "book-3"}
            ]
        })),
    };
    let id_set_report = compare_responses(
        "DISCOVERY-STRICT-ID-SET",
        &base,
        &id_set_diff,
        &allowlist,
        ComparisonMode::Json,
    );
    assert!(!id_set_report.matches);
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

#[tokio::test]
async fn phase8_readlist_books_family_owned_cases_self_diff_clean() {
    let config = HarnessConfig::load_default().expect("default compat cases should load");

    let responses =
        run_cases_against_self(&config, phase8_readlist_books_family_owned_case_ids()).await;

    assert_eq!(
        responses.len(),
        phase8_readlist_books_family_owned_case_ids().len(),
        "phase8 owned compat self-diff must cover the full owned inventory",
    );

    for (case_id, response) in responses {
        assert_eq!(
            response.status, 200,
            "owned readlist-books compat case should stay HTTP 200: {case_id}",
        );

        match response.body {
            NormalizedBody::Json(body) => {
                assert!(
                    body.get("_compat").is_none(),
                    "owned readlist-books compat case must not emit _compat diagnostics: {case_id}",
                );
            }
            other => panic!("owned readlist-books compat case must stay JSON: {case_id} => {other:?}"),
        }
    }
}

#[tokio::test]
async fn phase8_readlist_books_family_negative_inventory_is_explicit_and_self_consistent() {
    let config = HarnessConfig::load_default().expect("default compat cases should load");

    let dependency = config
        .cases
        .iter()
        .find(|it| it.id == "P8-READLIST-BOOKS-DEPENDENCY-UNPAGED-PREOWNED")
        .expect("dependency case should exist");
    let widened = config
        .cases
        .iter()
        .find(|it| it.id == "P8-READLIST-BOOKS-DEPENDENCY-UNPAGED-WIDENED-SHADOW")
        .expect("widened dependency case should exist");
    let readlists = config
        .cases
        .iter()
        .find(|it| it.id == "P8-READLIST-BOOKS-EXCLUDED-READLISTS-LIST-FAMILY")
        .expect("readlists exclusion case should exist");
    let tachiyomi = config
        .cases
        .iter()
        .find(|it| it.id == "P8-READLIST-BOOKS-EXCLUDED-TACHIYOMI")
        .expect("tachiyomi exclusion case should exist");

    assert_eq!(
        dependency
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership")),
        None,
        "bare unpaged dependency case must stay pre-owned inventory, not shadow inventory",
    );
    assert_eq!(
        widened
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership")),
        Some(&"shadow-java-writer".to_string()),
        "widened unpaged dependency case must stay explicit shadow inventory",
    );
    assert_eq!(
        readlists
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership")),
        Some(&"shadow-java-writer".to_string()),
        "list-family exclusion must stay explicit shadow inventory",
    );
    assert_eq!(
        tachiyomi
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership")),
        Some(&"shadow-java-writer".to_string()),
        "tachiyomi exclusion must stay explicit shadow inventory",
    );

    let responses = run_cases_against_self(&config, phase8_readlist_books_family_negative_case_ids()).await;

    assert_eq!(
        responses.len(),
        phase8_readlist_books_family_negative_case_ids().len(),
        "phase8 negative compat self-diff must cover the full explicit non-owned inventory",
    );

    for (case_id, response) in responses {
        match case_id.as_str() {
            "P8-READLIST-BOOKS-DEPENDENCY-UNPAGED-PREOWNED"
            | "P8-READLIST-BOOKS-DEPENDENCY-UNPAGED-WIDENED-SHADOW" => {
                assert_eq!(
                    response.status, 200,
                    "negative inventory case should stay HTTP 200: {case_id}"
                );

                let body = match response.body {
                    NormalizedBody::Json(body) => body,
                    other => panic!(
                        "dependency inventory case must stay JSON for explicit shadow evidence: {case_id} => {other:?}"
                    ),
                };

                if case_id == "P8-READLIST-BOOKS-DEPENDENCY-UNPAGED-PREOWNED" {
                    assert!(
                        body.get("_compat").is_none(),
                        "pre-owned dependency case must stay native-clean instead of shadow-inferred: {case_id}",
                    );
                    assert_eq!(
                        body["pageable"]["unpaged"],
                        serde_json::Value::Bool(true),
                        "pre-owned dependency case must stay on the unpaged route shape: {case_id}",
                    );
                } else {
                    assert_eq!(
                        body["_compat"]["discoveryOwnership"],
                        serde_json::Value::String("non-native".to_string()),
                        "widened dependency case must stay explicit shadow inventory: {case_id}",
                    );
                    assert_eq!(
                        body["_compat"]["shape"],
                        serde_json::Value::String(
                            "UnsupportedBookFilter(LibraryId)".to_string()
                        ),
                        "widened dependency case must stay explicit about the rejected ownership shape: {case_id}",
                    );
                }
            }
            "P8-READLIST-BOOKS-EXCLUDED-READLISTS-LIST-FAMILY"
            | "P8-READLIST-BOOKS-EXCLUDED-TACHIYOMI" => {
                assert_eq!(
                    response.status, 404,
                    "excluded inventory case should stay HTTP 404: {case_id}"
                );
            }
            _ => panic!("unexpected phase8 negative compat case id: {case_id}"),
        }
    }
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

async fn run_cases_against_self(
    config: &HarnessConfig,
    case_ids: &[&str],
) -> Vec<(String, NormalizedResponse)> {
    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind test listener");
    let address = listener.local_addr().expect("listener address");
    let server = tokio::spawn(async move {
        komga_rust::app::serve(listener)
            .await
            .expect("server should run");
    });

    let base_url = format!("http://{address}");
    let allowlist: BTreeSet<String> = config.header_allowlist.iter().cloned().collect();
    let client = reqwest::Client::builder()
        .build()
        .expect("reqwest client should build");
    let session_token = fetch_session_token(&client, &base_url).await;
    let mut responses = Vec::with_capacity(case_ids.len());

    for case_id in case_ids {
        let case = config
            .cases
            .iter()
            .find(|it| it.id == *case_id)
            .unwrap_or_else(|| panic!("missing phase8 readlist-books compat case: {case_id}"));
        let vars = BTreeMap::from([("SESSION_TOKEN".to_string(), session_token.clone())]);

        let left = execute_case_async(&client, &base_url, case, &vars)
            .await
            .unwrap_or_else(|error| panic!("left case should execute for {case_id}: {error:#}"));
        let right = execute_case_async(&client, &base_url, case, &vars)
            .await
            .unwrap_or_else(|error| panic!("right case should execute for {case_id}: {error:#}"));
        let report = compare_responses(&case.id, &left, &right, &allowlist, case.comparison);

        if !report.matches {
            write_diff_report(&temp_output_root(), &report)
                .unwrap_or_else(|error| panic!("report should be written on mismatch for {case_id}: {error:#}"));
        }

        assert!(report.matches, "compat diff failed for {case_id}");
        assert!(
            report.differences.is_empty(),
            "unexpected diffs for {case_id}: {:?}",
            report.differences,
        );

        responses.push((case.id.clone(), left));
    }

    server.abort();
    let _ = server.await;
    responses
}

async fn fetch_session_token(client: &reqwest::Client, base_url: &str) -> String {
    let response = client
        .get(format!("{}{}", base_url.trim_end_matches('/'), "/api/v2/users/me"))
        .header("Authorization", format!("Basic {USER_BASIC_AUTH}"))
        .header("X-Auth-Token", "")
        .send()
        .await
        .expect("login request should send");

    assert!(response.status().is_success(), "login should succeed");

    response
        .headers()
        .get("X-Auth-Token")
        .expect("login response should include X-Auth-Token")
        .to_str()
        .expect("X-Auth-Token should be valid UTF-8")
        .to_string()
}
