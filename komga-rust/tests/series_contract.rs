use std::fs;
use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use komga_compat_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::config::{CompatProfile, RuntimeConfig};
use komga_rust::persistence::sqlite::connect_pool;
use serde_json::{Value, json};
use sqlx::Row;
use tower::ServiceExt;

#[path = "compat/auth_env.rs"]
mod compat_auth_env;

#[path = "support/persistence_contract_fixture.rs"]
mod persistence_contract_fixture;

#[test]
fn series_contract_target_is_registered() {
    assert_required_target_declared("series", "series_contract");
}

#[tokio::test]
async fn series_listing_reflects_persisted_rows_and_collection_filters() {
    let fixture = SeriesContractFixture::new("series-list-persisted").await;
    let library_root = create_library_root(&fixture.paths.config_dir, "series-list-library");

    seed_library(&fixture.paths.main_db, "library-main", "Series Library", &library_root).await;
    seed_series(
        &fixture.paths.main_db,
        SeedSeriesRow {
            id: "series-alpha",
            library_id: "library-main",
            name: "Alpha Shelf",
            title: "Alpha Archive",
            title_sort: "Alpha Archive",
            summary: "alpha summary",
            created_date: "2024-01-01T00:00:00",
            last_modified_date: "2024-01-03T00:00:00",
            file_last_modified: "2024-01-03T00:00:00",
            sharing_labels: &["clubhouse"],
            books: &[SeedBookRow {
                id: "book-alpha-1",
                name: "Alpha Book 1",
                title: "Alpha Book 1",
                created_date: "2024-01-01T00:00:00",
                last_modified_date: "2024-01-03T00:00:00",
                file_last_modified: "2024-01-03T00:00:00",
                number: 1,
            }],
        },
    )
    .await;
    seed_series(
        &fixture.paths.main_db,
        SeedSeriesRow {
            id: "series-beta",
            library_id: "library-main",
            name: "Beta Shelf",
            title: "Beta Brigade",
            title_sort: "Beta Brigade",
            summary: "beta summary",
            created_date: "2024-01-02T00:00:00",
            last_modified_date: "2024-01-04T00:00:00",
            file_last_modified: "2024-01-04T00:00:00",
            sharing_labels: &["vip"],
            books: &[SeedBookRow {
                id: "book-beta-1",
                name: "Beta Book 1",
                title: "Beta Book 1",
                created_date: "2024-01-02T00:00:00",
                last_modified_date: "2024-01-04T00:00:00",
                file_last_modified: "2024-01-04T00:00:00",
                number: 1,
            }],
        },
    )
    .await;
    seed_collection(
        &fixture.paths.main_db,
        "collection-curated",
        "Curated Picks",
        &[("series-beta", 0)],
    )
    .await;

    assert_eq!(series_row_count(&fixture.paths.main_db).await, 2);
    assert_eq!(collection_series_count(&fixture.paths.main_db, "collection-curated").await, 1);

    let token = admin_session_token(&fixture.app).await;
    let listed = request_json(
        &fixture.app,
        "GET",
        "/api/v1/series?sort=metadata.titleSort,asc&unpaged=true",
        &token,
        None,
    )
    .await;
    let collection_filtered = request_json(
        &fixture.app,
        "GET",
        "/api/v1/series?sort=metadata.titleSort,asc&unpaged=true&collection_id=collection-curated",
        &token,
        None,
    )
    .await;

    assert_eq!(listed["totalElements"], Value::from(2));
    assert_eq!(series_titles(&listed), vec!["Alpha Archive".to_string(), "Beta Brigade".to_string()]);
    assert_eq!(collection_filtered["totalElements"], Value::from(1));
    assert_eq!(series_ids(&collection_filtered), vec!["series-beta".to_string()]);
    assert_eq!(collection_filtered["content"][0]["metadata"]["sharingLabels"], json!(["vip"]));

    fixture.cleanup();
}

#[tokio::test]
async fn series_detail_and_collections_reflect_persisted_rows() {
    let fixture = SeriesContractFixture::new("series-detail-persisted").await;
    let library_root = create_library_root(&fixture.paths.config_dir, "series-detail-library");

    seed_library(&fixture.paths.main_db, "library-main", "Series Library", &library_root).await;
    seed_series(
        &fixture.paths.main_db,
        SeedSeriesRow {
            id: "series-detail",
            library_id: "library-main",
            name: "Detail Source",
            title: "Persisted Detail Title",
            title_sort: "Persisted Detail Title",
            summary: "persisted detail summary",
            created_date: "2024-01-05T00:00:00",
            last_modified_date: "2024-01-06T00:00:00",
            file_last_modified: "2024-01-06T00:00:00",
            sharing_labels: &["staff"],
            books: &[SeedBookRow {
                id: "book-detail-1",
                name: "Detail Book 1",
                title: "Detail Book 1",
                created_date: "2024-01-05T00:00:00",
                last_modified_date: "2024-01-06T00:00:00",
                file_last_modified: "2024-01-06T00:00:00",
                number: 1,
            }],
        },
    )
    .await;
    seed_collection(
        &fixture.paths.main_db,
        "collection-series-detail",
        "Series Detail Collection",
        &[("series-detail", 0)],
    )
    .await;

    assert_eq!(persisted_series_title(&fixture.paths.main_db, "series-detail").await, "Persisted Detail Title");

    let token = admin_session_token(&fixture.app).await;
    let detail = request_json(
        &fixture.app,
        "GET",
        "/api/v1/series/series-detail",
        &token,
        None,
    )
    .await;
    let collections = request_json(
        &fixture.app,
        "GET",
        "/api/v1/series/series-detail/collections",
        &token,
        None,
    )
    .await;

    assert_eq!(detail["id"], "series-detail");
    assert_eq!(detail["libraryId"], "library-main");
    assert_eq!(detail["name"], "Persisted Detail Title");
    assert_eq!(detail["metadata"]["title"], "Persisted Detail Title");
    assert_eq!(detail["metadata"]["summary"], "persisted detail summary");
    assert_eq!(detail["metadata"]["sharingLabels"], json!(["staff"]));
    assert_eq!(detail["booksCount"], Value::from(1));
    assert_eq!(collections.as_array().map(Vec::len), Some(1));
    assert_eq!(collections[0]["id"], "collection-series-detail");
    assert_eq!(collections[0]["name"], "Series Detail Collection");

    fixture.cleanup();
}

#[tokio::test]
async fn alphabetical_groups_reflect_persisted_series_title_buckets() {
    let fixture = SeriesContractFixture::new("series-alpha-groups").await;
    let library_root = create_library_root(&fixture.paths.config_dir, "series-alpha-library");

    seed_library(&fixture.paths.main_db, "library-main", "Series Library", &library_root).await;
    for (id, title) in [
        ("series-alpha", "Alpha Archive"),
        ("series-beta-1", "Beta Brigade"),
        ("series-beta-2", "Beta Beyond"),
    ] {
        seed_series(
            &fixture.paths.main_db,
            SeedSeriesRow {
                id,
                library_id: "library-main",
                name: title,
                title,
                title_sort: title,
                summary: "alphabetical grouping",
                created_date: "2024-01-01T00:00:00",
                last_modified_date: "2024-01-02T00:00:00",
                file_last_modified: "2024-01-02T00:00:00",
                sharing_labels: &[],
                books: &[SeedBookRow {
                    id: Box::leak(format!("book-{id}").into_boxed_str()),
                    name: "Grouped Book",
                    title: "Grouped Book",
                    created_date: "2024-01-01T00:00:00",
                    last_modified_date: "2024-01-02T00:00:00",
                    file_last_modified: "2024-01-02T00:00:00",
                    number: 1,
                }],
            },
        )
        .await;
    }

    let token = admin_session_token(&fixture.app).await;
    let groups = request_json(
        &fixture.app,
        "POST",
        "/api/v1/series/list/alphabetical-groups",
        &token,
        Some(json!({
            "condition": {
                "type": "LibraryId",
                "operator": "is",
                "value": "library-main"
            }
        })),
    )
    .await;

    assert_eq!(group_counts(&groups), vec![("A".to_string(), 1), ("B".to_string(), 2)]);

    fixture.cleanup();
}

#[tokio::test]
async fn latest_series_surface_orders_persisted_recent_activity() {
    let fixture = SeriesContractFixture::new("series-latest-persisted").await;
    let library_root = create_library_root(&fixture.paths.config_dir, "series-latest-library");

    seed_library(&fixture.paths.main_db, "library-main", "Series Library", &library_root).await;
    seed_series(
        &fixture.paths.main_db,
        SeedSeriesRow {
            id: "series-older",
            library_id: "library-main",
            name: "Older Series",
            title: "Older Series",
            title_sort: "Older Series",
            summary: "older summary",
            created_date: "2024-01-01T00:00:00",
            last_modified_date: "2024-01-02T00:00:00",
            file_last_modified: "2024-01-02T00:00:00",
            sharing_labels: &[],
            books: &[SeedBookRow {
                id: "book-older-1",
                name: "Older Book 1",
                title: "Older Book 1",
                created_date: "2024-01-01T00:00:00",
                last_modified_date: "2024-01-02T00:00:00",
                file_last_modified: "2024-01-02T00:00:00",
                number: 1,
            }],
        },
    )
    .await;
    seed_series(
        &fixture.paths.main_db,
        SeedSeriesRow {
            id: "series-fresh",
            library_id: "library-main",
            name: "Fresh Series",
            title: "Fresh Series",
            title_sort: "Fresh Series",
            summary: "fresh summary",
            created_date: "2024-01-10T00:00:00",
            last_modified_date: "2024-01-11T00:00:00",
            file_last_modified: "2024-01-11T00:00:00",
            sharing_labels: &[],
            books: &[SeedBookRow {
                id: "book-fresh-1",
                name: "Fresh Book 1",
                title: "Fresh Book 1",
                created_date: "2024-01-10T00:00:00",
                last_modified_date: "2024-01-11T00:00:00",
                file_last_modified: "2024-01-11T00:00:00",
                number: 1,
            }],
        },
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let latest = request_json(
        &fixture.app,
        "GET",
        "/api/v1/series/latest?library_id=library-main&unpaged=true",
        &token,
        None,
    )
    .await;

    assert_eq!(latest["totalElements"], Value::from(2));
    assert_eq!(series_ids(&latest), vec!["series-fresh".to_string(), "series-older".to_string()]);

    fixture.cleanup();
}

#[tokio::test]
async fn ordinary_books_latest_browse_prefers_persisted_rows_without_owned_marker() {
    let fixture = SeriesContractFixture::new("books-latest-ordinary-persisted").await;
    let library_root = create_library_root(&fixture.paths.config_dir, "books-latest-library");

    seed_library(&fixture.paths.main_db, "library-main", "Series Library", &library_root).await;
    seed_series(
        &fixture.paths.main_db,
        SeedSeriesRow {
            id: "series-older",
            library_id: "library-main",
            name: "Older Series",
            title: "Older Series",
            title_sort: "Older Series",
            summary: "older summary",
            created_date: "2024-01-01T00:00:00",
            last_modified_date: "2024-01-02T00:00:00",
            file_last_modified: "2024-01-02T00:00:00",
            sharing_labels: &[],
            books: &[SeedBookRow {
                id: "book-older-1",
                name: "Older Book 1",
                title: "Older Book 1",
                created_date: "2024-01-01T00:00:00",
                last_modified_date: "2024-01-02T00:00:00",
                file_last_modified: "2024-01-02T00:00:00",
                number: 1,
            }],
        },
    )
    .await;
    seed_series(
        &fixture.paths.main_db,
        SeedSeriesRow {
            id: "series-fresh",
            library_id: "library-main",
            name: "Fresh Series",
            title: "Fresh Series",
            title_sort: "Fresh Series",
            summary: "fresh summary",
            created_date: "2024-01-10T00:00:00",
            last_modified_date: "2024-01-11T00:00:00",
            file_last_modified: "2024-01-11T00:00:00",
            sharing_labels: &[],
            books: &[SeedBookRow {
                id: "book-fresh-1",
                name: "Fresh Book 1",
                title: "Fresh Book 1",
                created_date: "2024-01-10T00:00:00",
                last_modified_date: "2024-01-11T00:00:00",
                file_last_modified: "2024-01-11T00:00:00",
                number: 1,
            }],
        },
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let latest = request_json(
        &fixture.app,
        "GET",
        "/api/v1/books/latest?library_id=library-main&unpaged=true",
        &token,
        None,
    )
    .await;

    let latest_ids = latest
        .get("content")
        .and_then(Value::as_array)
        .expect("books latest payload should expose content array")
        .iter()
        .map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .expect("books latest entry should include id")
                .to_string()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        latest_ids,
        vec!["book-fresh-1".to_string(), "book-older-1".to_string()],
        "ordinary GET /api/v1/books/latest must prefer persisted latest ordering instead of snapshot fallback content",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn metadata_update_round_trips_through_follow_up_detail_reads() {
    let fixture = SeriesContractFixture::new("series-metadata-update").await;
    let library_root = create_library_root(&fixture.paths.config_dir, "series-update-library");

    seed_library(&fixture.paths.main_db, "library-main", "Series Library", &library_root).await;
    seed_series(
        &fixture.paths.main_db,
        SeedSeriesRow {
            id: "series-update",
            library_id: "library-main",
            name: "Before Update",
            title: "Before Update",
            title_sort: "Before Update",
            summary: "before summary",
            created_date: "2024-01-01T00:00:00",
            last_modified_date: "2024-01-02T00:00:00",
            file_last_modified: "2024-01-02T00:00:00",
            sharing_labels: &[],
            books: &[SeedBookRow {
                id: "book-update-1",
                name: "Update Book 1",
                title: "Update Book 1",
                created_date: "2024-01-01T00:00:00",
                last_modified_date: "2024-01-02T00:00:00",
                file_last_modified: "2024-01-02T00:00:00",
                number: 1,
            }],
        },
    )
    .await;
    assert_eq!(persisted_series_title(&fixture.paths.main_db, "series-update").await, "Before Update");

    let token = admin_session_token(&fixture.app).await;
    let patch_response = request(
        &fixture.app,
        "PATCH",
        "/api/v1/series/series-update/metadata",
        &token,
        Some(json!({
            "title": "After Update",
            "titleSort": "After Update",
            "summary": "after summary"
        })),
    )
    .await;
    assert_eq!(
        patch_response.status(),
        StatusCode::NO_CONTENT,
        "PATCH /api/v1/series/{{id}}/metadata must update persisted SERIES_METADATA rows",
    );

    let detail = request_json(
        &fixture.app,
        "GET",
        "/api/v1/series/series-update",
        &token,
        None,
    )
    .await;

    assert_eq!(detail["metadata"]["title"], "After Update");
    assert_eq!(detail["metadata"]["titleSort"], "After Update");
    assert_eq!(detail["metadata"]["summary"], "after summary");

    fixture.cleanup();
}

#[tokio::test]
async fn rejects_snapshot_series_payloads_after_persisted_follow_up_reads() {
    let fixture = SeriesContractFixture::new("series-reject-snapshot").await;
    let library_root = create_library_root(&fixture.paths.config_dir, "series-snapshot-library");

    seed_library(&fixture.paths.main_db, "1", "Series Library", &library_root).await;
    seed_series(
        &fixture.paths.main_db,
        SeedSeriesRow {
            id: "series-1",
            library_id: "1",
            name: "Snapshot Source",
            title: "Persisted Snapshot Source",
            title_sort: "Persisted Snapshot Source",
            summary: "snapshot rejection summary",
            created_date: "2024-01-07T00:00:00",
            last_modified_date: "2024-01-08T00:00:00",
            file_last_modified: "2024-01-08T00:00:00",
            sharing_labels: &["persisted-only"],
            books: &[SeedBookRow {
                id: "book-snapshot-1",
                name: "Snapshot Book 1",
                title: "Snapshot Book 1",
                created_date: "2024-01-07T00:00:00",
                last_modified_date: "2024-01-08T00:00:00",
                file_last_modified: "2024-01-08T00:00:00",
                number: 1,
            }],
        },
    )
    .await;
    update_series_metadata(
        &fixture.paths.main_db,
        "series-1",
        "Mutated Persisted Series",
        "Mutated Persisted Series",
        "mutated summary",
    )
    .await;
    assert_eq!(
        persisted_series_title(&fixture.paths.main_db, "series-1").await,
        "Mutated Persisted Series",
    );

    let token = admin_session_token(&fixture.app).await;
    let detail_after_mutation = request_json(
        &fixture.app,
        "GET",
        "/api/v1/series/series-1",
        &token,
        None,
    )
    .await;
    let list_after_mutation = request_json(
        &fixture.app,
        "GET",
        "/api/v1/series?search=Mutated%20Persisted&sort=metadata.titleSort,asc&unpaged=true",
        &token,
        None,
    )
    .await;

    assert_eq!(detail_after_mutation["metadata"]["title"], "Mutated Persisted Series");
    assert_eq!(detail_after_mutation["metadata"]["summary"], "mutated summary");
    assert_eq!(detail_after_mutation["metadata"]["sharingLabels"], json!(["persisted-only"]));
    assert_eq!(list_after_mutation["totalElements"], Value::from(1));
    assert_eq!(series_ids(&list_after_mutation), vec!["series-1".to_string()]);
    assert_eq!(list_after_mutation["content"][0]["metadata"]["title"], "Mutated Persisted Series");

    fixture.cleanup();
}

struct SeriesContractFixture {
    paths: persistence_contract_fixture::LegacyDbPaths,
    app: axum::Router,
}

impl SeriesContractFixture {
    async fn new(case_id: &str) -> Self {
        compat_auth_env::ensure_compat_auth_env();

        let paths = persistence_contract_fixture::new_legacy_db_paths(case_id)
            .expect("series contract db paths should be created");
        persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
            .await
            .expect("main db flyway fixture should be created");
        persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
            .await
            .expect("tasks db flyway fixture should be created");

        fs::create_dir_all(paths.config_dir.join("lucene"))
            .expect("lucene directory should be created for series contract fixture");
        fs::create_dir_all(paths.config_dir.join("fonts"))
            .expect("fonts directory should be created for series contract fixture");

        let mut config = RuntimeConfig::for_compat_profile(CompatProfile::SnapshotAligned);
        config.config_dir = Some(paths.config_dir.clone());
        config.log_file = paths.config_dir.join("komga.log");
        config.database_file = paths.main_db.clone();
        config.tasks_db_file = paths.tasks_db.clone();
        config.lucene_data_directory = paths.config_dir.join("lucene");
        config.fonts_data_directory = paths.config_dir.join("fonts");

        let app = komga_rust::app::build_router_with_config(&config);

        Self { paths, app }
    }

    fn cleanup(self) {
        persistence_contract_fixture::cleanup(self.paths);
    }
}

struct SeedSeriesRow<'a> {
    id: &'a str,
    library_id: &'a str,
    name: &'a str,
    title: &'a str,
    title_sort: &'a str,
    summary: &'a str,
    created_date: &'a str,
    last_modified_date: &'a str,
    file_last_modified: &'a str,
    sharing_labels: &'a [&'a str],
    books: &'a [SeedBookRow<'a>],
}

struct SeedBookRow<'a> {
    id: &'a str,
    name: &'a str,
    title: &'a str,
    created_date: &'a str,
    last_modified_date: &'a str,
    file_last_modified: &'a str,
    number: i32,
}

fn create_library_root(config_dir: &Path, name: &str) -> PathBuf {
    let root = config_dir.join(name);
    fs::create_dir_all(root.join("oneshots"))
        .expect("library root fixture directory should be created");
    root
}

async fn admin_session_token(app: &axum::Router) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(
                    header::AUTHORIZATION,
                    format!(
                        "Basic {}",
                        compat_auth_env::COMPAT_ADMIN_BASIC_AUTH_BASE64
                    ),
                )
                .header("X-Auth-Token", "")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .get("X-Auth-Token")
        .expect("login response should include X-Auth-Token")
        .to_str()
        .expect("session token should be valid utf-8")
        .to_string()
}

async fn request_json(
    app: &axum::Router,
    method: &str,
    path: &str,
    token: &str,
    body: Option<Value>,
) -> Value {
    let response = request(app, method, path, token, body).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "unexpected status for {method} {path}",
    );
    response_json(response).await
}

async fn request(
    app: &axum::Router,
    method: &str,
    path: &str,
    token: &str,
    body: Option<Value>,
) -> axum::response::Response {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("X-Auth-Token", token);

    let request_body = if let Some(body) = body {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        Body::from(body.to_string())
    } else {
        Body::empty()
    };

    app.clone()
        .oneshot(builder.body(request_body).unwrap())
        .await
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn seed_library(main_db: &Path, library_id: &str, name: &str, root: &Path) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for series library fixture seeding");

    sqlx::query(
        "INSERT INTO LIBRARY (ID, NAME, ROOT, SCAN_STARTUP, EMPTY_TRASH_AFTER_SCAN, ONESHOTS_DIRECTORY) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(library_id)
    .bind(name)
    .bind(root.to_string_lossy().to_string())
    .bind(false)
    .bind(false)
    .bind(None::<String>)
    .execute(&pool)
    .await
    .expect("series contract library fixture row should insert");

    pool.close().await;
}

async fn seed_series(main_db: &Path, series: SeedSeriesRow<'_>) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for series fixture seeding");

    sqlx::query(
        "INSERT INTO SERIES (ID, CREATED_DATE, LAST_MODIFIED_DATE, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, ONESHOT, DELETED_DATE) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(series.id)
    .bind(series.created_date)
    .bind(series.last_modified_date)
    .bind(series.file_last_modified)
    .bind(series.name)
    .bind(format!("/library/{}/series/{}", series.library_id, series.id))
    .bind(series.library_id)
    .bind(false)
    .bind(None::<String>)
    .execute(&pool)
    .await
    .expect("series fixture row should insert");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (CREATED_DATE, LAST_MODIFIED_DATE, STATUS, TITLE, TITLE_SORT, SUMMARY, LANGUAGE, PUBLISHER, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(series.created_date)
    .bind(series.last_modified_date)
    .bind("ONGOING")
    .bind(series.title)
    .bind(series.title_sort)
    .bind(series.summary)
    .bind("en")
    .bind("Komga Press")
    .bind(series.id)
    .execute(&pool)
    .await
    .expect("series metadata fixture row should insert");

    for sharing_label in series.sharing_labels {
        sqlx::query("INSERT INTO SERIES_METADATA_SHARING (LABEL, SERIES_ID) VALUES (?, ?)")
            .bind(sharing_label)
            .bind(series.id)
            .execute(&pool)
            .await
            .expect("series sharing label fixture row should insert");
    }

    for book in series.books {
        sqlx::query(
            "INSERT INTO BOOK (ID, CREATED_DATE, LAST_MODIFIED_DATE, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, ONESHOT, DELETED_DATE) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(book.id)
        .bind(book.created_date)
        .bind(book.last_modified_date)
        .bind(book.file_last_modified)
        .bind(book.name)
        .bind(format!("/library/{}/books/{}", series.library_id, book.id))
        .bind(series.id)
        .bind(1_i64)
        .bind(book.number)
        .bind(series.library_id)
        .bind(false)
        .bind(None::<String>)
        .execute(&pool)
        .await
        .expect("series contract book fixture row should insert");

        sqlx::query(
            "INSERT INTO BOOK_METADATA (CREATED_DATE, LAST_MODIFIED_DATE, NUMBER, NUMBER_SORT, TITLE, SUMMARY, BOOK_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(book.created_date)
        .bind(book.last_modified_date)
        .bind(book.number.to_string())
        .bind(book.number as f64)
        .bind(book.title)
        .bind(format!("summary for {}", book.title))
        .bind(book.id)
        .execute(&pool)
        .await
        .expect("series contract book metadata fixture row should insert");

        sqlx::query("INSERT INTO MEDIA (STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?)")
            .bind("READY")
            .bind(book.id)
            .bind(1_i64)
            .execute(&pool)
            .await
            .expect("series contract media fixture row should insert");
    }

    pool.close().await;
}

async fn seed_collection(
    main_db: &Path,
    collection_id: &str,
    name: &str,
    members: &[(&str, i32)],
) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for series collection fixture seeding");

    sqlx::query(
        "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT, CREATED_DATE, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(collection_id)
    .bind(name)
    .bind(true)
    .bind(members.len() as i32)
    .bind("2024-01-01T00:00:00")
    .bind("2024-01-02T00:00:00")
    .execute(&pool)
    .await
    .expect("series collection fixture row should insert");

    for (series_id, number) in members {
        sqlx::query(
            "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
        )
        .bind(collection_id)
        .bind(series_id)
        .bind(number)
        .execute(&pool)
        .await
        .expect("series collection membership fixture row should insert");
    }

    pool.close().await;
}

async fn update_series_metadata(
    main_db: &Path,
    series_id: &str,
    title: &str,
    title_sort: &str,
    summary: &str,
) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for series metadata mutation");

    sqlx::query(
        "UPDATE SERIES_METADATA SET TITLE = ?, TITLE_SORT = ?, SUMMARY = ?, LAST_MODIFIED_DATE = ? WHERE SERIES_ID = ?",
    )
    .bind(title)
    .bind(title_sort)
    .bind(summary)
    .bind("2024-02-01T00:00:00")
    .bind(series_id)
    .execute(&pool)
    .await
    .expect("series metadata fixture mutation should succeed");

    pool.close().await;
}

async fn series_row_count(main_db: &Path) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for series count inspection");
    let count = sqlx::query("SELECT COUNT(*) AS COUNT FROM SERIES")
        .fetch_one(&pool)
        .await
        .expect("series count should be queryable")
        .get::<i64, _>("COUNT");
    pool.close().await;
    count
}

async fn collection_series_count(main_db: &Path, collection_id: &str) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for collection membership inspection");
    let count = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM COLLECTION_SERIES WHERE COLLECTION_ID = ?",
    )
    .bind(collection_id)
    .fetch_one(&pool)
    .await
    .expect("collection membership count should be queryable")
    .get::<i64, _>("COUNT");
    pool.close().await;
    count
}

async fn persisted_series_title(main_db: &Path, series_id: &str) -> String {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for series title inspection");
    let title = sqlx::query("SELECT TITLE FROM SERIES_METADATA WHERE SERIES_ID = ?")
        .bind(series_id)
        .fetch_one(&pool)
        .await
        .expect("persisted series title should be queryable")
        .get::<String, _>("TITLE");
    pool.close().await;
    title
}

fn series_ids(payload: &Value) -> Vec<String> {
    payload
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|series| series.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

fn series_titles(payload: &Value) -> Vec<String> {
    payload
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|series| series.pointer("/metadata/title").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

fn group_counts(payload: &Value) -> Vec<(String, i64)> {
    payload
        .as_array()
        .into_iter()
        .flatten()
        .map(|group| {
            (
                group
                    .get("group")
                    .and_then(Value::as_str)
                    .expect("alphabetical group payload should include group")
                    .to_string(),
                group
                    .get("count")
                    .and_then(Value::as_i64)
                    .expect("alphabetical group payload should include count"),
            )
        })
        .collect()
}
