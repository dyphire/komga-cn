#[path = "compat/http.rs"]
mod compat_http;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::util::ServiceExt;

use compat_http::{page_content_ids, response_json, session_token_for_basic_auth};

const SEARCH_OWNERSHIP_HEADER: &str = "x-komga-compat-search-ownership";
const NATIVE_OWNERSHIP_MARKER: &str = "native-rust-owned";
const USER_BASIC_AUTH: &str = "dXNlckBleGFtcGxlLm9yZzp1c2Vy";
const LIMITED_BASIC_AUTH: &str = "bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk";
const RESTRICTED_BASIC_AUTH: &str = "cmVzdHJpY3RlZEBleGFtcGxlLm9yZzpyZXN0cmljdGVk";

struct BrowseQueryParityCase<'a> {
    name: &'a str,
    basic_auth: &'a str,
    uri: &'a str,
    expected_ids: &'a [&'a str],
    expected_filtered: &'a [(&'a str, bool)],
    expected_total_elements: u64,
    expected_page_number: u64,
    expected_page_size: u64,
    expected_number_of_elements: u64,
}

struct SearchQueryParityCase<'a> {
    name: &'a str,
    basic_auth: &'a str,
    uri: &'a str,
    expected_ids: &'a [&'a str],
    expected_filtered: &'a [(&'a str, bool)],
    expected_total_elements: u64,
    expected_page_number: u64,
    expected_page_size: u64,
    expected_number_of_elements: u64,
}

#[tokio::test]
async fn phase9_query_parity_owned_browse_variants_match_java_contract() {
    let app = komga_rust::app::build_router();
    let cases = [
        BrowseQueryParityCase {
            name: "default browse uses page=0 size=20 semantics",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists",
            expected_ids: &["readlist-1", "readlist-2", "readlist-3"],
            expected_filtered: &[
                ("readlist-1", false),
                ("readlist-2", false),
                ("readlist-3", false),
            ],
            expected_total_elements: 3,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 3,
        },
        BrowseQueryParityCase {
            name: "explicit page and size slice browse readlists by name order",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?page=1&size=1",
            expected_ids: &["readlist-2"],
            expected_filtered: &[("readlist-2", false)],
            expected_total_elements: 3,
            expected_page_number: 1,
            expected_page_size: 1,
            expected_number_of_elements: 1,
        },
        BrowseQueryParityCase {
            name: "repeated library_id stays in browse contract bucket",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?library_id=1&library_id=2",
            expected_ids: &["readlist-1", "readlist-2", "readlist-3"],
            expected_filtered: &[
                ("readlist-1", false),
                ("readlist-2", false),
                ("readlist-3", false),
            ],
            expected_total_elements: 3,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 3,
        },
        BrowseQueryParityCase {
            name: "repeated library_id with page and size preserves paged browse semantics",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?library_id=1&library_id=2&page=1&size=1",
            expected_ids: &["readlist-2"],
            expected_filtered: &[("readlist-2", false)],
            expected_total_elements: 3,
            expected_page_number: 1,
            expected_page_size: 1,
            expected_number_of_elements: 1,
        },
        BrowseQueryParityCase {
            name: "size zero matches current JVM browse-list pageable semantics exactly",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?size=0",
            expected_ids: &["readlist-1", "readlist-2", "readlist-3"],
            expected_filtered: &[
                ("readlist-1", false),
                ("readlist-2", false),
                ("readlist-3", false),
            ],
            expected_total_elements: 3,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 3,
        },
        BrowseQueryParityCase {
            name: "authorized library intersection fail-closes instead of widening results",
            basic_auth: LIMITED_BASIC_AUTH,
            uri: "/api/v1/readlists?library_id=2",
            expected_ids: &[],
            expected_filtered: &[],
            expected_total_elements: 0,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 0,
        },
        BrowseQueryParityCase {
            name: "partially visible readlists stay present with filtered true while fully denied readlists are omitted",
            basic_auth: RESTRICTED_BASIC_AUTH,
            uri: "/api/v1/readlists",
            expected_ids: &["readlist-1", "readlist-2"],
            expected_filtered: &[("readlist-1", false), ("readlist-2", true)],
            expected_total_elements: 2,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 2,
        },
        BrowseQueryParityCase {
            name: "empty result pages preserve totals and requested paging metadata",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?page=2&size=2",
            expected_ids: &[],
            expected_filtered: &[],
            expected_total_elements: 3,
            expected_page_number: 2,
            expected_page_size: 2,
            expected_number_of_elements: 0,
        },
    ];

    assert_query_parity_cases_match_java_contract(&app, &cases).await;
}

#[tokio::test]
async fn phase10_search_query_shapes_are_native_owned_and_filter_results() {
    let app = komga_rust::app::build_router();
    let cases = [
        SearchQueryParityCase {
            name: "non-blank search is native-owned and filters readlists",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=alpha",
            expected_ids: &["readlist-1", "readlist-2"],
            expected_filtered: &[
                ("readlist-1", false),
                ("readlist-2", false),
            ],
            expected_total_elements: 2,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 2,
        },
        SearchQueryParityCase {
            name: "paged search keeps browse paging semantics while remaining filtered",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=alpha&page=1&size=1",
            expected_ids: &["readlist-2"],
            expected_filtered: &[("readlist-2", false)],
            expected_total_elements: 2,
            expected_page_number: 1,
            expected_page_size: 1,
            expected_number_of_elements: 1,
        },
        SearchQueryParityCase {
            name: "repeated library search is native-owned and filtered",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=alpha&library_id=1&library_id=2",
            expected_ids: &["readlist-1", "readlist-2"],
            expected_filtered: &[
                ("readlist-1", false),
                ("readlist-2", false),
            ],
            expected_total_elements: 2,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 2,
        },
        SearchQueryParityCase {
            name: "repeated library paged search remains native-owned and filtered",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=alpha&library_id=1&library_id=2&page=1&size=1",
            expected_ids: &["readlist-2"],
            expected_filtered: &[("readlist-2", false)],
            expected_total_elements: 2,
            expected_page_number: 1,
            expected_page_size: 1,
            expected_number_of_elements: 1,
        },
        SearchQueryParityCase {
            name: "size zero search preserves browse size-zero semantics while filtered",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=alpha&size=0",
            expected_ids: &["readlist-1", "readlist-2"],
            expected_filtered: &[
                ("readlist-1", false),
                ("readlist-2", false),
            ],
            expected_total_elements: 2,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 2,
        },
        SearchQueryParityCase {
            name: "repeated library size zero search stays native-owned and filtered",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=alpha&library_id=1&library_id=2&size=0",
            expected_ids: &["readlist-1", "readlist-2"],
            expected_filtered: &[
                ("readlist-1", false),
                ("readlist-2", false),
            ],
            expected_total_elements: 2,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 2,
        },
        SearchQueryParityCase {
            name: "no-match search returns empty native-owned result page",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=zzzz-no-match",
            expected_ids: &[],
            expected_filtered: &[],
            expected_total_elements: 0,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 0,
        },
        SearchQueryParityCase {
            name: "permission-filtered search keeps ACL behavior while remaining native-owned",
            basic_auth: RESTRICTED_BASIC_AUTH,
            uri: "/api/v1/readlists?search=alpha",
            expected_ids: &["readlist-1", "readlist-2"],
            expected_filtered: &[("readlist-1", false), ("readlist-2", true)],
            expected_total_elements: 2,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 2,
        },
    ];

    assert_search_query_parity_cases(&app, &cases).await;
}

#[tokio::test]
async fn phase11_blank_effective_search_falls_back_to_native_browse_semantics() {
    let app = komga_rust::app::build_router();
    let cases = [
        BrowseQueryParityCase {
            name: "protocol-only blank-effective search falls back to browse",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=",
            expected_ids: &["readlist-1", "readlist-2", "readlist-3"],
            expected_filtered: &[
                ("readlist-1", false),
                ("readlist-2", false),
                ("readlist-3", false),
            ],
            expected_total_elements: 3,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 3,
        },
        BrowseQueryParityCase {
            name: "decoded whitespace-only search falls back to browse",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=%20%20",
            expected_ids: &["readlist-1", "readlist-2", "readlist-3"],
            expected_filtered: &[
                ("readlist-1", false),
                ("readlist-2", false),
                ("readlist-3", false),
            ],
            expected_total_elements: 3,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 3,
        },
        BrowseQueryParityCase {
            name: "blank-effective search with paging keeps browse pagination",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=%20%20&page=1&size=1",
            expected_ids: &["readlist-2"],
            expected_filtered: &[("readlist-2", false)],
            expected_total_elements: 3,
            expected_page_number: 1,
            expected_page_size: 1,
            expected_number_of_elements: 1,
        },
        BrowseQueryParityCase {
            name: "blank-effective search with repeated library_id keeps browse bucket",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=%20%20&library_id=1&library_id=2",
            expected_ids: &["readlist-1", "readlist-2", "readlist-3"],
            expected_filtered: &[
                ("readlist-1", false),
                ("readlist-2", false),
                ("readlist-3", false),
            ],
            expected_total_elements: 3,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 3,
        },
        BrowseQueryParityCase {
            name: "blank-effective search with size zero preserves browse size-zero semantics",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=%20%20&size=0",
            expected_ids: &["readlist-1", "readlist-2", "readlist-3"],
            expected_filtered: &[
                ("readlist-1", false),
                ("readlist-2", false),
                ("readlist-3", false),
            ],
            expected_total_elements: 3,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 3,
        },
        BrowseQueryParityCase {
            name: "blank-effective search preserves ACL filtered visibility semantics",
            basic_auth: RESTRICTED_BASIC_AUTH,
            uri: "/api/v1/readlists?search=%20%20",
            expected_ids: &["readlist-1", "readlist-2"],
            expected_filtered: &[("readlist-1", false), ("readlist-2", true)],
            expected_total_elements: 2,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 2,
        },
    ];

    assert_query_parity_cases_match_java_contract(&app, &cases).await;
}

#[tokio::test]
async fn phase12_remaining_browse_query_shapes_are_native_owned_with_current_contract() {
    let app = komga_rust::app::build_router();
    let cases = [
        BrowseQueryParityCase {
            name: "unpaged true remains accepted and keeps current paged metadata contract",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?unpaged=true",
            expected_ids: &["readlist-1", "readlist-2", "readlist-3"],
            expected_filtered: &[
                ("readlist-1", false),
                ("readlist-2", false),
                ("readlist-3", false),
            ],
            expected_total_elements: 3,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 3,
        },
        BrowseQueryParityCase {
            name: "explicit sort is accepted and reorders browse results",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?sort=name,desc",
            expected_ids: &["readlist-3", "readlist-2", "readlist-1"],
            expected_filtered: &[
                ("readlist-3", false),
                ("readlist-2", false),
                ("readlist-1", false),
            ],
            expected_total_elements: 3,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 3,
        },
        BrowseQueryParityCase {
            name: "search plus sort is accepted and remains filtered",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=alpha&sort=name,asc",
            expected_ids: &["readlist-1", "readlist-2"],
            expected_filtered: &[("readlist-1", false), ("readlist-2", false)],
            expected_total_elements: 2,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 2,
        },
        BrowseQueryParityCase {
            name: "search plus unpaged true is accepted and remains filtered",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=alpha&unpaged=true",
            expected_ids: &["readlist-1", "readlist-2"],
            expected_filtered: &[("readlist-1", false), ("readlist-2", false)],
            expected_total_elements: 2,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 2,
        },
        BrowseQueryParityCase {
            name: "duplicate page is accepted and keeps first page value",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=alpha&page=0&page=1",
            expected_ids: &["readlist-1", "readlist-2"],
            expected_filtered: &[("readlist-1", false), ("readlist-2", false)],
            expected_total_elements: 2,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 2,
        },
        BrowseQueryParityCase {
            name: "duplicate size is accepted and keeps first size value",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=alpha&size=20&size=1",
            expected_ids: &["readlist-1", "readlist-2"],
            expected_filtered: &[("readlist-1", false), ("readlist-2", false)],
            expected_total_elements: 2,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 2,
        },
        BrowseQueryParityCase {
            name: "unsupported extra query key is ignored instead of exclusion",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=alpha&foo=bar",
            expected_ids: &["readlist-1", "readlist-2"],
            expected_filtered: &[("readlist-1", false), ("readlist-2", false)],
            expected_total_elements: 2,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 2,
        },
        BrowseQueryParityCase {
            name: "blank search plus sort falls back to browse and stays accepted",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=&sort=name,asc",
            expected_ids: &["readlist-1", "readlist-2", "readlist-3"],
            expected_filtered: &[
                ("readlist-1", false),
                ("readlist-2", false),
                ("readlist-3", false),
            ],
            expected_total_elements: 3,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 3,
        },
        BrowseQueryParityCase {
            name: "blank search plus unpaged true falls back to browse and stays accepted",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=&unpaged=true",
            expected_ids: &["readlist-1", "readlist-2", "readlist-3"],
            expected_filtered: &[
                ("readlist-1", false),
                ("readlist-2", false),
                ("readlist-3", false),
            ],
            expected_total_elements: 3,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 3,
        },
        BrowseQueryParityCase {
            name: "blank search plus duplicate page stays accepted with first page value",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=&page=0&page=1",
            expected_ids: &["readlist-1", "readlist-2", "readlist-3"],
            expected_filtered: &[
                ("readlist-1", false),
                ("readlist-2", false),
                ("readlist-3", false),
            ],
            expected_total_elements: 3,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 3,
        },
        BrowseQueryParityCase {
            name: "blank search plus duplicate size stays accepted with first size value",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=&size=20&size=1",
            expected_ids: &["readlist-1", "readlist-2", "readlist-3"],
            expected_filtered: &[
                ("readlist-1", false),
                ("readlist-2", false),
                ("readlist-3", false),
            ],
            expected_total_elements: 3,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 3,
        },
        BrowseQueryParityCase {
            name: "blank search plus extra key stays accepted and ignored",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=&foo=bar",
            expected_ids: &["readlist-1", "readlist-2", "readlist-3"],
            expected_filtered: &[
                ("readlist-1", false),
                ("readlist-2", false),
                ("readlist-3", false),
            ],
            expected_total_elements: 3,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 3,
        },
        BrowseQueryParityCase {
            name: "duplicate search stays accepted under current multi-value contract",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=&search=%20%20",
            expected_ids: &[],
            expected_filtered: &[],
            expected_total_elements: 0,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 0,
        },
        BrowseQueryParityCase {
            name: "library filter plus unpaged stays accepted and filtered by ACL intersection",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?library_id=1&unpaged=true",
            expected_ids: &["readlist-1", "readlist-2", "readlist-3"],
            expected_filtered: &[
                ("readlist-1", false),
                ("readlist-2", false),
                ("readlist-3", false),
            ],
            expected_total_elements: 3,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 3,
        },
        BrowseQueryParityCase {
            name: "library filter plus explicit sort stays accepted with requested ordering",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?library_id=1&page=0&size=20&sort=name,desc",
            expected_ids: &["readlist-3", "readlist-2", "readlist-1"],
            expected_filtered: &[
                ("readlist-3", false),
                ("readlist-2", false),
                ("readlist-1", false),
            ],
            expected_total_elements: 3,
            expected_page_number: 0,
            expected_page_size: 20,
            expected_number_of_elements: 3,
        },
    ];

    assert_query_parity_cases_match_java_contract(&app, &cases).await;
}

async fn assert_query_parity_cases_match_java_contract<S>(
    app: &S,
    cases: &[BrowseQueryParityCase<'_>],
) where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let mut failures = Vec::new();

    for case in cases {
        let token = session_token_for_basic_auth(app, case.basic_auth).await;
        let response = get_response(app, &token, case.uri).await;
        let header = ownership_header(&response).map(str::to_string);
        let status = response.status();

        if status != StatusCode::OK {
            failures.push(format!(
                "{}: expected HTTP 200 but got {} for {}",
                case.name, status, case.uri
            ));
            continue;
        }

        let json = response_json(response).await;
        let ids = page_content_ids(&json);
        let mut problems = Vec::new();

        if header.as_deref() != Some(NATIVE_OWNERSHIP_MARKER) {
            problems.push(format!(
                "expected ownership marker {:?}, got {:?}",
                NATIVE_OWNERSHIP_MARKER, header
            ));
        }
        if json.get("_compat").is_some() {
            problems.push(format!(
                "expected no _compat diagnostics, got {}",
                json["_compat"]
            ));
        }
        if ids != case.expected_ids {
            problems.push(format!(
                "expected ids {:?}, got {:?}",
                case.expected_ids, ids
            ));
        }
        if json["pageable"]["paged"] != Value::Bool(true) {
            problems.push(format!(
                "expected pageable.paged=true, got {}",
                json["pageable"]["paged"]
            ));
        }
        if json["pageable"]["unpaged"] != Value::Bool(false) {
            problems.push(format!(
                "expected pageable.unpaged=false, got {}",
                json["pageable"]["unpaged"]
            ));
        }
        if json["pageable"]["pageNumber"] != Value::from(case.expected_page_number) {
            problems.push(format!(
                "expected pageable.pageNumber={}, got {}",
                case.expected_page_number, json["pageable"]["pageNumber"]
            ));
        }
        if json["pageable"]["pageSize"] != Value::from(case.expected_page_size) {
            problems.push(format!(
                "expected pageable.pageSize={}, got {}",
                case.expected_page_size, json["pageable"]["pageSize"]
            ));
        }
        if json["number"] != Value::from(case.expected_page_number) {
            problems.push(format!(
                "expected number={}, got {}",
                case.expected_page_number, json["number"]
            ));
        }
        if json["size"] != Value::from(case.expected_page_size) {
            problems.push(format!(
                "expected size={}, got {}",
                case.expected_page_size, json["size"]
            ));
        }
        if json["numberOfElements"] != Value::from(case.expected_number_of_elements) {
            problems.push(format!(
                "expected numberOfElements={}, got {}",
                case.expected_number_of_elements, json["numberOfElements"]
            ));
        }
        if json["totalElements"] != Value::from(case.expected_total_elements) {
            problems.push(format!(
                "expected totalElements={}, got {}",
                case.expected_total_elements, json["totalElements"]
            ));
        }

        for (id, expected_filtered) in case.expected_filtered {
            match page_item_by_id(&json, id) {
                Some(item) if item["filtered"] == Value::Bool(*expected_filtered) => {}
                Some(item) => problems.push(format!(
                    "expected filtered={} for {}, got {}",
                    expected_filtered, id, item["filtered"]
                )),
                None => problems.push(format!(
                    "expected item {} to be present in page content",
                    id
                )),
            }
        }

        if !problems.is_empty() {
            failures.push(format!(
                "{} [{}]: {}",
                case.name,
                case.uri,
                problems.join("; ")
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "Phase 9 readlists-list browse parity gaps:\n- {}",
        failures.join("\n- ")
    );
}

async fn assert_search_query_parity_cases<S>(
    app: &S,
    cases: &[SearchQueryParityCase<'_>],
) where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let mut failures = Vec::new();

    for case in cases {
        let token = session_token_for_basic_auth(app, case.basic_auth).await;
        let response = get_response(app, &token, case.uri).await;
        let header = ownership_header(&response).map(str::to_string);
        let status = response.status();

        if status != StatusCode::OK {
            failures.push(format!(
                "{}: expected HTTP 200 but got {} for {}",
                case.name, status, case.uri
            ));
            continue;
        }

        let json = response_json(response).await;
        let ids = page_content_ids(&json);
        let mut problems = Vec::new();

        if header.as_deref() != Some(NATIVE_OWNERSHIP_MARKER) {
            problems.push(format!(
                "expected ownership marker {:?}, got {:?}",
                NATIVE_OWNERSHIP_MARKER, header
            ));
        }
        if json.get("_compat").is_some() {
            problems.push(format!(
                "expected no _compat diagnostics, got {}",
                json["_compat"]
            ));
        }
        if ids != case.expected_ids {
            problems.push(format!(
                "expected ids {:?}, got {:?}",
                case.expected_ids, ids
            ));
        }

        if json["pageable"]["paged"] != Value::Bool(true) {
            problems.push(format!(
                "expected pageable.paged=true, got {}",
                json["pageable"]["paged"]
            ));
        }
        if json["pageable"]["unpaged"] != Value::Bool(false) {
            problems.push(format!(
                "expected pageable.unpaged=false, got {}",
                json["pageable"]["unpaged"]
            ));
        }
        if json["pageable"]["pageNumber"] != Value::from(case.expected_page_number) {
            problems.push(format!(
                "expected pageable.pageNumber={}, got {}",
                case.expected_page_number, json["pageable"]["pageNumber"]
            ));
        }
        if json["pageable"]["pageSize"] != Value::from(case.expected_page_size) {
            problems.push(format!(
                "expected pageable.pageSize={}, got {}",
                case.expected_page_size, json["pageable"]["pageSize"]
            ));
        }
        if json["number"] != Value::from(case.expected_page_number) {
            problems.push(format!(
                "expected number={}, got {}",
                case.expected_page_number, json["number"]
            ));
        }
        if json["size"] != Value::from(case.expected_page_size) {
            problems.push(format!(
                "expected size={}, got {}",
                case.expected_page_size, json["size"]
            ));
        }
        if json["numberOfElements"] != Value::from(case.expected_number_of_elements) {
            problems.push(format!(
                "expected numberOfElements={}, got {}",
                case.expected_number_of_elements, json["numberOfElements"]
            ));
        }
        if json["totalElements"] != Value::from(case.expected_total_elements) {
            problems.push(format!(
                "expected totalElements={}, got {}",
                case.expected_total_elements, json["totalElements"]
            ));
        }

        for (id, expected_filtered) in case.expected_filtered {
            match page_item_by_id(&json, id) {
                Some(item) if item["filtered"] == Value::Bool(*expected_filtered) => {}
                Some(item) => problems.push(format!(
                    "expected filtered={} for {}, got {}",
                    expected_filtered, id, item["filtered"]
                )),
                None => problems.push(format!(
                    "expected item {} to be present in page content",
                    id
                )),
            }
        }

        if !problems.is_empty() {
            failures.push(format!(
                "{} [{}]: {}",
                case.name,
                case.uri,
                problems.join("; ")
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "Phase 10 readlists-list search parity gaps:\n- {}",
        failures.join("\n- ")
    );
}

fn ownership_header(response: &axum::response::Response) -> Option<&str> {
    response
        .headers()
        .get(SEARCH_OWNERSHIP_HEADER)
        .and_then(|value| value.to_str().ok())
}

fn page_item_by_id<'a>(json: &'a Value, id: &str) -> Option<&'a Value> {
    json["content"].as_array().and_then(|items| {
        items
            .iter()
            .find(|item| item["id"] == Value::String(id.to_string()))
    })
}

async fn get_response<S>(app: &S, token: &str, uri: &str) -> axum::response::Response
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    app.clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("X-Auth-Token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}
