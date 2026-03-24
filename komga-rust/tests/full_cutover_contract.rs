use anyhow::Context;
use komga_compat_testkit::cases::{CaseConfig, HarnessConfig, SetupStep};
use komga_compat_testkit::contract_matrix::required_full_cutover_targets;
use komga_compat_testkit::diff_writer::{compare_responses, write_diff_report};
use komga_compat_testkit::normalize::{normalize_headers, normalize_json_body, normalize_xml_body};
use komga_compat_testkit::runtime::resolve_headers;
use komga_compat_testkit::{ComparisonMode, NormalizedBody, NormalizedResponse};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "support/persistence_seed_fixture.rs"]
mod persistence_seed_fixture;

#[path = "compat/auth_env.rs"]
mod compat_auth_env;

const ADMIN_BASIC_AUTH: &str = "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=";
const FORBIDDEN_LIVE_KOTLIN_ENV: &str = "KOMGA_RUST_JAVA_LIVE_BASE_URL";
const FINAL_CUTOVER_CASE_IDS: &[&str] = &[
    "P0-OPDS-V2-CATALOG-UNAUTH",
    "P0-OPDS-V2-AUTH-DOCUMENT",
    "P6-CORE-LIBRARY-DETAIL-ADMIN-OWNED",
    "P6-CORE-LIBRARY-DETAIL-USER-OWNED",
    "P6-CORE-BOOK-THUMBNAILS-LIST-OWNED",
    "P12-READLISTS-TACHIYOMI-PUT-OWNED",
    "P12-READLISTS-THUMBNAIL-GET-OWNED",
    "P12-READLISTS-THUMBNAILS-LIST-OWNED",
    "P12-READLISTS-FILE-DOWNLOAD-OWNED",
    "P12-READLISTS-CREATE-OWNED",
    "P12-READLISTS-PATCH-OWNED",
    "P12-READLISTS-DELETE-OWNED",
    "P12-READLISTS-COMICRACK-MATCH-OWNED",
];

#[tokio::test]
async fn full_cutover_contract() {
    run_final_cutover_contract()
        .await
        .expect("final cutover contract should run in rust-only mode");
}

#[tokio::test]
async fn live_kotlin_dependency_is_rejected_in_final_mode() {
    let config = HarnessConfig::load_default().expect("default compat cases should load");
    let cases =
        collect_final_cutover_cases(&config).expect("final cutover case inventory should resolve");

    for case in &cases {
        assert_case_has_no_live_kotlin_dependency(case);
    }

    run_final_cutover_contract()
        .await
        .expect("rust-only final verification must run without live Kotlin wiring");
}

#[test]
fn full_cutover_target_matrix_stays_aligned_with_contract_harness() {
    let required_targets = required_full_cutover_targets();
    let tests_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut missing = Vec::new();

    for target in required_targets {
        let path = tests_dir.join(format!("{target}.rs"));
        if !path.exists() {
            missing.push(path.display().to_string());
        }
    }

    assert!(
        required_targets.contains(&"full_cutover_contract"),
        "full cutover aggregate target must stay in the matrix"
    );
    assert!(
        missing.is_empty(),
        "full cutover matrix has missing targets:\n{}",
        missing.join("\n")
    );
}

async fn run_final_cutover_contract() -> anyhow::Result<()> {
    compat_auth_env::ensure_compat_auth_env();

    let config = HarnessConfig::load_default()?;
    let cases = collect_final_cutover_cases(&config)?;
    let allowlist: BTreeSet<String> = config.header_allowlist.iter().cloned().collect();

    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .context("bind test listener")?;
    let address = listener.local_addr().context("listener address")?;
    let release_config = release_cutover_runtime_config();
    prepare_release_cutover_storage(&release_config).await?;
    let server = tokio::spawn(async move {
        komga_rust::app::serve_with_config(listener, release_config)
            .await
            .expect("server should run");
    });

    let base_url = format!("http://{address}");
    let client = reqwest::Client::builder()
        .build()
        .context("reqwest client should build")?;
    let admin_token = fetch_session_token(&client, &base_url).await?;
    let mut mismatches = Vec::new();

    for case in &cases {
        let mut vars = BTreeMap::from([("SESSION_TOKEN".to_string(), admin_token.clone())]);
        if let Some(setup) = &case.setup {
            apply_setup_steps_async(&client, &base_url, setup, &mut vars).await?;
        }

        let left = execute_case_async(&client, &base_url, case, &vars).await?;
        let right = execute_case_async(&client, &base_url, case, &vars).await?;
        let report = compare_responses(&case.id, &left, &right, &allowlist, case.comparison);

        if !report.matches {
            let output = PathBuf::from(&config.output_dir);
            write_diff_report(&output, &report)?;
            mismatches.push(case.id.clone());
        }
    }

    server.abort();
    let _ = server.await;

    if !mismatches.is_empty() {
        anyhow::bail!(
            "final cutover rust-only compatibility mismatches: {}",
            mismatches.join(", ")
        );
    }

    Ok(())
}

fn collect_final_cutover_cases<'a>(
    config: &'a HarnessConfig,
) -> anyhow::Result<Vec<&'a CaseConfig>> {
    let mut cases = Vec::with_capacity(FINAL_CUTOVER_CASE_IDS.len());

    for case_id in FINAL_CUTOVER_CASE_IDS {
        let case = config
            .cases
            .iter()
            .find(|it| it.id == *case_id)
            .with_context(|| format!("missing final cutover compat case: {case_id}"))?;

        cases.push(case);
    }

    Ok(cases)
}

fn assert_case_has_no_live_kotlin_dependency(case: &CaseConfig) {
    assert!(
        !case.path.contains(FORBIDDEN_LIVE_KOTLIN_ENV),
        "final-mode path must not reference live Kotlin env in {}",
        case.id
    );
    if let Some(body) = &case.body {
        assert!(
            !body.contains(FORBIDDEN_LIVE_KOTLIN_ENV),
            "final-mode body must not reference live Kotlin env in {}",
            case.id
        );
    }
    if let Some(headers) = &case.headers {
        for (name, value) in headers {
            assert!(
                !name.contains(FORBIDDEN_LIVE_KOTLIN_ENV),
                "final-mode header name must not reference live Kotlin env in {}",
                case.id
            );
            assert!(
                !value.contains(FORBIDDEN_LIVE_KOTLIN_ENV),
                "final-mode header value must not reference live Kotlin env in {}",
                case.id
            );
        }
    }
    if let Some(setup) = &case.setup {
        for step in setup {
            assert_setup_step_has_no_live_kotlin_dependency(&case.id, step);
        }
    }
}

fn assert_setup_step_has_no_live_kotlin_dependency(case_id: &str, step: &SetupStep) {
    assert!(
        !step.path.contains(FORBIDDEN_LIVE_KOTLIN_ENV),
        "final-mode setup path must not reference live Kotlin env in {}",
        case_id
    );

    if let Some(headers) = &step.headers {
        for (name, value) in headers {
            assert!(
                !name.contains(FORBIDDEN_LIVE_KOTLIN_ENV),
                "final-mode setup header name must not reference live Kotlin env in {}",
                case_id
            );
            assert!(
                !value.contains(FORBIDDEN_LIVE_KOTLIN_ENV),
                "final-mode setup header value must not reference live Kotlin env in {}",
                case_id
            );
        }
    }

    if let Some(extract_headers) = &step.extract_headers {
        for (name, value) in extract_headers {
            assert!(
                !name.contains(FORBIDDEN_LIVE_KOTLIN_ENV),
                "final-mode setup extract key must not reference live Kotlin env in {}",
                case_id
            );
            assert!(
                !value.contains(FORBIDDEN_LIVE_KOTLIN_ENV),
                "final-mode setup extract value must not reference live Kotlin env in {}",
                case_id
            );
        }
    }
}

async fn fetch_session_token(client: &reqwest::Client, base_url: &str) -> anyhow::Result<String> {
    let response = client
        .get(format!(
            "{}{}",
            base_url.trim_end_matches('/'),
            "/api/v2/users/me"
        ))
        .header("Authorization", format!("Basic {ADMIN_BASIC_AUTH}"))
        .header("X-Auth-Token", "")
        .send()
        .await
        .context("login request should send")?;

    if !response.status().is_success() {
        anyhow::bail!("login should succeed for final cutover harness");
    }

    response
        .headers()
        .get("X-Auth-Token")
        .context("login response should include X-Auth-Token")?
        .to_str()
        .context("X-Auth-Token should be valid UTF-8")
        .map(ToString::to_string)
}

async fn apply_setup_steps_async(
    client: &reqwest::Client,
    base_url: &str,
    steps: &[SetupStep],
    vars: &mut BTreeMap<String, String>,
) -> anyhow::Result<()> {
    for step in steps {
        let mut request = match step.method.as_str() {
            "GET" => client.get(format!(
                "{}{}",
                base_url.trim_end_matches('/'),
                step.path.as_str()
            )),
            other => anyhow::bail!("unsupported setup method in cutover harness: {other}"),
        };

        if let Some(headers) = resolve_headers(&step.headers, vars)? {
            let mut header_map = HeaderMap::new();
            for (name, value) in headers {
                header_map.insert(
                    HeaderName::from_bytes(name.as_bytes())?,
                    HeaderValue::from_str(&value)?,
                );
            }
            request = request.headers(header_map);
        }

        let response = request.send().await?;
        if !response.status().is_success() {
            anyhow::bail!(
                "setup step '{}' returned HTTP {}",
                step.name,
                response.status().as_u16()
            );
        }

        if let Some(extract_headers) = &step.extract_headers {
            for (var_name, header_name) in extract_headers {
                let value = response
                    .headers()
                    .get(header_name)
                    .and_then(|value| value.to_str().ok())
                    .with_context(|| {
                        format!(
                            "setup response missing extractable header '{header_name}' for '{}': {}",
                            step.name, var_name
                        )
                    })?
                    .to_string();
                vars.insert(var_name.clone(), value);
            }
        }
    }

    Ok(())
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
        "POST" => client.post(format!(
            "{}{}",
            base_url.trim_end_matches('/'),
            case.path.as_str()
        )),
        "PUT" => client.put(format!(
            "{}{}",
            base_url.trim_end_matches('/'),
            case.path.as_str()
        )),
        "DELETE" => client.delete(format!(
            "{}{}",
            base_url.trim_end_matches('/'),
            case.path.as_str()
        )),
        "PATCH" => client.patch(format!(
            "{}{}",
            base_url.trim_end_matches('/'),
            case.path.as_str()
        )),
        other => anyhow::bail!("unsupported method in cutover harness: {other}"),
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

fn release_cutover_runtime_config() -> komga_rust::config::RuntimeConfig {
    let mut config =
        komga_rust::config::RuntimeConfig::for_compat_profile(komga_rust::config::CompatProfile::SnapshotAligned);
    let root = release_cutover_runtime_root();
    config.config_dir = Some(root.clone());
    config.database_file = root.join("database.sqlite");
    config.tasks_db_file = root.join("tasks.sqlite");
    config.lucene_data_directory = root.join("lucene");
    config.fonts_data_directory = root.join("fonts");
    config.log_file = root.join("komga.log");
    config
}

fn release_cutover_runtime_root() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_nanos();
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("full-cutover-runtime-{nanos}"))
}

async fn prepare_release_cutover_storage(
    config: &komga_rust::config::RuntimeConfig,
) -> anyhow::Result<()> {
    let config_dir = config
        .config_dir
        .as_ref()
        .expect("full cutover runtime should have config dir");
    fs::create_dir_all(config_dir)?;
    fs::create_dir_all(&config.lucene_data_directory)?;
    fs::create_dir_all(&config.fonts_data_directory)?;

    persistence_seed_fixture::seed_main_db_from_flyway(&config.database_file).await?;
    persistence_seed_fixture::seed_tasks_db_from_flyway(&config.tasks_db_file).await?;

    Ok(())
}
