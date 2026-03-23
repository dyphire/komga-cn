#[path = "compat/http.rs"]
mod compat_http;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::util::ServiceExt;

use compat_http::{page_content_ids, response_json, session_token_for_basic_auth};

const SEARCH_OWNERSHIP_HEADER: &str = "x-komga-compat-search-ownership";
const NATIVE_OWNERSHIP_MARKER: &str = "native-rust-owned";
const NON_NATIVE_OWNERSHIP_MARKER: &str = "shadow-java-writer";
const USER_BASIC_AUTH: &str = "dXNlckBleGFtcGxlLm9yZzp1c2Vy";
const LIMITED_BASIC_AUTH: &str = "bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk";
const RESTRICTED_BASIC_AUTH: &str = "cmVzdHJpY3RlZEBleGFtcGxlLm9yZzpyZXN0cmljdGVk";

struct OwnedBrowseRouteCase<'a> {
    name: &'a str,
    basic_auth: &'a str,
    uri: &'a str,
    expected_ids: &'a [&'a str],
    expected_page_number: u64,
    expected_page_size: u64,
    expected_total_elements: u64,
}

struct ExplicitNonNativeCase<'a> {
    name: &'a str,
    uri: &'a str,
    expected_shape: &'a str,
}

#[tokio::test]
async fn phase9_owned_browse_list_routes_are_native_owned() {
    let app = komga_rust::app::build_router();
    let cases = [
        OwnedBrowseRouteCase {
            name: "default browse list is native-owned",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists",
            expected_ids: &["readlist-1", "readlist-2", "readlist-3"],
            expected_page_number: 0,
            expected_page_size: 20,
            expected_total_elements: 3,
        },
        OwnedBrowseRouteCase {
            name: "explicit page and size stay native-owned",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?page=1&size=1",
            expected_ids: &["readlist-2"],
            expected_page_number: 1,
            expected_page_size: 1,
            expected_total_elements: 3,
        },
        OwnedBrowseRouteCase {
            name: "repeated library_id stays native-owned",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?library_id=1&library_id=2",
            expected_ids: &["readlist-1", "readlist-2", "readlist-3"],
            expected_page_number: 0,
            expected_page_size: 20,
            expected_total_elements: 3,
        },
        OwnedBrowseRouteCase {
            name: "repeated library_id with paging stays native-owned",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?library_id=1&library_id=2&page=1&size=1",
            expected_ids: &["readlist-2"],
            expected_page_number: 1,
            expected_page_size: 1,
            expected_total_elements: 3,
        },
        OwnedBrowseRouteCase {
            name: "size zero parity bucket is still native-owned",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?size=0",
            expected_ids: &["readlist-1", "readlist-2", "readlist-3"],
            expected_page_number: 0,
            expected_page_size: 20,
            expected_total_elements: 3,
        },
        OwnedBrowseRouteCase {
            name: "unauthorized library intersection stays native-owned and fail-closed",
            basic_auth: LIMITED_BASIC_AUTH,
            uri: "/api/v1/readlists?library_id=2",
            expected_ids: &[],
            expected_page_number: 0,
            expected_page_size: 20,
            expected_total_elements: 0,
        },
    ];

    assert_native_owned_cases(&app, &cases).await;
}

#[tokio::test]
async fn phase10_search_only_browse_routes_are_native_owned() {
    let app = komga_rust::app::build_router();
    let cases = [
        OwnedBrowseRouteCase {
            name: "search-only browse list is native-owned",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=alpha",
            expected_ids: &["readlist-1", "readlist-2"],
            expected_page_number: 0,
            expected_page_size: 20,
            expected_total_elements: 2,
        },
        OwnedBrowseRouteCase {
            name: "search plus library filter stays native-owned",
            basic_auth: USER_BASIC_AUTH,
            uri: "/api/v1/readlists?search=alpha&library_id=1",
            expected_ids: &["readlist-1", "readlist-2"],
            expected_page_number: 0,
            expected_page_size: 20,
            expected_total_elements: 2,
        },
    ];

    assert_native_owned_cases(&app, &cases).await;
}

#[tokio::test]
async fn phase9_dependency_routes_remain_unchanged() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;
    let restricted_token = session_token_for_basic_auth(&app, RESTRICTED_BASIC_AUTH).await;

    let readlist_detail = get_response(&app, &token, "/api/v1/readlists/readlist-2").await;
    assert_eq!(readlist_detail.status(), StatusCode::OK);
    assert_eq!(ownership_header(&readlist_detail), None);
    let readlist_detail_json = response_json(readlist_detail).await;
    assert_eq!(readlist_detail_json["id"], "readlist-2");
    assert_eq!(string_array(&readlist_detail_json["bookIds"]), vec!["book-1", "book-2"]);
    assert_eq!(readlist_detail_json["filtered"], false);
    assert!(readlist_detail_json.get("_compat").is_none());

    let restricted_detail = get_response(&app, &restricted_token, "/api/v1/readlists/readlist-2").await;
    assert_eq!(restricted_detail.status(), StatusCode::OK);
    assert_eq!(ownership_header(&restricted_detail), None);
    let restricted_detail_json = response_json(restricted_detail).await;
    assert_eq!(restricted_detail_json["id"], "readlist-2");
    assert_eq!(string_array(&restricted_detail_json["bookIds"]), vec!["book-1"]);
    assert_eq!(restricted_detail_json["filtered"], true);
    assert!(restricted_detail_json.get("_compat").is_none());

    let paged_books = get_response(&app, &token, "/api/v1/readlists/readlist-2/books?page=0&size=20").await;
    assert_eq!(paged_books.status(), StatusCode::OK);
    assert_eq!(ownership_header(&paged_books), Some(NATIVE_OWNERSHIP_MARKER));
    let paged_books_json = response_json(paged_books).await;
    assert_eq!(page_content_ids(&paged_books_json), vec!["book-1", "book-2"]);
    assert!(paged_books_json.get("_compat").is_none());

    let unpaged_books = get_response(&app, &token, "/api/v1/readlists/readlist-2/books?unpaged=true").await;
    assert_eq!(unpaged_books.status(), StatusCode::OK);
    assert_eq!(ownership_header(&unpaged_books), Some(NATIVE_OWNERSHIP_MARKER));
    let unpaged_books_json = response_json(unpaged_books).await;
    assert_eq!(page_content_ids(&unpaged_books_json), vec!["book-1", "book-2"]);
    assert_eq!(unpaged_books_json["pageable"]["paged"], false);
    assert_eq!(unpaged_books_json["pageable"]["unpaged"], true);
    assert!(unpaged_books_json.get("_compat").is_none());
}

#[tokio::test]
async fn phase9_excluded_browse_list_variants_stay_explicitly_non_native() {
    let app = komga_rust::app::build_router();
    let cases = [
        ExplicitNonNativeCase {
            name: "blank search stays non-native",
            uri: "/api/v1/readlists?search=",
            expected_shape: "UnsupportedBookFilter(search)",
        },
        ExplicitNonNativeCase {
            name: "whitespace-only search stays non-native",
            uri: "/api/v1/readlists?search=%20%20",
            expected_shape: "UnsupportedBookFilter(search)",
        },
        ExplicitNonNativeCase {
            name: "explicit unpaged true stays non-native",
            uri: "/api/v1/readlists?unpaged=true",
            expected_shape: "UnsupportedBookFilter(unpaged)",
        },
        ExplicitNonNativeCase {
            name: "explicit unpaged false still widens the request and stays non-native",
            uri: "/api/v1/readlists?unpaged=false",
            expected_shape: "UnsupportedBookFilter(unpaged)",
        },
        ExplicitNonNativeCase {
            name: "explicit sort stays non-native",
            uri: "/api/v1/readlists?sort=name,desc",
            expected_shape: "UnsupportedBookSort(name,desc)",
        },
        ExplicitNonNativeCase {
            name: "search plus sort stays non-native",
            uri: "/api/v1/readlists?search=alpha&sort=name,asc",
            expected_shape: "UnsupportedBookSort(name,asc)",
        },
        ExplicitNonNativeCase {
            name: "search plus unpaged stays non-native",
            uri: "/api/v1/readlists?search=alpha&unpaged=true",
            expected_shape: "UnsupportedBookFilter(unpaged)",
        },
        ExplicitNonNativeCase {
            name: "library plus unpaged stays non-native",
            uri: "/api/v1/readlists?library_id=1&unpaged=true",
            expected_shape: "UnsupportedBookFilter(unpaged)",
        },
        ExplicitNonNativeCase {
            name: "library plus sort stays non-native",
            uri: "/api/v1/readlists?library_id=1&page=0&size=20&sort=name,desc",
            expected_shape: "UnsupportedBookSort(name,desc)",
        },
        ExplicitNonNativeCase {
            name: "duplicate page stays non-native",
            uri: "/api/v1/readlists?search=alpha&page=0&page=1",
            expected_shape: "UnsupportedBookFilter(page)",
        },
        ExplicitNonNativeCase {
            name: "duplicate size stays non-native",
            uri: "/api/v1/readlists?search=alpha&size=20&size=1",
            expected_shape: "UnsupportedBookFilter(size)",
        },
        ExplicitNonNativeCase {
            name: "unsupported extra query key stays non-native",
            uri: "/api/v1/readlists?search=alpha&foo=bar",
            expected_shape: "UnsupportedBookFilter(foo)",
        },
    ];

    assert_explicit_non_native_cases(&app, &cases).await;

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

async fn assert_native_owned_cases<S>(app: &S, cases: &[OwnedBrowseRouteCase<'_>])
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
        "Phase 9 readlists browse-list owned routing gaps:\n- {}",
        failures.join("\n- ")
    );
}

async fn assert_explicit_non_native_cases<S>(app: &S, cases: &[ExplicitNonNativeCase<'_>])
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let mut failures = Vec::new();

    for case in cases {
        let token = session_token_for_basic_auth(app, USER_BASIC_AUTH).await;
        let response = get_response(app, &token, case.uri).await;
        let header = ownership_header(&response).map(str::to_string);
        let status = response.status();

        if status != StatusCode::OK {
            failures.push(format!(
                "{}: expected HTTP 200 explicit non-native response but got {} for {}",
                case.name, status, case.uri
            ));
            continue;
        }

        let json = response_json(response).await;
        let mut problems = Vec::new();

        if header.as_deref() != Some(NON_NATIVE_OWNERSHIP_MARKER) {
            problems.push(format!(
                "expected ownership marker {:?}, got {:?}",
                NON_NATIVE_OWNERSHIP_MARKER, header
            ));
        }
        if json["_compat"]["discoveryOwnership"] != Value::String("non-native".to_string()) {
            problems.push(format!(
                "expected _compat.discoveryOwnership=non-native, got {}",
                json["_compat"]["discoveryOwnership"]
            ));
        }
        if json["_compat"]["reason"] != Value::String("unsupported-request-shape".to_string()) {
            problems.push(format!(
                "expected _compat.reason=unsupported-request-shape, got {}",
                json["_compat"]["reason"]
            ));
        }
        if json["_compat"]["shape"] != Value::String(case.expected_shape.to_string()) {
            problems.push(format!(
                "expected _compat.shape={}, got {}",
                case.expected_shape, json["_compat"]["shape"]
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
        "Phase 9 readlists browse-list exclusion routing gaps:\n- {}",
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
