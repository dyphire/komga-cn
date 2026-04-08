use super::*;

#[path = "catalog/latest_series.rs"]
mod latest_series;

#[path = "catalog/browse_collections_readlists.rs"]
mod browse_collections_readlists;

#[path = "catalog/latest_books.rs"]
mod latest_books;

#[path = "catalog/on_deck.rs"]
mod on_deck;

#[path = "catalog/recommended.rs"]
mod recommended;

#[path = "catalog/keep_reading.rs"]
mod keep_reading;

#[path = "catalog/readlist_detail.rs"]
mod readlist_detail;

#[path = "catalog/book_route_auth.rs"]
mod book_route_auth;

#[tokio::test]
async fn router_opds_v1_catalog_route_returns_atom_feed() {
    let paths = new_router_fixture("router-opds-v1-catalog-route").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/catalog")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 catalog request should build"),
        )
        .await
        .expect("opds v1 catalog request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(content_type.contains("application/atom+xml"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_catalog_includes_search_and_opds_v2_alternate_links() {
    let paths = new_router_fixture("router-opds-v1-catalog-links").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/catalog")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 catalog links request should build"),
        )
        .await
        .expect("opds v1 catalog links request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("rel=\"search\"") && body.contains("/opds/v1.2/search"),
        "OPDS v1 catalog must include search link, body={body}"
    );
    assert!(
        body.contains("rel=\"alternate\"")
            && body.contains("type=\"application/opds+json\"")
            && body.contains("/opds/v2/catalog"),
        "OPDS v1 catalog must include OPDS v2 alternate link, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_libraries_unauthorized_includes_basic_challenge() {
    let paths = new_router_fixture("router-opds-v1-libraries-basic-challenge").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/libraries")
                .body(Body::empty())
                .expect("opds v1 libraries unauthorized request should build"),
        )
        .await
        .expect("opds v1 libraries unauthorized request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"Realm\"")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_libraries_preserves_kotlin_dao_iteration_order() {
    let paths = new_router_fixture("router-opds-v1-libraries-dao-order").await;
    seed_router_contract_data(&paths).await;
    update_router_library_name(&paths, "library-1", "Z Library").await;
    seed_router_library(&paths, "library-2", "A Library").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/libraries")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 libraries request should build"),
        )
        .await
        .expect("opds v1 libraries request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    let library_1_pos = body
        .find("/opds/v1.2/libraries/library-1")
        .expect("library-1 entry should be present");
    let library_2_pos = body
        .find("/opds/v1.2/libraries/library-2")
        .expect("library-2 entry should be present");
    assert!(
        library_1_pos < library_2_pos,
        "OPDS v1 libraries should keep Kotlin DAO iteration order instead of name-sorting, body={body}"
    );

    cleanup_router_fixture(paths);
}

async fn clear_router_collections_and_readlists(paths: &RuntimeDbPaths) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds collection/readlist cleanup db should open");

    for sql in [
        "DELETE FROM COLLECTION_SERIES",
        "DELETE FROM COLLECTION",
        "DELETE FROM READLIST_BOOK",
        "DELETE FROM READLIST",
    ] {
        sqlx::query(sql)
            .execute(&pool)
            .await
            .expect("collections/readlists should be deleted");
    }

    pool.close().await;
}

async fn seed_router_collection_series_entry(
    paths: &RuntimeDbPaths,
    collection_id: &str,
    series_id: &str,
    number: i64,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds collection-series seed db should open");

    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind(collection_id)
    .bind(series_id)
    .bind(number)
    .execute(&pool)
    .await
    .expect("collection series entry should be inserted");

    pool.close().await;
}

async fn seed_router_readlist(
    paths: &RuntimeDbPaths,
    readlist_id: &str,
    name: &str,
    book_id: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds readlist seed db should open");

    sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT) VALUES (?, ?, ?)")
        .bind(readlist_id)
        .bind(name)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("readlist row should be inserted");

    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind(readlist_id)
        .bind(book_id)
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("readlist book row should be inserted");

    pool.close().await;
}

async fn seed_router_readlist_book_entry(
    paths: &RuntimeDbPaths,
    readlist_id: &str,
    book_id: &str,
    number: i64,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds readlist-book seed db should open");

    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind(readlist_id)
        .bind(book_id)
        .bind(number)
        .execute(&pool)
        .await
        .expect("readlist book entry should be inserted");

    pool.close().await;
}

#[tokio::test]
async fn router_opds_v2_catalog_uses_kotlin_top_level_links_when_authenticated() {
    let paths = new_router_fixture("router-opds-v2-catalog-self-link").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/catalog")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 catalog self link request should build"),
        )
        .await
        .expect("opds v2 catalog self link request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let links = payload
        .get("links")
        .and_then(Value::as_array)
        .expect("catalog links should be present");

    let self_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("self"))
        .expect("catalog self link should be present");
    assert_eq!(
        self_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries")
    );
    assert!(
        self_link.get("type").is_none(),
        "catalog self link should omit type like Kotlin, link={self_link}"
    );

    let start_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("start"))
        .expect("catalog start link should be present");
    assert_eq!(
        start_link.get("title").and_then(Value::as_str),
        Some("Home")
    );
    assert_eq!(
        start_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/catalog")
    );
    assert_eq!(
        start_link.get("type").and_then(Value::as_str),
        Some("application/opds+json")
    );

    let search_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("search"))
        .expect("catalog search link should be present");
    assert_eq!(
        search_link.get("title").and_then(Value::as_str),
        Some("Search")
    );
    assert_eq!(
        search_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/search{?query}")
    );
    assert_eq!(
        search_link.get("type").and_then(Value::as_str),
        Some("application/opds+json")
    );
    assert_eq!(
        search_link.get("templated").and_then(Value::as_bool),
        Some(true)
    );

    let recommended_href = payload
        .get("navigation")
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries.iter().find_map(|entry| {
                (entry.get("title").and_then(Value::as_str) == Some("Recommended"))
                    .then(|| entry.get("href").and_then(Value::as_str))
                    .flatten()
            })
        });
    assert_eq!(recommended_href, Some("http://localhost/opds/v2/libraries"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_libraries_uses_kotlin_top_level_links_when_authenticated() {
    let paths = new_router_fixture("router-opds-v2-libraries-top-level-links").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 libraries request should build"),
        )
        .await
        .expect("opds v2 libraries request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str),
        Some("All libraries - Recommended")
    );

    let links = payload
        .get("links")
        .and_then(Value::as_array)
        .expect("libraries links should be present");

    let self_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("self"))
        .expect("libraries self link should be present");
    assert_eq!(
        self_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries")
    );
    assert!(
        self_link.get("type").is_none(),
        "libraries self link should omit type like Kotlin, link={self_link}"
    );

    let start_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("start"))
        .expect("libraries start link should be present");
    assert_eq!(
        start_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/catalog")
    );
    assert_eq!(
        start_link.get("title").and_then(Value::as_str),
        Some("Home")
    );

    let recommended_link = payload
        .get("navigation")
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.get("title").and_then(Value::as_str) == Some("Recommended"))
        })
        .expect("libraries recommended navigation should be present");
    assert_eq!(
        recommended_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries")
    );

    let browse_link = payload
        .get("navigation")
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.get("title").and_then(Value::as_str) == Some("Browse"))
        })
        .expect("libraries browse navigation should be present");
    assert_eq!(
        browse_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/libraries/browse")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_catalog_latest_books_publication_uses_webpub_like_shape() {
    let paths = new_router_fixture("router-opds-v2-catalog-publication-shape").await;
    seed_router_contract_data(&paths).await;
    update_router_book_isbn(&paths, "book-1", "9781234567890").await;
    update_router_book_number_metadata(&paths, "book-1", "Special", 10.0).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/catalog")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 catalog publication shape request should build"),
        )
        .await
        .expect("opds v2 catalog publication shape request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let latest_books_group = payload
        .get("groups")
        .and_then(Value::as_array)
        .and_then(|groups| {
            groups.iter().find(|group| {
                group
                    .get("metadata")
                    .and_then(|metadata| metadata.get("title"))
                    .and_then(Value::as_str)
                    == Some("Latest Books")
            })
        })
        .expect("latest books group should be present");
    let publication = latest_books_group
        .get("publications")
        .and_then(Value::as_array)
        .and_then(|publications| publications.first())
        .expect("latest books group should include a publication");

    assert_eq!(
        publication.get("@context").and_then(Value::as_str),
        Some("https://readium.org/webpub-manifest/context.jsonld")
    );

    let links = publication
        .get("links")
        .and_then(Value::as_array)
        .expect("publication links should be present");
    let self_link = links
        .iter()
        .find(|link| link.get("rel").and_then(Value::as_str) == Some("self"))
        .expect("publication self link should be present");
    assert_eq!(
        self_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/books/book-1/manifest")
    );
    assert_eq!(
        self_link.get("type").and_then(Value::as_str),
        Some("application/webpub+json")
    );
    assert_eq!(
        self_link
            .get("properties")
            .and_then(|properties| properties.get("authenticate"))
            .and_then(|authenticate| authenticate.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/opds/v2/auth")
    );

    let metadata = publication
        .get("metadata")
        .expect("publication metadata should be present");
    assert_eq!(
        metadata.get("identifier").and_then(Value::as_str),
        Some("urn:isbn:9781234567890")
    );
    assert_eq!(
        metadata.get("numberOfPages").and_then(Value::as_u64),
        Some(10)
    );
    assert_eq!(
        metadata.get("published").and_then(Value::as_str),
        Some("2024-01-15")
    );
    assert_eq!(
        metadata
            .get("subject")
            .and_then(Value::as_array)
            .and_then(|subjects| subjects.first())
            .and_then(Value::as_str),
        Some("favorite-tag")
    );
    assert_eq!(
        metadata
            .get("contributor")
            .and_then(Value::as_array)
            .and_then(|contributors| contributors.first())
            .and_then(Value::as_str),
        Some("Jane Writer")
    );
    assert_eq!(
        metadata
            .get("belongsTo")
            .and_then(|belongs_to| belongs_to.get("series"))
            .and_then(Value::as_array)
            .and_then(|series| series.first())
            .and_then(|series| series.get("name"))
            .and_then(Value::as_str),
        Some("Series 1")
    );
    assert_eq!(
        metadata
            .get("belongsTo")
            .and_then(|belongs_to| belongs_to.get("series"))
            .and_then(Value::as_array)
            .and_then(|series| series.first())
            .and_then(|series| series.get("position"))
            .and_then(Value::as_f64),
        Some(10.0)
    );
    assert_eq!(
        metadata
            .get("belongsTo")
            .and_then(|belongs_to| belongs_to.get("series"))
            .and_then(Value::as_array)
            .and_then(|series| series.first())
            .and_then(|series| series.get("links"))
            .and_then(Value::as_array)
            .and_then(|links| links.first())
            .and_then(|link| link.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/opds/v2/series/series-1")
    );

    let progression_link = links
        .iter()
        .find(|link| {
            link.get("rel").and_then(Value::as_str)
                == Some("http://www.cantook.com/api/progression")
        })
        .expect("publication progression link should be present");
    assert_eq!(
        progression_link.get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/books/book-1/progression")
    );
    assert_eq!(
        progression_link.get("type").and_then(Value::as_str),
        Some("application/vnd.readium.progression+json")
    );

    let images = publication
        .get("images")
        .and_then(Value::as_array)
        .expect("publication images should be present");
    assert_eq!(images.len(), 1);
    assert_eq!(
        images[0].get("href").and_then(Value::as_str),
        Some("http://localhost/opds/v2/books/book-1/thumbnail")
    );
    assert_eq!(
        images[0].get("type").and_then(Value::as_str),
        Some("image/jpeg")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_catalog_latest_books_filters_before_limit_for_library_restricted_user() {
    let paths = new_router_fixture("router-opds-v2-catalog-latest-books-prefiltered").await;
    seed_router_contract_data(&paths).await;
    seed_router_library(&paths, "library-2", "Library 2").await;
    seed_router_custom_series(&paths, "series-2", "Hidden Series", "library-2").await;
    update_router_book_created_date(&paths, "book-1", "2024-01-01 00:00:00").await;

    for (index, created_date) in [
        "2024-02-05 00:00:00",
        "2024-02-04 00:00:00",
        "2024-02-03 00:00:00",
        "2024-02-02 00:00:00",
        "2024-02-01 00:00:00",
    ]
    .into_iter()
    .enumerate()
    {
        let book_id = format!("hidden-book-{}", index + 1);
        seed_catalog_book(
            &paths,
            &book_id,
            "series-2",
            "library-2",
            &format!("Hidden Book {}", index + 1),
            (index + 2) as i64,
            created_date,
        )
        .await;
    }

    seed_router_library_restricted_user(
        &paths,
        "library-user",
        "library-user@example.org",
        "library-user-pass-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library-user@example.org",
        "library-user-pass-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/catalog")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 catalog library restricted request should build"),
        )
        .await
        .expect("opds v2 catalog library restricted request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let latest_books_group = payload
        .get("groups")
        .and_then(Value::as_array)
        .and_then(|groups| {
            groups.iter().find(|group| {
                group
                    .get("metadata")
                    .and_then(|metadata| metadata.get("title"))
                    .and_then(Value::as_str)
                    == Some("Latest Books")
            })
        })
        .expect("latest books group should be present for restricted user");

    assert_eq!(
        latest_books_group
            .get("metadata")
            .and_then(|metadata| metadata.get("numberOfItems"))
            .and_then(Value::as_u64),
        Some(1)
    );
    let publications = latest_books_group
        .get("publications")
        .and_then(Value::as_array)
        .expect("latest books publications should be present");
    assert_eq!(publications.len(), 1);
    assert_eq!(
        publications[0]
            .get("metadata")
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str),
        Some("Book 1")
    );

    cleanup_router_fixture(paths);
}

async fn seed_router_read_progress_entry(
    paths: &RuntimeDbPaths,
    book_id: &str,
    user_id: &str,
    page: i64,
    completed: bool,
    read_date: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds read-progress seed db should open");

    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, READ_DATE) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(user_id)
    .bind(page)
    .bind(completed)
    .bind(read_date)
    .execute(&pool)
    .await
    .expect("read progress row should be inserted");

    pool.close().await;
}

async fn assert_unauthorized_opds_auth_document(response: axum::response::Response) {
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"Realm\"")
    );
    assert!(
        response
            .headers()
            .get(header::LINK)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value.contains("/opds/v2/auth")
                    && value.contains("http://opds-spec.org/auth/document")
                    && value.contains("application/opds-authentication+json")
            })
    );

    let payload = response_json(response).await;
    assert!(
        payload
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|value| value.contains("/opds/v2/auth"))
    );
}

async fn upsert_router_series_read_progress(
    paths: &RuntimeDbPaths,
    series_id: &str,
    user_id: &str,
    read_count: i64,
    in_progress_count: i64,
    most_recent_read_date: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds series read-progress seed db should open");

    sqlx::query(
        "INSERT INTO READ_PROGRESS_SERIES (SERIES_ID, USER_ID, READ_COUNT, IN_PROGRESS_COUNT, MOST_RECENT_READ_DATE) VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT (SERIES_ID, USER_ID) DO UPDATE SET READ_COUNT = excluded.READ_COUNT, IN_PROGRESS_COUNT = excluded.IN_PROGRESS_COUNT, MOST_RECENT_READ_DATE = excluded.MOST_RECENT_READ_DATE",
    )
    .bind(series_id)
    .bind(user_id)
    .bind(read_count)
    .bind(in_progress_count)
    .bind(most_recent_read_date)
    .execute(&pool)
    .await
    .expect("series read progress row should be upserted");

    pool.close().await;
}

async fn update_router_book_isbn(paths: &RuntimeDbPaths, book_id: &str, isbn: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds book isbn update db should open");

    sqlx::query("UPDATE BOOK_METADATA SET ISBN = ? WHERE BOOK_ID = ?")
        .bind(isbn)
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("book metadata isbn should be updated");

    pool.close().await;
}

async fn update_router_book_number_metadata(
    paths: &RuntimeDbPaths,
    book_id: &str,
    number: &str,
    number_sort: f64,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds book number metadata update db should open");

    sqlx::query("UPDATE BOOK_METADATA SET NUMBER = ?, NUMBER_SORT = ? WHERE BOOK_ID = ?")
        .bind(number)
        .bind(number_sort)
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("book metadata number fields should be updated");

    pool.close().await;
}

async fn update_router_book_created_date(
    paths: &RuntimeDbPaths,
    book_id: &str,
    created_date: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds book created_date update db should open");

    sqlx::query("UPDATE BOOK SET CREATED_DATE = ?, LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind(created_date)
        .bind(created_date)
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("book created date should be updated");

    pool.close().await;
}

async fn update_router_series_catalog_fields(
    paths: &RuntimeDbPaths,
    series_id: &str,
    one_shot: bool,
    last_modified_date: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds series catalog update db should open");

    sqlx::query(
        "UPDATE SERIES SET ONESHOT = ?, LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?",
    )
    .bind(one_shot)
    .bind(last_modified_date)
    .bind(last_modified_date)
    .bind(series_id)
    .execute(&pool)
    .await
    .expect("series catalog fields should be updated");

    pool.close().await;
}

async fn seed_catalog_book(
    paths: &RuntimeDbPaths,
    book_id: &str,
    series_id: &str,
    library_id: &str,
    title: &str,
    number: i64,
    created_date: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds catalog book seed db should open");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(0_i64)
    .bind(format!("{book_id}.epub"))
    .bind(format!("books/{book_id}.epub"))
    .bind(series_id)
    .bind(2_048_i64)
    .bind(number)
    .bind(library_id)
    .execute(&pool)
    .await
    .expect("catalog book row should be inserted");

    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind(book_id)
        .bind(10_i64)
        .execute(&pool)
        .await
        .expect("catalog book media should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(number.to_string())
    .bind(number as f64)
    .bind(title)
    .bind("2024-01-15")
    .bind(book_id)
    .execute(&pool)
    .await
    .expect("catalog book metadata should be inserted");

    sqlx::query("UPDATE BOOK SET CREATED_DATE = ?, LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind(created_date)
        .bind(created_date)
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("catalog book created date should be updated");

    pool.close().await;
}
