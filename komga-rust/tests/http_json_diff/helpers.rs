use super::*;

pub(super) fn phase8_readlist_books_family_owned_case_ids() -> &'static [&'static str] {
    &[
        "P8-READLIST-BOOKS-PAGED-DEFAULT-OWNED",
        "P8-READLIST-BOOKS-PAGED-EXPLICIT-OWNED",
        "P8-READLIST-BOOKS-LIBRARY-FILTER-OWNED",
        "P8-READLIST-BOOKS-READ-STATUS-FILTER-OWNED",
        "P8-READLIST-BOOKS-MEDIA-STATUS-FILTER-OWNED",
        "P8-READLIST-BOOKS-REPEATED-TAG-FILTER-OWNED",
        "P8-READLIST-BOOKS-REPEATED-AUTHOR-FILTER-OWNED",
        "P8-READLIST-BOOKS-DELETED-TRUE-FILTER-OWNED",
        "P8-READLIST-BOOKS-DELETED-FALSE-FILTER-OWNED",
        "P8-READLIST-BOOKS-COMBINED-FILTERS-OWNED",
        "P8-READLIST-BOOKS-COMBINED-REPEATED-FILTERS-OWNED",
        "P8-READLIST-BOOKS-UNPAGED-FALSE-OWNED",
    ]
}

pub(super) fn phase8_readlist_books_family_negative_case_ids() -> &'static [&'static str] {
    &[
        "P8-READLIST-BOOKS-DEPENDENCY-UNPAGED-PREOWNED",
        "P8-READLIST-BOOKS-DEPENDENCY-UNPAGED-WIDENED-SHADOW",
        "P8-READLIST-BOOKS-EXCLUDED-READLISTS-LIST-FAMILY",
        "P8-READLIST-BOOKS-EXCLUDED-TACHIYOMI",
    ]
}

pub(super) fn phase8_readlist_books_family_all_case_ids() -> &'static [&'static str] {
    &[
        "P8-READLIST-BOOKS-PAGED-DEFAULT-OWNED",
        "P8-READLIST-BOOKS-PAGED-EXPLICIT-OWNED",
        "P8-READLIST-BOOKS-LIBRARY-FILTER-OWNED",
        "P8-READLIST-BOOKS-READ-STATUS-FILTER-OWNED",
        "P8-READLIST-BOOKS-MEDIA-STATUS-FILTER-OWNED",
        "P8-READLIST-BOOKS-REPEATED-TAG-FILTER-OWNED",
        "P8-READLIST-BOOKS-REPEATED-AUTHOR-FILTER-OWNED",
        "P8-READLIST-BOOKS-DELETED-TRUE-FILTER-OWNED",
        "P8-READLIST-BOOKS-DELETED-FALSE-FILTER-OWNED",
        "P8-READLIST-BOOKS-COMBINED-FILTERS-OWNED",
        "P8-READLIST-BOOKS-COMBINED-REPEATED-FILTERS-OWNED",
        "P8-READLIST-BOOKS-UNPAGED-FALSE-OWNED",
        "P8-READLIST-BOOKS-DEPENDENCY-UNPAGED-PREOWNED",
        "P8-READLIST-BOOKS-DEPENDENCY-UNPAGED-WIDENED-SHADOW",
        "P8-READLIST-BOOKS-EXCLUDED-READLISTS-LIST-FAMILY",
        "P8-READLIST-BOOKS-EXCLUDED-TACHIYOMI",
    ]
}

pub(super) fn phase9_readlists_list_browse_owned_case_ids() -> &'static [&'static str] {
    &[
        "P9-READLISTS-LIST-BROWSE-DEFAULT-OWNED",
        "P9-READLISTS-LIST-BROWSE-PAGE-SIZE-OWNED",
        "P9-READLISTS-LIST-BROWSE-REPEATED-LIBRARY-ID-OWNED",
        "P9-READLISTS-LIST-BROWSE-REPEATED-LIBRARY-ID-PAGE-SIZE-OWNED",
        "P9-READLISTS-LIST-BROWSE-SIZE-ZERO-OWNED",
    ]
}

pub(super) fn phase9_readlists_list_browse_negative_case_ids() -> &'static [&'static str] {
    &[
        "P9-READLISTS-LIST-BROWSE-NEGATIVE-UNPAGED-TRUE",
        "P9-READLISTS-LIST-BROWSE-NEGATIVE-SORT",
        "P9-READLISTS-LIST-BROWSE-NEGATIVE-TACHIYOMI",
    ]
}

pub(super) fn phase9_readlists_list_browse_all_case_ids() -> &'static [&'static str] {
    &[
        "P9-READLISTS-LIST-BROWSE-DEFAULT-OWNED",
        "P9-READLISTS-LIST-BROWSE-PAGE-SIZE-OWNED",
        "P9-READLISTS-LIST-BROWSE-REPEATED-LIBRARY-ID-OWNED",
        "P9-READLISTS-LIST-BROWSE-REPEATED-LIBRARY-ID-PAGE-SIZE-OWNED",
        "P9-READLISTS-LIST-BROWSE-SIZE-ZERO-OWNED",
        "P9-READLISTS-LIST-BROWSE-NEGATIVE-UNPAGED-TRUE",
        "P9-READLISTS-LIST-BROWSE-NEGATIVE-SORT",
        "P9-READLISTS-LIST-BROWSE-NEGATIVE-TACHIYOMI",
    ]
}

pub(super) fn phase10_readlists_search_owned_case_ids() -> &'static [&'static str] {
    &[
        "P10-READLISTS-SEARCH-DEFAULT-OWNED",
        "P10-READLISTS-SEARCH-PAGE-SIZE-OWNED",
        "P10-READLISTS-SEARCH-REPEATED-LIBRARY-ID-OWNED",
        "P10-READLISTS-SEARCH-REPEATED-LIBRARY-ID-PAGE-SIZE-OWNED",
        "P10-READLISTS-SEARCH-SIZE-ZERO-OWNED",
        "P10-READLISTS-SEARCH-REPEATED-LIBRARY-ID-SIZE-ZERO-OWNED",
        "P10-READLISTS-SEARCH-NO-RESULTS-OWNED",
    ]
}

pub(super) fn phase10_readlists_search_negative_case_ids() -> &'static [&'static str] {
    &[
        "P10-READLISTS-SEARCH-NEGATIVE-SORT",
        "P10-READLISTS-SEARCH-NEGATIVE-UNPAGED-TRUE",
        "P10-READLISTS-SEARCH-NEGATIVE-DUPLICATE-PAGE",
        "P10-READLISTS-SEARCH-NEGATIVE-DUPLICATE-SIZE",
        "P10-READLISTS-SEARCH-NEGATIVE-UNSUPPORTED-EXTRA",
    ]
}

pub(super) fn phase10_readlists_search_all_case_ids() -> &'static [&'static str] {
    &[
        "P10-READLISTS-SEARCH-DEFAULT-OWNED",
        "P10-READLISTS-SEARCH-PAGE-SIZE-OWNED",
        "P10-READLISTS-SEARCH-REPEATED-LIBRARY-ID-OWNED",
        "P10-READLISTS-SEARCH-REPEATED-LIBRARY-ID-PAGE-SIZE-OWNED",
        "P10-READLISTS-SEARCH-SIZE-ZERO-OWNED",
        "P10-READLISTS-SEARCH-REPEATED-LIBRARY-ID-SIZE-ZERO-OWNED",
        "P10-READLISTS-SEARCH-NO-RESULTS-OWNED",
        "P10-READLISTS-SEARCH-NEGATIVE-SORT",
        "P10-READLISTS-SEARCH-NEGATIVE-UNPAGED-TRUE",
        "P10-READLISTS-SEARCH-NEGATIVE-DUPLICATE-PAGE",
        "P10-READLISTS-SEARCH-NEGATIVE-DUPLICATE-SIZE",
        "P10-READLISTS-SEARCH-NEGATIVE-UNSUPPORTED-EXTRA",
    ]
}

pub(super) fn phase11_readlists_blank_search_owned_case_ids() -> &'static [&'static str] {
    &[
        "P11-READLISTS-BLANK-SEARCH-DEFAULT-OWNED",
        "P11-READLISTS-BLANK-SEARCH-WHITESPACE-OWNED",
        "P11-READLISTS-BLANK-SEARCH-PAGE-SIZE-OWNED",
        "P11-READLISTS-BLANK-SEARCH-REPEATED-LIBRARY-ID-OWNED",
        "P11-READLISTS-BLANK-SEARCH-REPEATED-LIBRARY-ID-PAGE-SIZE-OWNED",
        "P11-READLISTS-BLANK-SEARCH-SIZE-ZERO-OWNED",
        "P11-READLISTS-BLANK-SEARCH-REPEATED-LIBRARY-ID-SIZE-ZERO-OWNED",
    ]
}

pub(super) fn phase11_readlists_blank_search_negative_case_ids() -> &'static [&'static str] {
    &[
        "P11-READLISTS-BLANK-SEARCH-NEGATIVE-SORT",
        "P11-READLISTS-BLANK-SEARCH-NEGATIVE-UNPAGED-TRUE",
        "P11-READLISTS-BLANK-SEARCH-NEGATIVE-DUPLICATE-PAGE",
        "P11-READLISTS-BLANK-SEARCH-NEGATIVE-DUPLICATE-SIZE",
        "P11-READLISTS-BLANK-SEARCH-NEGATIVE-UNSUPPORTED-EXTRA",
        "P11-READLISTS-BLANK-SEARCH-NEGATIVE-DUPLICATE-SEARCH",
    ]
}

pub(super) fn phase11_readlists_blank_search_all_case_ids() -> &'static [&'static str] {
    &[
        "P11-READLISTS-BLANK-SEARCH-DEFAULT-OWNED",
        "P11-READLISTS-BLANK-SEARCH-WHITESPACE-OWNED",
        "P11-READLISTS-BLANK-SEARCH-PAGE-SIZE-OWNED",
        "P11-READLISTS-BLANK-SEARCH-REPEATED-LIBRARY-ID-OWNED",
        "P11-READLISTS-BLANK-SEARCH-REPEATED-LIBRARY-ID-PAGE-SIZE-OWNED",
        "P11-READLISTS-BLANK-SEARCH-SIZE-ZERO-OWNED",
        "P11-READLISTS-BLANK-SEARCH-REPEATED-LIBRARY-ID-SIZE-ZERO-OWNED",
        "P11-READLISTS-BLANK-SEARCH-NEGATIVE-SORT",
        "P11-READLISTS-BLANK-SEARCH-NEGATIVE-UNPAGED-TRUE",
        "P11-READLISTS-BLANK-SEARCH-NEGATIVE-DUPLICATE-PAGE",
        "P11-READLISTS-BLANK-SEARCH-NEGATIVE-DUPLICATE-SIZE",
        "P11-READLISTS-BLANK-SEARCH-NEGATIVE-UNSUPPORTED-EXTRA",
        "P11-READLISTS-BLANK-SEARCH-NEGATIVE-DUPLICATE-SEARCH",
    ]
}

pub(super) fn live_http_json_case_ids() -> &'static [&'static str] {
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

pub(super) fn execute_case(
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

pub(super) async fn execute_case_async(
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

pub(super) async fn apply_setup_steps_async(
    client: &reqwest::Client,
    base_url: &str,
    steps: &[komga_compat_testkit::cases::SetupStep],
    vars: &mut BTreeMap<String, String>,
) -> anyhow::Result<()> {
    for step in steps {
        let mut request = match step.method.as_str() {
            "GET" => client.get(format!(
                "{}{}",
                base_url.trim_end_matches('/'),
                step.path.as_str()
            )),
            other => anyhow::bail!("unsupported setup method in skeleton: {other}"),
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
                let value = if header_name.eq_ignore_ascii_case("X-Auth-Token") {
                    extract_session_cookie_token(response.headers()).unwrap_or_else(|| {
                        response
                            .headers()
                            .get(header_name)
                            .and_then(|value| value.to_str().ok())
                            .expect("setup response should include extractable x-auth-token")
                            .to_string()
                    })
                } else {
                    response
                        .headers()
                        .get(header_name)
                        .and_then(|value| value.to_str().ok())
                        .expect("setup response should include requested extract header")
                        .to_string()
                };
                vars.insert(var_name.clone(), value);
            }
        }
    }

    Ok(())
}

fn extract_session_cookie_token(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .find_map(|value| {
            value.to_str().ok().and_then(|cookie| {
                cookie
                    .split(';')
                    .map(str::trim)
                    .find_map(|part| part.strip_prefix("KOMGA-SESSION="))
                    .map(str::to_string)
            })
        })
}

pub(super) fn smoke_harness_config() -> HarnessConfig {
    HarnessConfig {
        output_dir: "target/compat-diff-smoke".to_string(),
        header_allowlist: vec!["content-type".to_string()],
        cases: vec![
            smoke_case("SMOKE-LIBRARIES", "/api/v1/libraries"),
            smoke_case("SMOKE-BOOKS", "/api/v1/books"),
        ],
    }
}

pub(super) fn seeded_localdb_smoke_harness_config() -> HarnessConfig {
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

pub(super) fn seeded_localdb_binary_manifest_smoke_harness_config() -> HarnessConfig {
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

pub(super) fn seeded_localdb_session_vars(
    client: &Client,
    base_url: &str,
) -> BTreeMap<String, String> {
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

pub(super) fn temp_output_root() -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_millis();
    let path = std::env::temp_dir().join(format!("komga-compat-diff-{millis}"));
    fs::create_dir_all(&path).expect("temp output root should be creatable");
    path
}
