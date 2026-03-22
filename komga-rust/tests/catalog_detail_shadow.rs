use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use komga_rust::application::discovery::{DiscoveryQueries, ReadListBooksQuery};
use komga_rust::domain::discovery::{DiscoveryError, DiscoveryQueryContext};
use komga_rust::persistence::discovery::SqliteDiscoveryAdapter;
use serde_json::Value;
use std::collections::BTreeSet;
use tower::util::ServiceExt;

const SEARCH_OWNERSHIP_HEADER: &str = "x-komga-compat-search-ownership";
const NATIVE_OWNERSHIP_MARKER: &str = "native-rust-owned";
const ADMIN_BASIC_AUTH: &str = "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=";
const USER_BASIC_AUTH: &str = "dXNlckBleGFtcGxlLm9yZzp1c2Vy";
const LIMITED_BASIC_AUTH: &str = "bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk";
const RESTRICTED_BASIC_AUTH: &str = "cmVzdHJpY3RlZEBleGFtcGxlLm9yZzpyZXN0cmljdGVk";

struct DirectBrowsePrincipalCase<'a> {
    name: &'a str,
    basic_auth: &'a str,
    expected_series_url: &'a str,
    expected_book_url: &'a str,
    expect_filtered_collection: bool,
    expect_filtered_readlist: bool,
}

struct DirectOneshotPrincipalCase<'a> {
    name: &'a str,
    basic_auth: &'a str,
    expected_series_url: &'a str,
    expected_book_url: &'a str,
}

#[tokio::test]
async fn admin_user_limited_restricted_direct_browse_matrix() {
    let app = komga_rust::app::build_router();
    let mut admin_series_books_ids: Option<Vec<String>> = None;
    let cases = [
        DirectBrowsePrincipalCase {
            name: "admin",
            basic_auth: ADMIN_BASIC_AUTH,
            expected_series_url: "/library/1/series",
            expected_book_url: "/library1/book.cbr",
            expect_filtered_collection: false,
            expect_filtered_readlist: false,
        },
        DirectBrowsePrincipalCase {
            name: "user",
            basic_auth: USER_BASIC_AUTH,
            expected_series_url: "",
            expected_book_url: "book.cbr",
            expect_filtered_collection: false,
            expect_filtered_readlist: false,
        },
        DirectBrowsePrincipalCase {
            name: "limited",
            basic_auth: LIMITED_BASIC_AUTH,
            expected_series_url: "",
            expected_book_url: "book.cbr",
            expect_filtered_collection: false,
            expect_filtered_readlist: false,
        },
        DirectBrowsePrincipalCase {
            name: "restricted",
            basic_auth: RESTRICTED_BASIC_AUTH,
            expected_series_url: "",
            expected_book_url: "book.cbr",
            expect_filtered_collection: true,
            expect_filtered_readlist: true,
        },
    ];

    for case in cases {
        let token = session_token_for_basic_auth(&app, case.basic_auth).await;

        let series_detail = get_response(&app, &token, "/api/v1/series/series-1").await;
        assert_eq!(series_detail.status(), StatusCode::OK, "{} series detail status", case.name);
        assert_native_owned(&series_detail, &format!("{} series detail", case.name));
        let series_detail_json = response_json(series_detail).await;
        assert_eq!(series_detail_json["id"], "series-1", "{} series detail id", case.name);
        assert_eq!(
            series_detail_json["url"],
            case.expected_series_url,
            "{} series url parity",
            case.name,
        );

        let series_collections = get_response(&app, &token, "/api/v1/series/series-1/collections").await;
        assert_eq!(
            series_collections.status(),
            StatusCode::OK,
            "{} series collections status",
            case.name,
        );
        assert_native_owned(&series_collections, &format!("{} series collections", case.name));
        let series_collections_json = response_json(series_collections).await;
        let series_collections_ids = array_ids(&series_collections_json);
        assert_eq!(
            series_collections_ids,
            vec!["collection-1"],
            "{} collections membership",
            case.name,
        );
        assert_eq!(
            series_collections_json[0]["filtered"],
            case.expect_filtered_collection,
            "{} collection filtered flag",
            case.name,
        );
        assert_eq!(
            string_array(&series_collections_json[0]["seriesIds"]),
            if case.expect_filtered_collection {
                vec!["series-1"]
            } else {
                vec!["series-1", "series-2"]
            },
            "{} collection visible series ids",
            case.name,
        );

        let series_books = post_response(
            &app,
            &token,
            "/api/v1/books/list?page=0&size=20&sort=metadata.numberSort,asc",
            r#"{"condition":{"type":"AllOfBook","conditions":[{"type":"SeriesId","operator":"is","value":"series-1"}]}}"#,
            Some(NATIVE_OWNERSHIP_MARKER),
        )
        .await;
        assert_eq!(series_books.status(), StatusCode::OK, "{} direct browse books status", case.name);
        assert_eq!(
            series_books
                .headers()
                .get(SEARCH_OWNERSHIP_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some(NATIVE_OWNERSHIP_MARKER),
            "{} direct browse books marker propagation",
            case.name,
        );
        let series_books_json = response_json(series_books).await;
        let series_books_ids = page_content_ids(&series_books_json)
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert!(
            series_books_ids.iter().any(|id| id == "book-1"),
            "{} direct browse books should include owned target book",
            case.name,
        );
        assert!(
            series_books_ids.iter().all(|id| id != "book-2"),
            "{} direct browse books must not leak restricted-series book",
            case.name,
        );

        if let Some(expected_ids) = &admin_series_books_ids {
            assert_eq!(
                &series_books_ids,
                expected_ids,
                "{} direct browse books ids must match admin control",
                case.name,
            );
        } else {
            admin_series_books_ids = Some(series_books_ids.clone());
        }
        assert_eq!(
            series_books_json["content"]
                .as_array()
                .expect("direct browse books content should be an array")
                .len(),
            series_books_ids.len(),
            "{} direct browse books content length should stay consistent",
            case.name,
        );

        let book_detail = get_response(&app, &token, "/api/v1/books/book-1").await;
        assert_eq!(book_detail.status(), StatusCode::OK, "{} book detail status", case.name);
        assert_native_owned(&book_detail, &format!("{} book detail", case.name));
        let book_detail_json = response_json(book_detail).await;
        assert_eq!(book_detail_json["id"], "book-1", "{} book detail id", case.name);
        assert_eq!(
            book_detail_json["url"],
            case.expected_book_url,
            "{} book detail url",
            case.name,
        );
        assert_eq!(book_detail_json["sizeBytes"], 222, "{} book detail size bytes", case.name);
        assert_eq!(book_detail_json["size"], "222 B", "{} book detail size", case.name);
        assert_eq!(
            book_detail_json["media"]["mediaProfile"],
            "DIVINA",
            "{} book detail media profile",
            case.name,
        );

        let previous = get_response(&app, &token, "/api/v1/books/book-1/previous").await;
        assert_eq!(previous.status(), StatusCode::OK, "{} previous sibling status", case.name);
        assert_native_owned(&previous, &format!("{} previous sibling", case.name));
        let previous_json = response_json(previous).await;
        assert_eq!(previous_json["id"], "book-0", "{} previous sibling id", case.name);

        let next = get_response(&app, &token, "/api/v1/books/book-1/next").await;
        assert_eq!(next.status(), StatusCode::OK, "{} next sibling status", case.name);
        assert_native_owned(&next, &format!("{} next sibling", case.name));
        let next_json = response_json(next).await;
        assert_eq!(next_json["id"], "book-3", "{} next sibling id", case.name);

        let readlists = get_response(&app, &token, "/api/v1/books/book-1/readlists").await;
        assert_eq!(readlists.status(), StatusCode::OK, "{} readlists status", case.name);
        assert_native_owned(&readlists, &format!("{} readlists", case.name));
        let readlists_json = response_json(readlists).await;
        assert_eq!(
            array_ids(&readlists_json),
            vec!["readlist-1", "readlist-2"],
            "{} readlists ids",
            case.name,
        );
        assert_eq!(readlists_json[0]["filtered"], false, "{} first readlist visible", case.name);
        assert_eq!(
            readlists_json[1]["filtered"],
            case.expect_filtered_readlist,
            "{} mixed readlist filtered flag",
            case.name,
        );
        assert_eq!(
            string_array(&readlists_json[1]["bookIds"]),
            if case.expect_filtered_readlist {
                vec!["book-1"]
            } else {
                vec!["book-1", "book-2"]
            },
            "{} mixed readlist visible book ids",
            case.name,
        );
    }
}

#[tokio::test]
async fn direct_oneshot_admin_user_limited_restricted_matrix() {
    let app = komga_rust::app::build_router();
    let cases = [
        DirectOneshotPrincipalCase {
            name: "admin",
            basic_auth: ADMIN_BASIC_AUTH,
            expected_series_url: "/library/1/oneshot",
            expected_book_url: "/library1/oneshot-book.cbz",
        },
        DirectOneshotPrincipalCase {
            name: "user",
            basic_auth: USER_BASIC_AUTH,
            expected_series_url: "",
            expected_book_url: "oneshot-book.cbz",
        },
        DirectOneshotPrincipalCase {
            name: "limited",
            basic_auth: LIMITED_BASIC_AUTH,
            expected_series_url: "",
            expected_book_url: "oneshot-book.cbz",
        },
        DirectOneshotPrincipalCase {
            name: "restricted",
            basic_auth: RESTRICTED_BASIC_AUTH,
            expected_series_url: "",
            expected_book_url: "oneshot-book.cbz",
        },
    ];

    for case in cases {
        let token = session_token_for_basic_auth(&app, case.basic_auth).await;

        let series_detail = get_response(&app, &token, "/api/v1/series/series-oneshot").await;
        assert_eq!(series_detail.status(), StatusCode::OK, "{} oneshot series detail status", case.name);
        assert_native_owned(&series_detail, &format!("{} oneshot series detail", case.name));
        let series_detail_json = response_json(series_detail).await;
        assert_eq!(series_detail_json["id"], "series-oneshot", "{} oneshot series detail id", case.name);
        assert_eq!(
            series_detail_json["url"],
            case.expected_series_url,
            "{} oneshot series url parity",
            case.name,
        );
        assert_eq!(series_detail_json["oneshot"], true, "{} oneshot series flag", case.name);

        let collections = get_response(&app, &token, "/api/v1/series/series-oneshot/collections").await;
        assert_eq!(collections.status(), StatusCode::OK, "{} oneshot collections status", case.name);
        assert_native_owned(&collections, &format!("{} oneshot collections", case.name));
        let collections_json = response_json(collections).await;
        assert!(collections_json.is_array(), "{} oneshot collections payload type", case.name);
        assert!(
            collections_json.as_array().is_some_and(|items| items.is_empty()),
            "{} direct oneshot collections should stay empty",
            case.name,
        );

        let bootstrap = post_response(
            &app,
            &token,
            "/api/v1/books/list",
            r#"{"condition":{"type":"SeriesId","operator":"is","value":"series-oneshot"}}"#,
            None,
        )
        .await;
        assert_eq!(bootstrap.status(), StatusCode::OK, "{} oneshot bootstrap status", case.name);
        assert_eq!(
            bootstrap
                .headers()
                .get(SEARCH_OWNERSHIP_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some(NATIVE_OWNERSHIP_MARKER),
            "{} oneshot bootstrap should stay natively owned",
            case.name,
        );
        let bootstrap_json = response_json(bootstrap).await;
        assert_eq!(page_content_ids(&bootstrap_json), vec!["book-oneshot"], "{} oneshot bootstrap ids", case.name);
        assert!(bootstrap_json.get("_compat").is_none(), "{} oneshot bootstrap compat payload", case.name);

        let book_detail = get_response(&app, &token, "/api/v1/books/book-oneshot").await;
        assert_eq!(book_detail.status(), StatusCode::OK, "{} oneshot book detail status", case.name);
        assert_native_owned(&book_detail, &format!("{} oneshot book detail", case.name));
        let book_detail_json = response_json(book_detail).await;
        assert_eq!(book_detail_json["id"], "book-oneshot", "{} oneshot book detail id", case.name);
        assert_eq!(
            book_detail_json["url"],
            case.expected_book_url,
            "{} oneshot book detail url",
            case.name,
        );
        assert_eq!(book_detail_json["sizeBytes"], 150, "{} oneshot book size bytes", case.name);
        assert_eq!(book_detail_json["size"], "150 B", "{} oneshot book size", case.name);
        assert_eq!(
            book_detail_json["media"]["mediaProfile"],
            "",
            "{} oneshot book media profile",
            case.name,
        );

        let readlists = get_response(&app, &token, "/api/v1/books/book-oneshot/readlists").await;
        assert_eq!(readlists.status(), StatusCode::OK, "{} oneshot readlists status", case.name);
        assert_native_owned(&readlists, &format!("{} oneshot readlists", case.name));
        let readlists_json = response_json(readlists).await;
        assert!(readlists_json.is_array(), "{} oneshot readlists payload type", case.name);
        assert!(
            readlists_json.as_array().is_some_and(|items| items.is_empty()),
            "{} direct oneshot readlists should stay empty",
            case.name,
        );
    }
}

#[tokio::test]
async fn series_detail_and_collections_are_native_owned() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;

    let detail_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/series/series-1")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail_response.status(), StatusCode::OK);
    assert!(
        detail_response
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .is_none(),
        "native-owned series detail should not emit shadow marker",
    );

    let detail_body = axum::body::to_bytes(detail_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let detail_json: Value = serde_json::from_slice(&detail_body).unwrap();
    assert_eq!(detail_json["id"], "series-1");
    assert_eq!(detail_json["url"], "");
    assert!(detail_json.get("_compat").is_none());

    let collections_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/series/series-1/collections")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(collections_response.status(), StatusCode::OK);
    assert!(
        collections_response
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .is_none(),
        "native-owned series collections should not emit shadow marker",
    );

    let collections_body = axum::body::to_bytes(collections_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let collections_json: Value = serde_json::from_slice(&collections_body).unwrap();
    assert!(collections_json.is_array());
    assert_eq!(collections_json[0]["id"], "collection-1");
    assert!(collections_json.get("_compat").is_none());
}

async fn session_token_for_basic_auth<S>(app: &S, basic_auth: &str) -> String
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(header::AUTHORIZATION, format!("Basic {basic_auth}"))
                .header("X-Auth-Token", "")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .get("x-auth-token")
        .expect("login response should include x-auth-token")
        .to_str()
        .expect("x-auth-token should be valid UTF-8")
        .to_string()
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

async fn post_response<S>(
    app: &S,
    token: &str,
    uri: &str,
    body: &str,
    ownership_header: Option<&str>,
) -> axum::response::Response
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let mut request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("X-Auth-Token", token)
        .header(header::CONTENT_TYPE, "application/json");

    if let Some(marker) = ownership_header {
        request = request.header(SEARCH_OWNERSHIP_HEADER, marker);
    }

    app.clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn assert_native_owned(response: &axum::response::Response, branch: &str) {
    assert!(
        response.headers().get(SEARCH_OWNERSHIP_HEADER).is_none(),
        "native-owned detail branch should not emit shadow marker: {branch}",
    );
}

fn array_ids(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .expect("payload should be an array")
        .iter()
        .map(|it| {
            it.get("id")
                .and_then(Value::as_str)
                .expect("payload id should be a string")
        })
        .collect()
}

fn string_array(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .expect("payload field should be an array")
        .iter()
        .map(|it| it.as_str().expect("array entry should be a string"))
        .collect()
}

fn page_content_ids(value: &Value) -> Vec<&str> {
    value["content"]
        .as_array()
        .expect("page payload should expose array content")
        .iter()
        .map(|it| {
            it.get("id")
                .and_then(Value::as_str)
                .expect("page item id should be a string")
        })
        .collect()
}

#[tokio::test]
async fn page_scoped_books_list_is_native_owned() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, ADMIN_BASIC_AUTH).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20&sort=metadata.numberSort,asc")
                .header("X-Auth-Token", &token)
                .header(SEARCH_OWNERSHIP_HEADER, NATIVE_OWNERSHIP_MARKER)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"condition":{"type":"AllOfBook","conditions":[{"type":"SeriesId","operator":"is","value":"series-1"}]}}"#
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .and_then(|v| v.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["content"][0]["id"], "book-1");
}

#[tokio::test]
async fn oneshot_books_list_shape_is_native_owned() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list")
                .header("X-Auth-Token", &token)
                .header(SEARCH_OWNERSHIP_HEADER, NATIVE_OWNERSHIP_MARKER)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"condition":{"type":"SeriesId","operator":"is","value":"series-oneshot"}}"#.to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .and_then(|v| v.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(page_content_ids(&json), vec!["book-oneshot"]);
    assert!(json.get("_compat").is_none());
}

#[tokio::test]
async fn browse_oneshot_happy_path_uses_native_bootstrap_shape() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;

    let series_detail = get_response(&app, &token, "/api/v1/series/series-oneshot").await;
    assert_eq!(series_detail.status(), StatusCode::OK);
    assert_native_owned(&series_detail, "oneshot series detail");
    let series_detail_json = response_json(series_detail).await;
    assert_eq!(series_detail_json["id"], "series-oneshot");
    assert!(series_detail_json.get("_compat").is_none());

    let collections = get_response(&app, &token, "/api/v1/series/series-oneshot/collections").await;
    assert_eq!(collections.status(), StatusCode::OK);
    assert_native_owned(&collections, "oneshot series collections");
    let collections_json = response_json(collections).await;
    assert!(collections_json.is_array());
    assert!(collections_json.get("_compat").is_none());

    let bootstrap = post_response(
        &app,
        &token,
        "/api/v1/books/list",
        r#"{"condition":{"type":"SeriesId","operator":"is","value":"series-oneshot"}}"#,
        None,
    )
    .await;
    assert_eq!(bootstrap.status(), StatusCode::OK);
    assert_eq!(
        bootstrap
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
        "oneshot bootstrap books/list should return native marker without request marker hack",
    );
    let bootstrap_json = response_json(bootstrap).await;
    assert_eq!(page_content_ids(&bootstrap_json), vec!["book-oneshot"]);
    assert!(bootstrap_json.get("_compat").is_none());

    let readlists = get_response(&app, &token, "/api/v1/books/book-oneshot/readlists").await;
    assert_eq!(readlists.status(), StatusCode::OK);
    assert_native_owned(&readlists, "oneshot book readlists");
    let readlists_json = response_json(readlists).await;
    assert!(readlists_json.is_array());
    assert!(readlists_json.get("_compat").is_none());
}

#[tokio::test]
async fn phase3_phase4_owned_routes_do_not_regress_with_oneshot_bootstrap() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;

    let oneshot_bootstrap = post_response(
        &app,
        &token,
        "/api/v1/books/list",
        r#"{"condition":{"type":"SeriesId","operator":"is","value":"series-oneshot"}}"#,
        None,
    )
    .await;
    assert_eq!(oneshot_bootstrap.status(), StatusCode::OK);
    assert_eq!(
        oneshot_bootstrap
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
        "oneshot bootstrap should stay native without request marker hack",
    );

    let phase3_books = post_response(
        &app,
        &token,
        "/api/v1/books/list?page=0&size=20&sort=metadata.numberSort,asc",
        r#"{"condition":{"type":"AllOfBook","conditions":[{"type":"SeriesId","operator":"is","value":"series-1"}]}}"#,
        Some(NATIVE_OWNERSHIP_MARKER),
    )
    .await;
    assert_eq!(phase3_books.status(), StatusCode::OK);
    assert_eq!(
        phase3_books
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
        "phase3 page-scoped books/list marker propagation must stay unchanged",
    );
    let phase3_books_json = response_json(phase3_books).await;
    assert_eq!(phase3_books_json["content"][0]["id"], "book-1");
    assert!(phase3_books_json.get("_compat").is_none());

    let phase4_readlist_books = get_response(&app, &token, "/api/v1/readlists/readlist-2/books?unpaged=true").await;
    assert_eq!(phase4_readlist_books.status(), StatusCode::OK);
    assert_eq!(
        phase4_readlist_books
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
        "phase4 readlist unpaged route marker propagation must stay unchanged",
    );
    let phase4_readlist_books_json = response_json(phase4_readlist_books).await;
    assert_eq!(page_content_ids(&phase4_readlist_books_json), vec!["book-1", "book-2"]);
    assert!(phase4_readlist_books_json.get("_compat").is_none());
}

#[tokio::test]
async fn book_detail_is_native_owned() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers().get(SEARCH_OWNERSHIP_HEADER).is_none(),
        "native-owned book detail should not emit shadow marker",
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["id"], "book-1");
    assert_eq!(json["url"], "book.cbr");
    assert_eq!(json["sizeBytes"], 222);
    assert_eq!(json["size"], "222 B");
    assert_eq!(json["media"]["mediaProfile"], "DIVINA");
    assert!(json.get("_compat").is_none());
}

#[tokio::test]
async fn excluded_oneshot_query_parameter_emits_shadow_marker() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;

    let response = get_response(&app, &token, "/api/v1/series/series-oneshot?oneshot=true").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_shadow_marker(&response, "series detail oneshot query parameter");

    let json = response_json(response).await;
    assert_eq!(json["id"], "series-oneshot");
    assert_eq!(json["_compat"]["discoveryOwnership"], "non-native");
    assert_eq!(
        json["_compat"]["shape"],
        "UnsupportedSeriesFilter(oneshot-query-parameter)",
    );
}

#[tokio::test]
async fn excluded_oneshot_branches_emit_shadow_marker() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;
    let expected = BTreeSet::from([
        "oneshot bootstrap widening",
        "media delivery",
        "reader download adjacency",
        "progress routes",
        "SSE live-refresh",
    ]);
    let mut observed = BTreeSet::new();

    let widened_bootstrap = post_response(
        &app,
        &token,
        "/api/v1/books/list?page=0&size=20&sort=metadata.numberSort,asc",
        r#"{"condition":{"type":"AllOfBook","conditions":[{"type":"SeriesId","operator":"is","value":"series-oneshot"}]}}"#,
        Some("shadow-java-writer"),
    )
    .await;
    assert_eq!(widened_bootstrap.status(), StatusCode::OK);
    assert_shadow_marker(&widened_bootstrap, "oneshot bootstrap widening");
    observed.insert("oneshot bootstrap widening");

    let native_detail = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(native_detail.status(), StatusCode::OK);
    assert!(
        native_detail.headers().get(SEARCH_OWNERSHIP_HEADER).is_none(),
        "native detail payload must stay unmarked even when adjacent excluded branches are visible in UI",
    );
    let native_detail_body = axum::body::to_bytes(native_detail.into_body(), usize::MAX)
        .await
        .unwrap();
    let native_detail_json: Value = serde_json::from_slice(&native_detail_body).unwrap();
    assert_eq!(native_detail_json["url"], "book.cbr");

    let book_pages = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/pages")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(book_pages.status(), StatusCode::OK);
    assert_shadow_marker(&book_pages, "book pages inventory");
    observed.insert("media delivery");

    let page_asset = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/pages/1")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page_asset.status(), StatusCode::OK);
    assert_shadow_marker(&page_asset, "book page asset");

    let page_thumbnail = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/pages/1/thumbnail")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page_thumbnail.status(), StatusCode::OK);
    assert_shadow_marker(&page_thumbnail, "book page thumbnail");

    let book_thumbnail = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/thumbnail")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(book_thumbnail.status(), StatusCode::NOT_FOUND);
    assert_shadow_marker(&book_thumbnail, "book thumbnail");

    let download = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/file")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(download.status(), StatusCode::OK);
    assert_shadow_marker(&download, "book file download");
    assert!(download.headers().get(header::CONTENT_DISPOSITION).is_some());
    observed.insert("reader download adjacency");

    let read_progress_patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/read-progress")
                .header("X-Auth-Token", &token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{\"page\":10}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(read_progress_patch.status(), StatusCode::NO_CONTENT);
    assert_shadow_marker(&read_progress_patch, "read-progress patch");
    observed.insert("progress routes");

    let read_progress_delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/books/book-1/read-progress")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(read_progress_delete.status(), StatusCode::NO_CONTENT);
    assert_shadow_marker(&read_progress_delete, "read-progress delete");

    let progression = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/progression")
                .header("X-Auth-Token", &token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"modified":"2024-01-01T00:00:00Z","device":"compat-client","locator":{"href":"OEBPS/chapter-1.xhtml","type":"application/xhtml+xml","locations":{"progression":0.3}}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(progression.status(), StatusCode::NO_CONTENT);
    assert_shadow_marker(&progression, "progression patch");

    let live_refresh = app
        .oneshot(
            Request::builder()
                .uri("/sse/v1/events")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(live_refresh.status(), StatusCode::OK);
    assert_shadow_marker(&live_refresh, "SSE live-refresh");
    assert_eq!(
        live_refresh.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/event-stream",
    );
    observed.insert("SSE live-refresh");

    assert_eq!(expected, observed);
}

#[tokio::test]
async fn embedded_read_progress_is_preserved_without_owning_progress_routes() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;

    let before = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(before.status(), StatusCode::OK);
    let before_body = axum::body::to_bytes(before.into_body(), usize::MAX)
        .await
        .unwrap();
    let before_json: Value = serde_json::from_slice(&before_body).unwrap();
    assert_eq!(before_json["readProgress"]["page"], 7);
    assert_eq!(before_json["readProgress"]["completed"], false);
    assert_eq!(before_json["readProgress"]["deviceId"], "device-android");

    let patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/read-progress")
                .header("X-Auth-Token", &token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{\"page\":10}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::NO_CONTENT);

    let after = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(after.status(), StatusCode::OK);
    let after_body = axum::body::to_bytes(after.into_body(), usize::MAX)
        .await
        .unwrap();
    let after_json: Value = serde_json::from_slice(&after_body).unwrap();
    assert_eq!(after_json["readProgress"]["page"], 7);
    assert_eq!(after_json["readProgress"]["readDate"], "2024-01-04T03:04:05Z");
    assert_eq!(after_json["readProgress"]["deviceName"], "Android");
}

#[tokio::test]
async fn book_navigation_and_readlists_are_native_owned() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;

    let previous_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/previous")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(previous_response.status(), StatusCode::OK);
    assert!(
        previous_response
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .is_none(),
        "native-owned previous sibling should not emit shadow marker",
    );
    let previous_body = axum::body::to_bytes(previous_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let previous_json: Value = serde_json::from_slice(&previous_body).unwrap();
    assert_eq!(previous_json["id"], "book-0");
    assert!(previous_json.get("_compat").is_none());

    let next_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/next")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(next_response.status(), StatusCode::OK);
    assert!(
        next_response.headers().get(SEARCH_OWNERSHIP_HEADER).is_none(),
        "native-owned next sibling should not emit shadow marker",
    );
    let next_body = axum::body::to_bytes(next_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let next_json: Value = serde_json::from_slice(&next_body).unwrap();
    assert_eq!(next_json["id"], "book-3");
    assert!(next_json.get("_compat").is_none());

    let readlists_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/readlists")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(readlists_response.status(), StatusCode::OK);
    assert!(
        readlists_response
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .is_none(),
        "native-owned book readlists should not emit shadow marker",
    );

    let readlists_body = axum::body::to_bytes(readlists_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let readlists_json: Value = serde_json::from_slice(&readlists_body).unwrap();
    assert!(readlists_json.is_array());
    assert_eq!(readlists_json[0]["id"], "readlist-1");
    assert!(readlists_json.get("_compat").is_none());
}

fn assert_shadow_marker(response: &axum::response::Response, branch: &str) {
    let marker = response
        .headers()
        .get(SEARCH_OWNERSHIP_HEADER)
        .and_then(|value| value.to_str().ok());
    assert_eq!(
        marker,
        Some("shadow-java-writer"),
        "branch {branch} should emit explicit non-native marker, got {marker:?}",
    );
}

#[test]
fn readlist_books_paged_variant_remains_non_native() {
    let queries = DiscoveryQueries::new(SqliteDiscoveryAdapter::default());
    let context = DiscoveryQueryContext::allow_all();

    let result = queries.list_readlist_books(
        &context,
        ReadListBooksQuery {
            readlist_id: "readlist-1".to_string(),
            page: 0,
            size: 20,
            unpaged: false,
            library_ids: None,
        },
    );

    assert!(matches!(
        result,
        Err(DiscoveryError::NonNativeRequestShape(_))
    ));
}

#[test]
fn readlist_books_library_id_variant_remains_non_native() {
    let queries = DiscoveryQueries::new(SqliteDiscoveryAdapter::default());
    let context = DiscoveryQueryContext::allow_all();

    let result = queries.list_readlist_books(
        &context,
        ReadListBooksQuery {
            readlist_id: "readlist-1".to_string(),
            page: 0,
            size: 20,
            unpaged: true,
            library_ids: Some(vec!["1".to_string()]),
        },
    );

    assert!(matches!(
        result,
        Err(DiscoveryError::NonNativeRequestShape(_))
    ));
}

#[tokio::test]
async fn readlist_books_runtime_ownership_stays_narrow() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;

    let previous_response = get_response(&app, &token, "/api/v1/readlists/readlist-2/books/book-1/previous").await;
    assert_eq!(previous_response.status(), StatusCode::NOT_FOUND);
    assert_native_owned(&previous_response, "readlist previous boundary");

    let next_response = get_response(&app, &token, "/api/v1/readlists/readlist-2/books/book-1/next").await;
    assert_eq!(next_response.status(), StatusCode::OK);
    assert_native_owned(&next_response, "readlist next sibling");
    let next_json = response_json(next_response).await;
    assert_eq!(next_json["id"], "book-2");
    assert!(next_json.get("_compat").is_none());

    let native_response = get_response(&app, &token, "/api/v1/readlists/readlist-2/books?unpaged=true").await;
    assert_eq!(native_response.status(), StatusCode::OK);
    assert_eq!(
        native_response
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
        "bare unpaged readlist books route should be native-owned",
    );
    let native_json = response_json(native_response).await;
    assert_eq!(page_content_ids(&native_json), vec!["book-1", "book-2"]);
    assert_eq!(native_json["pageable"]["paged"], false);
    assert_eq!(native_json["pageable"]["unpaged"], true);
    assert!(native_json.get("_compat").is_none());

    let paged_response = get_response(&app, &token, "/api/v1/readlists/readlist-2/books?page=0&size=20").await;
    assert_eq!(paged_response.status(), StatusCode::OK);
    assert_shadow_marker(&paged_response, "paged readlist books");
    let paged_json = response_json(paged_response).await;
    assert_eq!(paged_json["_compat"]["discoveryOwnership"], "non-native");
    assert_eq!(paged_json["_compat"]["shape"], "UnsupportedBookFilter(paged)");

    let library_scoped_response =
        get_response(&app, &token, "/api/v1/readlists/readlist-2/books?unpaged=true&library_id=1").await;
    assert_eq!(library_scoped_response.status(), StatusCode::OK);
    assert_shadow_marker(&library_scoped_response, "library-scoped readlist books");
    let library_scoped_json = response_json(library_scoped_response).await;
    assert_eq!(library_scoped_json["_compat"]["discoveryOwnership"], "non-native");
    assert_eq!(
        library_scoped_json["_compat"]["shape"],
        "UnsupportedBookFilter(LibraryId)",
    );
}
