use super::*;

#[tokio::test]
async fn router_removed_authors_v1_route_returns_not_found() {
    let paths = new_router_fixture("router-removed-authors-v1-route").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/authors?search=jane")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("removed authors v1 route request should build"),
        )
        .await
        .expect("removed authors v1 route request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_author_endpoints_filter_to_authorized_libraries() {
    let paths = new_router_fixture("router-authors-authorized-libraries").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-1-user",
        "library1@example.org",
        "router-contract-library1-123",
        &["library-1"],
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("authors authorized-libraries db should open");
    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
        .bind("book-3")
        .bind("Morgan Else")
        .bind("inker")
        .execute(&pool)
        .await
        .expect("cross-library inker role row should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library1@example.org",
        "router-contract-library1-123",
    )
    .await;

    for (route, expected) in [
        ("/api/v1/authors/names", json!(["Alex Side", "Jane Writer"])),
        ("/api/v1/authors/roles", json!(["writer"])),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("authors authorized-libraries request should build"),
            )
            .await
            .expect("authors authorized-libraries request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let payload = response_json(response).await;
        assert_eq!(payload, expected, "route: {route}");
    }

    let v2_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/authors?unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("authors v2 authorized-libraries request should build"),
        )
        .await
        .expect("authors v2 authorized-libraries request should complete");

    assert_eq!(v2_response.status(), StatusCode::OK);
    let payload = response_json(v2_response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("authors v2 payload should expose content array");
    let author_names = content
        .iter()
        .filter_map(|entry| entry.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();

    assert_eq!(author_names, vec!["Alex Side", "Jane Writer"]);
    assert_eq!(payload["totalElements"], json!(2));
    assert_eq!(payload["size"], json!(20));
    assert_eq!(payload["sort"]["empty"], json!(false));
    assert_eq!(payload["sort"]["sorted"], json!(true));
    assert_eq!(payload["sort"]["unsorted"], json!(false));
    assert_eq!(payload["pageable"]["pageSize"], json!(20));
    assert_eq!(payload["pageable"]["paged"], json!(true));
    assert_eq!(payload["pageable"]["unpaged"], json!(false));
    assert_eq!(payload["pageable"]["sort"]["empty"], json!(false));
    assert_eq!(payload["pageable"]["sort"]["sorted"], json!(true));
    assert_eq!(payload["pageable"]["sort"]["unsorted"], json!(false));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_author_endpoints_accept_basic_auth_like_kotlin_clients() {
    let paths = new_router_fixture("router-authors-basic-auth-compat").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let authorization =
        basic_authorization_header_value("admin@example.org", "router-contract-admin-123");

    for route in [
        "/api/v1/authors/names",
        "/api/v1/authors/roles",
        "/api/v2/authors?unpaged=true",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header(header::AUTHORIZATION, authorization.as_str())
                    .header("x-auth-token", "")
                    .body(Body::empty())
                    .expect("authors basic-auth request should build"),
            )
            .await
            .expect("authors basic-auth request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_authors_names_matches_search_without_accents() {
    let paths = new_router_fixture("router-authors-names-strip-accents").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("authors names strip-accents db should open");
    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
        .bind("book-1")
        .bind("José Álvarez")
        .bind("writer")
        .execute(&pool)
        .await
        .expect("accented author row should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/authors/names?search=jose")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("authors names strip-accents request should build"),
        )
        .await
        .expect("authors names strip-accents request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let names = payload
        .as_array()
        .expect("authors names strip-accents payload should be an array");
    assert_eq!(names, &vec![json!("José Álvarez")]);

    cleanup_router_fixture(paths);
}
