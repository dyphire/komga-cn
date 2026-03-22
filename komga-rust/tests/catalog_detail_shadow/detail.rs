use super::*;

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
        assert_eq!(
            series_detail.status(),
            StatusCode::OK,
            "{} series detail status",
            case.name
        );
        assert_native_owned(&series_detail, &format!("{} series detail", case.name));
        let series_detail_json = response_json(series_detail).await;
        assert_eq!(
            series_detail_json["id"], "series-1",
            "{} series detail id",
            case.name
        );
        assert_eq!(
            series_detail_json["url"], case.expected_series_url,
            "{} series url parity",
            case.name,
        );

        let series_collections =
            get_response(&app, &token, "/api/v1/series/series-1/collections").await;
        assert_eq!(
            series_collections.status(),
            StatusCode::OK,
            "{} series collections status",
            case.name,
        );
        assert_native_owned(
            &series_collections,
            &format!("{} series collections", case.name),
        );
        let series_collections_json = response_json(series_collections).await;
        let series_collections_ids = array_ids(&series_collections_json);
        assert_eq!(
            series_collections_ids,
            vec!["collection-1"],
            "{} collections membership",
            case.name,
        );
        assert_eq!(
            series_collections_json[0]["filtered"], case.expect_filtered_collection,
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
        assert_eq!(
            series_books.status(),
            StatusCode::OK,
            "{} direct browse books status",
            case.name
        );
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
                &series_books_ids, expected_ids,
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
        assert_eq!(
            book_detail.status(),
            StatusCode::OK,
            "{} book detail status",
            case.name
        );
        assert_native_owned(&book_detail, &format!("{} book detail", case.name));
        let book_detail_json = response_json(book_detail).await;
        assert_eq!(
            book_detail_json["id"], "book-1",
            "{} book detail id",
            case.name
        );
        assert_eq!(
            book_detail_json["url"], case.expected_book_url,
            "{} book detail url",
            case.name,
        );
        assert_eq!(
            book_detail_json["sizeBytes"], 222,
            "{} book detail size bytes",
            case.name
        );
        assert_eq!(
            book_detail_json["size"], "222 B",
            "{} book detail size",
            case.name
        );
        assert_eq!(
            book_detail_json["media"]["mediaProfile"], "DIVINA",
            "{} book detail media profile",
            case.name,
        );

        let previous = get_response(&app, &token, "/api/v1/books/book-1/previous").await;
        assert_eq!(
            previous.status(),
            StatusCode::OK,
            "{} previous sibling status",
            case.name
        );
        assert_native_owned(&previous, &format!("{} previous sibling", case.name));
        let previous_json = response_json(previous).await;
        assert_eq!(
            previous_json["id"], "book-0",
            "{} previous sibling id",
            case.name
        );

        let next = get_response(&app, &token, "/api/v1/books/book-1/next").await;
        assert_eq!(
            next.status(),
            StatusCode::OK,
            "{} next sibling status",
            case.name
        );
        assert_native_owned(&next, &format!("{} next sibling", case.name));
        let next_json = response_json(next).await;
        assert_eq!(next_json["id"], "book-3", "{} next sibling id", case.name);

        let readlists = get_response(&app, &token, "/api/v1/books/book-1/readlists").await;
        assert_eq!(
            readlists.status(),
            StatusCode::OK,
            "{} readlists status",
            case.name
        );
        assert_native_owned(&readlists, &format!("{} readlists", case.name));
        let readlists_json = response_json(readlists).await;
        assert_eq!(
            array_ids(&readlists_json),
            vec!["readlist-1", "readlist-2"],
            "{} readlists ids",
            case.name,
        );
        assert_eq!(
            readlists_json[0]["filtered"], false,
            "{} first readlist visible",
            case.name
        );
        assert_eq!(
            readlists_json[1]["filtered"], case.expect_filtered_readlist,
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
        assert_eq!(
            series_detail.status(),
            StatusCode::OK,
            "{} oneshot series detail status",
            case.name
        );
        assert_native_owned(
            &series_detail,
            &format!("{} oneshot series detail", case.name),
        );
        let series_detail_json = response_json(series_detail).await;
        assert_eq!(
            series_detail_json["id"], "series-oneshot",
            "{} oneshot series detail id",
            case.name
        );
        assert_eq!(
            series_detail_json["url"], case.expected_series_url,
            "{} oneshot series url parity",
            case.name,
        );
        assert_eq!(
            series_detail_json["oneshot"], true,
            "{} oneshot series flag",
            case.name
        );

        let collections =
            get_response(&app, &token, "/api/v1/series/series-oneshot/collections").await;
        assert_eq!(
            collections.status(),
            StatusCode::OK,
            "{} oneshot collections status",
            case.name
        );
        assert_native_owned(&collections, &format!("{} oneshot collections", case.name));
        let collections_json = response_json(collections).await;
        assert!(
            collections_json.is_array(),
            "{} oneshot collections payload type",
            case.name
        );
        assert!(
            collections_json
                .as_array()
                .is_some_and(|items| items.is_empty()),
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
        assert_eq!(
            bootstrap.status(),
            StatusCode::OK,
            "{} oneshot bootstrap status",
            case.name
        );
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
        assert_eq!(
            page_content_ids(&bootstrap_json),
            vec!["book-oneshot"],
            "{} oneshot bootstrap ids",
            case.name
        );
        assert!(
            bootstrap_json.get("_compat").is_none(),
            "{} oneshot bootstrap compat payload",
            case.name
        );

        let book_detail = get_response(&app, &token, "/api/v1/books/book-oneshot").await;
        assert_eq!(
            book_detail.status(),
            StatusCode::OK,
            "{} oneshot book detail status",
            case.name
        );
        assert_native_owned(&book_detail, &format!("{} oneshot book detail", case.name));
        let book_detail_json = response_json(book_detail).await;
        assert_eq!(
            book_detail_json["id"], "book-oneshot",
            "{} oneshot book detail id",
            case.name
        );
        assert_eq!(
            book_detail_json["url"], case.expected_book_url,
            "{} oneshot book detail url",
            case.name,
        );
        assert_eq!(
            book_detail_json["sizeBytes"], 150,
            "{} oneshot book size bytes",
            case.name
        );
        assert_eq!(
            book_detail_json["size"], "150 B",
            "{} oneshot book size",
            case.name
        );
        assert_eq!(
            book_detail_json["media"]["mediaProfile"], "",
            "{} oneshot book media profile",
            case.name,
        );

        let readlists = get_response(&app, &token, "/api/v1/books/book-oneshot/readlists").await;
        assert_eq!(
            readlists.status(),
            StatusCode::OK,
            "{} oneshot readlists status",
            case.name
        );
        assert_native_owned(&readlists, &format!("{} oneshot readlists", case.name));
        let readlists_json = response_json(readlists).await;
        assert!(
            readlists_json.is_array(),
            "{} oneshot readlists payload type",
            case.name
        );
        assert!(
            readlists_json
                .as_array()
                .is_some_and(|items| items.is_empty()),
            "{} direct oneshot readlists should stay empty",
            case.name,
        );
    }
}

pub(super) async fn series_detail_and_collections_are_native_owned() {
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

pub(super) async fn phase7_missing_and_restricted_series_oneshot_detail_matches_plain_detail_semantics() {
    let app = komga_rust::app::build_router();
    let cases = [
        ("missing series", USER_BASIC_AUTH, "series-missing"),
        (
            "restricted series for restricted user",
            RESTRICTED_BASIC_AUTH,
            "series-2",
        ),
    ];

    for (name, basic_auth, series_id) in cases {
        let token = session_token_for_basic_auth(&app, basic_auth).await;
        let plain_uri = format!("/api/v1/series/{series_id}");
        let oneshot_uri = format!("{plain_uri}?oneshot=true");

        let plain_response = get_response(&app, &token, &plain_uri).await;
        let plain_status = plain_response.status();
        assert_native_owned(&plain_response, &format!("{name} plain detail"));

        let oneshot_response = get_response(&app, &token, &oneshot_uri).await;
        let oneshot_status = oneshot_response.status();
        assert_native_owned(&oneshot_response, &format!("{name} oneshot detail"));

        assert_eq!(
            oneshot_status, plain_status,
            "{name} exact oneshot route must preserve plain detail status semantics",
        );
        assert!(
            matches!(plain_status, StatusCode::NOT_FOUND | StatusCode::FORBIDDEN),
            "{name} should be an inaccessible detail status, got {plain_status}",
        );
    }
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
        next_response
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .is_none(),
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
