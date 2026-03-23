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

struct OwnedReadListBooksCase<'a> {
    name: &'a str,
    basic_auth: &'a str,
    uri: &'a str,
    expected_ids: &'a [&'a str],
    expected_total_elements: u64,
    expected_page_number: u64,
    expected_page_size: u64,
}

#[tokio::test]
async fn phase8_paged_and_filter_readlist_books_variants_are_native_owned() {
    let app = komga_rust::app::build_router();
    let cases = [
        OwnedReadListBooksCase {
            name: "default paging uses page=0 size=20 semantics",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books",
            expected_ids: &["book-1", "book-2"],
            expected_total_elements: 2,
            expected_page_number: 0,
            expected_page_size: 20,
        },
        OwnedReadListBooksCase {
            name: "explicit page and size slice ordered readlist results",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=1&size=1",
            expected_ids: &["book-2"],
            expected_total_elements: 2,
            expected_page_number: 1,
            expected_page_size: 1,
        },
        OwnedReadListBooksCase {
            name: "explicit unpaged=false stays in paged contract bucket",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?unpaged=false",
            expected_ids: &["book-1", "book-2"],
            expected_total_elements: 2,
            expected_page_number: 0,
            expected_page_size: 20,
        },
        OwnedReadListBooksCase {
            name: "library_id narrows visible readlist books while preserving native ownership",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=0&size=20&library_id=1",
            expected_ids: &["book-1", "book-2"],
            expected_total_elements: 2,
            expected_page_number: 0,
            expected_page_size: 20,
        },
        OwnedReadListBooksCase {
            name: "read_status filter keeps only matching books",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=0&size=20&read_status=READ",
            expected_ids: &["book-1"],
            expected_total_elements: 1,
            expected_page_number: 0,
            expected_page_size: 20,
        },
        OwnedReadListBooksCase {
            name: "media_status filter fail-closes to empty results when nothing matches",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=0&size=20&media_status=UNSUPPORTED",
            expected_ids: &[],
            expected_total_elements: 0,
            expected_page_number: 0,
            expected_page_size: 20,
        },
        OwnedReadListBooksCase {
            name: "repeated tag params are OR-combined instead of being ignored",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=0&size=20&tag=safe&tag=missing",
            expected_ids: &["book-1"],
            expected_total_elements: 1,
            expected_page_number: 0,
            expected_page_size: 20,
        },
        OwnedReadListBooksCase {
            name: "repeated author params are OR-combined using author=name,role semantics",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=0&size=20&author=alice,writer&author=charlie,writer",
            expected_ids: &["book-1"],
            expected_total_elements: 1,
            expected_page_number: 0,
            expected_page_size: 20,
        },
        OwnedReadListBooksCase {
            name: "deleted=true is a fail-closed empty query on visible seeded data",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=0&size=20&deleted=true",
            expected_ids: &[],
            expected_total_elements: 0,
            expected_page_number: 0,
            expected_page_size: 20,
        },
        OwnedReadListBooksCase {
            name: "combined filters intersect correctly under native paged semantics",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=0&size=20&read_status=READ&media_status=READY&tag=safe&author=alice,writer&deleted=false",
            expected_ids: &["book-1"],
            expected_total_elements: 1,
            expected_page_number: 0,
            expected_page_size: 20,
        },
        OwnedReadListBooksCase {
            name: "empty results stay native when filters exclude every book",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=0&size=20&read_status=READ&author=bob,writer",
            expected_ids: &[],
            expected_total_elements: 0,
            expected_page_number: 0,
            expected_page_size: 20,
        },
        OwnedReadListBooksCase {
            name: "unauthorized library scope collapses to empty results instead of leaking content",
            basic_auth: LIMITED_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=0&size=20&library_id=2",
            expected_ids: &[],
            expected_total_elements: 0,
            expected_page_number: 0,
            expected_page_size: 20,
        },
        OwnedReadListBooksCase {
            name: "restricted-content filtering returns only still-visible books under native paged semantics",
            basic_auth: RESTRICTED_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=0&size=20",
            expected_ids: &["book-1"],
            expected_total_elements: 1,
            expected_page_number: 0,
            expected_page_size: 20,
        },
    ];

    assert_native_owned_cases_match_java_contract(&app, &cases).await;
}

#[tokio::test]
async fn phase8_dependency_only_readlist_routes_keep_preowned_markers_and_behavior() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;
    let restricted_token = session_token_for_basic_auth(&app, RESTRICTED_BASIC_AUTH).await;

    let unpaged = get_response(&app, &token, "/api/v1/readlists/readlist-2/books?unpaged=true")
        .await;
    assert_eq!(unpaged.status(), StatusCode::OK);
    assert_eq!(ownership_header(&unpaged), Some(NATIVE_OWNERSHIP_MARKER));
    let unpaged_json = response_json(unpaged).await;
    assert_eq!(page_content_ids(&unpaged_json), vec!["book-1", "book-2"]);
    assert_eq!(unpaged_json["pageable"]["paged"], false);
    assert_eq!(unpaged_json["pageable"]["unpaged"], true);
    assert!(unpaged_json.get("_compat").is_none());

    let readlist_detail = get_response(&app, &token, "/api/v1/readlists/readlist-2").await;
    assert_eq!(readlist_detail.status(), StatusCode::OK);
    assert_eq!(ownership_header(&readlist_detail), None);
    let readlist_detail_json = response_json(readlist_detail).await;
    assert_eq!(readlist_detail_json["id"], "readlist-2");
    assert_eq!(string_array(&readlist_detail_json["bookIds"]), vec!["book-1", "book-2"]);
    assert_eq!(readlist_detail_json["filtered"], false);
    assert!(readlist_detail_json.get("_compat").is_none());

    let restricted_readlist_detail =
        get_response(&app, &restricted_token, "/api/v1/readlists/readlist-2").await;
    assert_eq!(restricted_readlist_detail.status(), StatusCode::OK);
    assert_eq!(ownership_header(&restricted_readlist_detail), None);
    let restricted_readlist_detail_json = response_json(restricted_readlist_detail).await;
    assert_eq!(restricted_readlist_detail_json["id"], "readlist-2");
    assert_eq!(
        string_array(&restricted_readlist_detail_json["bookIds"]),
        vec!["book-1"],
    );
    assert_eq!(restricted_readlist_detail_json["filtered"], true);
    assert!(restricted_readlist_detail_json.get("_compat").is_none());

    let previous = get_response(
        &app,
        &token,
        "/api/v1/readlists/readlist-2/books/book-1/previous",
    )
    .await;
    assert_eq!(previous.status(), StatusCode::NOT_FOUND);
    assert_eq!(ownership_header(&previous), None);

    let next = get_response(&app, &token, "/api/v1/readlists/readlist-2/books/book-1/next")
        .await;
    assert_eq!(next.status(), StatusCode::OK);
    assert_eq!(ownership_header(&next), None);
    let next_json = response_json(next).await;
    assert_eq!(next_json["id"], "book-2");
    assert!(next_json.get("_compat").is_none());
}

#[tokio::test]
async fn phase8_list_family_and_tachiyomi_routes_remain_excluded() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;

    let tachiyomi = get_response(
        &app,
        &token,
        "/api/v1/readlists/readlist-2/read-progress/tachiyomi",
    )
    .await;
    assert_eq!(tachiyomi.status(), StatusCode::NOT_FOUND);
    assert_eq!(ownership_header(&tachiyomi), None);
}

async fn assert_native_owned_cases_match_java_contract<S>(app: &S, cases: &[OwnedReadListBooksCase<'_>])
where
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
            problems.push(format!("expected no _compat diagnostics, got {}", json["_compat"]));
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
        if json["totalElements"] != Value::from(case.expected_total_elements) {
            problems.push(format!(
                "expected totalElements={}, got {}",
                case.expected_total_elements, json["totalElements"]
            ));
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
        "Phase 8 readlist-books routing gaps:\n- {}",
        failures.join("\n- ")
    );
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

fn ownership_header(response: &axum::response::Response) -> Option<&str> {
    response
        .headers()
        .get(SEARCH_OWNERSHIP_HEADER)
        .and_then(|value| value.to_str().ok())
}

fn string_array(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .expect("payload field should be an array")
        .iter()
        .map(|it| it.as_str().expect("array entry should be a string"))
        .collect()
}
