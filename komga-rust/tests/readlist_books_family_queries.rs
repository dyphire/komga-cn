#[path = "compat/http.rs"]
mod compat_http;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::util::ServiceExt;

use compat_http::{page_content_ids, response_json, session_token_for_basic_auth};

const USER_BASIC_AUTH: &str = "dXNlckBleGFtcGxlLm9yZzp1c2Vy";
const LIMITED_BASIC_AUTH: &str = "bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk";
const RESTRICTED_BASIC_AUTH: &str = "cmVzdHJpY3RlZEBleGFtcGxlLm9yZzpyZXN0cmljdGVk";

struct QueryParityReadListBooksCase<'a> {
    name: &'a str,
    basic_auth: &'a str,
    uri: &'a str,
    expected_ids: &'a [&'a str],
    expected_total_elements: u64,
    expected_page_number: u64,
    expected_page_size: u64,
}

#[tokio::test]
async fn phase8_query_parity_paged_readlist_books_variants_match_java_contract() {
    let app = komga_rust::app::build_router();
    let cases = [
        QueryParityReadListBooksCase {
            name: "default paging uses page=0 size=20 semantics",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books",
            expected_ids: &["book-1", "book-2"],
            expected_total_elements: 2,
            expected_page_number: 0,
            expected_page_size: 20,
        },
        QueryParityReadListBooksCase {
            name: "explicit page and size slice ordered readlist results",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=1&size=1",
            expected_ids: &["book-2"],
            expected_total_elements: 2,
            expected_page_number: 1,
            expected_page_size: 1,
        },
        QueryParityReadListBooksCase {
            name: "explicit unpaged=false stays in paged contract bucket",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?unpaged=false",
            expected_ids: &["book-1", "book-2"],
            expected_total_elements: 2,
            expected_page_number: 0,
            expected_page_size: 20,
        },
    ];

    assert_query_parity_cases_match_java_contract(&app, &cases).await;
}

#[tokio::test]
async fn phase8_query_parity_filter_readlist_books_variants_match_java_contract() {
    let app = komga_rust::app::build_router();
    let cases = [
        QueryParityReadListBooksCase {
            name: "library_id narrows visible readlist books while preserving query parity",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=0&size=20&library_id=1",
            expected_ids: &["book-1", "book-2"],
            expected_total_elements: 2,
            expected_page_number: 0,
            expected_page_size: 20,
        },
        QueryParityReadListBooksCase {
            name: "read_status filter keeps only matching books",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=0&size=20&read_status=READ",
            expected_ids: &["book-1"],
            expected_total_elements: 1,
            expected_page_number: 0,
            expected_page_size: 20,
        },
        QueryParityReadListBooksCase {
            name: "media_status filter fail-closes to empty results when nothing matches",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=0&size=20&media_status=UNSUPPORTED",
            expected_ids: &[],
            expected_total_elements: 0,
            expected_page_number: 0,
            expected_page_size: 20,
        },
        QueryParityReadListBooksCase {
            name: "repeated tag params are OR-combined instead of being ignored",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=0&size=20&tag=safe&tag=missing",
            expected_ids: &["book-1"],
            expected_total_elements: 1,
            expected_page_number: 0,
            expected_page_size: 20,
        },
        QueryParityReadListBooksCase {
            name: "repeated author params are OR-combined using author=name,role semantics",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=0&size=20&author=alice,writer&author=charlie,writer",
            expected_ids: &["book-1"],
            expected_total_elements: 1,
            expected_page_number: 0,
            expected_page_size: 20,
        },
        QueryParityReadListBooksCase {
            name: "deleted=true is a fail-closed empty query on visible seeded data",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=0&size=20&deleted=true",
            expected_ids: &[],
            expected_total_elements: 0,
            expected_page_number: 0,
            expected_page_size: 20,
        },
        QueryParityReadListBooksCase {
            name: "combined filters intersect correctly under paged semantics",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=0&size=20&read_status=READ&media_status=READY&tag=safe&author=alice,writer&deleted=false",
            expected_ids: &["book-1"],
            expected_total_elements: 1,
            expected_page_number: 0,
            expected_page_size: 20,
        },
        QueryParityReadListBooksCase {
            name: "empty results stay correct when filters exclude every book",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=0&size=20&read_status=READ&author=bob,writer",
            expected_ids: &[],
            expected_total_elements: 0,
            expected_page_number: 0,
            expected_page_size: 20,
        },
    ];

    assert_query_parity_cases_match_java_contract(&app, &cases).await;
}

#[tokio::test]
async fn phase8_query_parity_fail_closed_readlist_books_cases_match_java_contract() {
    let app = komga_rust::app::build_router();
    let cases = [
        QueryParityReadListBooksCase {
            name: "unauthorized library scope collapses to empty results instead of leaking content",
            basic_auth: LIMITED_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=0&size=20&library_id=2",
            expected_ids: &[],
            expected_total_elements: 0,
            expected_page_number: 0,
            expected_page_size: 20,
        },
        QueryParityReadListBooksCase {
            name: "restricted-content filtering returns only still-visible books under paged semantics",
            basic_auth: RESTRICTED_BASIC_AUTH,
            uri: "/api/v1/readlists/readlist-2/books?page=0&size=20",
            expected_ids: &["book-1"],
            expected_total_elements: 1,
            expected_page_number: 0,
            expected_page_size: 20,
        },
    ];

    assert_query_parity_cases_match_java_contract(&app, &cases).await;
}

async fn assert_query_parity_cases_match_java_contract<S>(
    app: &S,
    cases: &[QueryParityReadListBooksCase<'_>],
)
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let mut failures = Vec::new();

    for case in cases {
        let token = session_token_for_basic_auth(app, case.basic_auth).await;
        let response = get_response(app, &token, case.uri).await;
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
        "Phase 8 readlist-books query parity gaps:\n- {}",
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
