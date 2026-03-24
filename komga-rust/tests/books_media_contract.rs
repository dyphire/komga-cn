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

const PLACEHOLDER_PDF_BYTES: &[u8] = b"%PDF-1.7\n%komga-rust-placeholder\n";
const PLACEHOLDER_THUMBNAIL_BYTES: &[u8] = b"\xff\xd8\xff\xdb\x00C\x00placeholder-jpeg\xff\xd9";

#[test]
fn books_media_contract_target_is_registered() {
    assert_required_target_declared("books/media", "books_media_contract");
}

#[tokio::test]
async fn book_detail_reads_persisted_rows_instead_of_seeded_snapshot_records() {
    let fixture = BooksMediaContractFixture::new("books-detail-persisted").await;
    seed_persisted_book_bundle(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedBookBundle {
            library_id: "library-books",
            series_id: "series-books",
            book_id: "book-persisted-1",
            title: "Persisted Media Contract Book",
            file_name: "persisted-book.cbz",
            created_date: "2024-03-01T00:00:00",
            last_modified_date: "2024-03-03T00:00:00",
        },
    )
    .await;

    assert_eq!(book_row_count(&fixture.paths.main_db, "book-persisted-1").await, 1);

    let token = admin_session_token(&fixture.app).await;
    let response = request(
        &fixture.app,
        "GET",
        "/api/v1/books/book-persisted-1",
        &token,
        None,
        &[],
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "book detail contract requires GET /api/v1/books/{{id}} to resolve persisted BOOK/BOOK_METADATA rows, not only seeded snapshot ids",
    );

    let payload = response_json(response).await;
    assert_eq!(payload["id"], "book-persisted-1");
    assert_eq!(payload["libraryId"], "library-books");
    assert_eq!(payload["metadata"]["title"], "Persisted Media Contract Book");

    fixture.cleanup();
}

#[tokio::test]
async fn page_delivery_uses_persisted_media_content_type_and_cache_semantics() {
    let fixture = BooksMediaContractFixture::new("books-page-transport").await;
    seed_persisted_book_bundle(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedBookBundle {
            library_id: "library-books",
            series_id: "series-books",
            book_id: "book-persisted-1",
            title: "Persisted Media Contract Book",
            file_name: "persisted-book.cbz",
            created_date: "2024-03-01T00:00:00",
            last_modified_date: "2024-03-03T00:00:00",
        },
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let first = request(
        &fixture.app,
        "GET",
        "/api/v1/books/book-persisted-1/pages/1",
        &token,
        None,
        &[(header::ACCEPT.as_str(), "image/jpeg")],
    )
    .await;

    assert_eq!(
        first.status(),
        StatusCode::OK,
        "page delivery contract requires persisted book pages to be served",
    );
    assert_eq!(
        first
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("page response should include content type"),
        "image/jpeg",
        "page delivery must expose Kotlin-visible page media content type semantics rather than fixed placeholder PDF headers",
    );
    assert_eq!(
        first
            .headers()
            .get(header::CACHE_CONTROL)
            .expect("page response should include cache-control"),
        "max-age=0, must-revalidate, private",
    );

    let last_modified = first
        .headers()
        .get(header::LAST_MODIFIED)
        .expect("page response should include last-modified")
        .to_str()
        .expect("last-modified header should be utf-8")
        .to_string();

    let cached = request(
        &fixture.app,
        "GET",
        "/api/v1/books/book-persisted-1/pages/1",
        &token,
        None,
        &[(header::IF_MODIFIED_SINCE.as_str(), &last_modified)],
    )
    .await;

    assert_eq!(
        cached.status(),
        StatusCode::NOT_MODIFIED,
        "page delivery contract requires If-Modified-Since parity for persisted media pages",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn page_delivery_remaps_snapshot_book_id_to_persisted_media_payload() {
    let fixture = BooksMediaContractFixture::new("books-page-transport-snapshot-remap").await;
    seed_persisted_book_bundle(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedBookBundle {
            library_id: "library-books",
            series_id: "series-books",
            book_id: "book-persisted-1",
            title: "Persisted Media Contract Book",
            file_name: "persisted-book.cbz",
            created_date: "2024-03-01T00:00:00",
            last_modified_date: "2024-03-03T00:00:00",
        },
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let response = request(
        &fixture.app,
        "GET",
        "/api/v1/books/book-1/pages/1",
        &token,
        None,
        &[],
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "snapshot-style book ids must resolve to persisted book pages for reader parity",
    );
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("page response should include content type"),
        "image/jpeg",
        "reader page endpoint must not regress to placeholder pdf content-type when a persisted page exists",
    );

    let bytes = response_bytes(response).await;
    assert_ne!(
        bytes.as_ref(),
        PLACEHOLDER_PDF_BYTES,
        "snapshot-style page route must not return fixed placeholder pdf bytes",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn file_delivery_supports_range_headers_and_persisted_filename_content_type() {
    let fixture = BooksMediaContractFixture::new("books-file-transport").await;
    seed_persisted_book_bundle(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedBookBundle {
            library_id: "library-books",
            series_id: "series-books",
            book_id: "book-persisted-1",
            title: "Persisted Media Contract Book",
            file_name: "persisted-book.cbz",
            created_date: "2024-03-01T00:00:00",
            last_modified_date: "2024-03-03T00:00:00",
        },
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let response = request(
        &fixture.app,
        "GET",
        "/api/v1/books/book-persisted-1/file",
        &token,
        None,
        &[(header::RANGE.as_str(), "bytes=0-7")],
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::PARTIAL_CONTENT,
        "file delivery contract requires byte-range support for persisted books",
    );
    assert_eq!(
        response
            .headers()
            .get(header::ACCEPT_RANGES)
            .expect("file response should include Accept-Ranges"),
        "bytes",
    );
    assert!(
        response
            .headers()
            .get(header::CONTENT_RANGE)
            .expect("file response should include Content-Range")
            .to_str()
            .expect("content-range should be utf-8")
            .starts_with("bytes 0-7/"),
    );
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("file response should include content type"),
        "application/vnd.comicbook+zip",
        "file delivery must expose persisted media content type rather than fixed placeholder type",
    );

    let disposition = response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .expect("file response should include content-disposition")
        .to_str()
        .expect("content-disposition should be utf-8");
    assert!(
        disposition.contains("persisted-book.cbz"),
        "file delivery must derive attachment filename from persisted book media metadata",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn book_thumbnail_delivery_must_not_be_fixed_not_found_when_persisted_book_exists() {
    let fixture = BooksMediaContractFixture::new("books-thumbnail-transport").await;
    seed_persisted_book_bundle(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedBookBundle {
            library_id: "library-books",
            series_id: "series-books",
            book_id: "book-persisted-1",
            title: "Persisted Media Contract Book",
            file_name: "persisted-book.cbz",
            created_date: "2024-03-01T00:00:00",
            last_modified_date: "2024-03-03T00:00:00",
        },
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let response = request(
        &fixture.app,
        "GET",
        "/api/v1/books/book-persisted-1/thumbnail",
        &token,
        None,
        &[],
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "book thumbnail delivery contract rejects fixed 404 behavior for persisted books",
    );
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("thumbnail response should include content type"),
        "image/jpeg",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn rejects_placeholder_media_payload_bytes_for_persisted_book_transport() {
    let fixture = BooksMediaContractFixture::new("books-reject-placeholder-media").await;
    seed_persisted_book_bundle(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedBookBundle {
            library_id: "library-books",
            series_id: "series-books",
            book_id: "book-persisted-1",
            title: "Persisted Media Contract Book",
            file_name: "persisted-book.cbz",
            created_date: "2024-03-01T00:00:00",
            last_modified_date: "2024-03-03T00:00:00",
        },
    )
    .await;

    let token = admin_session_token(&fixture.app).await;

    let page_response = request(
        &fixture.app,
        "GET",
        "/api/v1/books/book-persisted-1/pages/1",
        &token,
        None,
        &[],
    )
    .await;
    assert_eq!(page_response.status(), StatusCode::OK);
    let page_bytes = response_bytes(page_response).await;
    assert_ne!(
        page_bytes.as_ref(),
        PLACEHOLDER_PDF_BYTES,
        "book page delivery must not return the fixed placeholder PDF payload",
    );

    let page_thumbnail_response = request(
        &fixture.app,
        "GET",
        "/api/v1/books/book-persisted-1/pages/1/thumbnail",
        &token,
        None,
        &[],
    )
    .await;
    assert_eq!(page_thumbnail_response.status(), StatusCode::OK);
    let page_thumbnail_bytes = response_bytes(page_thumbnail_response).await;
    assert_ne!(
        page_thumbnail_bytes.as_ref(),
        PLACEHOLDER_THUMBNAIL_BYTES,
        "page thumbnail delivery must not return fixed placeholder JPEG bytes",
    );

    let file_response = request(
        &fixture.app,
        "GET",
        "/api/v1/books/book-persisted-1/file",
        &token,
        None,
        &[],
    )
    .await;
    assert_eq!(file_response.status(), StatusCode::OK);
    let file_bytes = response_bytes(file_response).await;
    assert_ne!(
        file_bytes.as_ref(),
        PLACEHOLDER_PDF_BYTES,
        "book file delivery must not return fixed placeholder PDF bytes",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn read_progress_get_patch_delete_must_roundtrip_through_persisted_rows() {
    let fixture = BooksMediaContractFixture::new("books-read-progress-mutation").await;
    seed_persisted_book_bundle(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedBookBundle {
            library_id: "library-books",
            series_id: "series-books",
            book_id: "book-persisted-1",
            title: "Persisted Media Contract Book",
            file_name: "persisted-book.cbz",
            created_date: "2024-03-01T00:00:00",
            last_modified_date: "2024-03-03T00:00:00",
        },
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let user_id = current_user_id(&fixture.app, &token).await;

    let get_response = request(
        &fixture.app,
        "GET",
        "/api/v1/books/book-persisted-1/read-progress",
        &token,
        None,
        &[],
    )
    .await;
    assert_eq!(
        get_response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "read-progress GET contract must keep Kotlin-style method-not-allowed semantics for book-specific endpoint",
    );
    let get_payload = response_json(get_response).await;
    assert_eq!(get_payload["path"], "/api/v1/books/book-persisted-1/read-progress");

    let patch_response = request(
        &fixture.app,
        "PATCH",
        "/api/v1/books/book-persisted-1/read-progress",
        &token,
        Some(json!({ "page": 1 })),
        &[],
    )
    .await;
    assert_eq!(
        patch_response.status(),
        StatusCode::NO_CONTENT,
        "read-progress PATCH contract requires persisted books to accept page updates",
    );

    assert_eq!(
        persisted_read_progress_for_book(&fixture.paths.main_db, "book-persisted-1", &user_id).await,
        Some((1, false)),
        "read-progress PATCH contract rejects memory-only behavior: READ_PROGRESS row must be persisted",
    );

    let delete_response = request(
        &fixture.app,
        "DELETE",
        "/api/v1/books/book-persisted-1/read-progress",
        &token,
        None,
        &[],
    )
    .await;
    assert_eq!(
        delete_response.status(),
        StatusCode::NO_CONTENT,
        "read-progress DELETE contract requires persisted books to support unread reset",
    );
    assert_eq!(
        persisted_read_progress_for_book(&fixture.paths.main_db, "book-persisted-1", &user_id).await,
        None,
        "read-progress DELETE contract requires persisted READ_PROGRESS row removal",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn books_ondeck_surface_must_derive_from_persisted_read_progress_and_series_order() {
    let fixture = BooksMediaContractFixture::new("books-ondeck-persisted").await;
    seed_persisted_book_bundle(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedBookBundle {
            library_id: "library-books",
            series_id: "series-books",
            book_id: "book-persisted-1",
            title: "Persisted Book One",
            file_name: "persisted-book-one.cbz",
            created_date: "2024-03-01T00:00:00",
            last_modified_date: "2024-03-03T00:00:00",
        },
    )
    .await;
    seed_persisted_book_in_existing_series(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedAdditionalBook {
            series_id: "series-books",
            library_id: "library-books",
            book_id: "book-persisted-2",
            title: "Persisted Book Two",
            file_name: "persisted-book-two.cbz",
            number: 2,
            created_date: "2024-03-02T00:00:00",
            last_modified_date: "2024-03-04T00:00:00",
        },
    )
    .await;
    let token = admin_session_token(&fixture.app).await;
    let user_id = current_user_id(&fixture.app, &token).await;
    ensure_user_row_exists(&fixture.paths.main_db, &user_id).await;
    upsert_read_progress(
        &fixture.paths.main_db,
        "book-persisted-1",
        &user_id,
        1,
        true,
    )
    .await;

    let response = request(
        &fixture.app,
        "GET",
        "/api/v1/books/ondeck?unpaged=true",
        &token,
        None,
        &[],
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "ondeck contract requires a dedicated persisted-backed endpoint, not book-id fallback routing",
    );
    let payload = response_json(response).await;
    assert_eq!(
        payload["content"][0]["id"],
        "book-persisted-2",
        "ondeck contract should surface first unread book after a completed predecessor in series order",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn books_duplicates_surface_must_list_persisted_duplicate_file_hash_groups() {
    let fixture = BooksMediaContractFixture::new("books-duplicates-persisted").await;
    seed_persisted_book_bundle(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedBookBundle {
            library_id: "library-books-1",
            series_id: "series-books-1",
            book_id: "book-duplicate-1",
            title: "Duplicate One",
            file_name: "duplicate-one.cbz",
            created_date: "2024-03-01T00:00:00",
            last_modified_date: "2024-03-03T00:00:00",
        },
    )
    .await;
    seed_persisted_book_bundle(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedBookBundle {
            library_id: "library-books-2",
            series_id: "series-books-2",
            book_id: "book-duplicate-2",
            title: "Duplicate Two",
            file_name: "duplicate-two.cbz",
            created_date: "2024-03-02T00:00:00",
            last_modified_date: "2024-03-04T00:00:00",
        },
    )
    .await;
    set_book_file_hash(&fixture.paths.main_db, "book-duplicate-1", "hash-duplicate-group").await;
    set_book_file_hash(&fixture.paths.main_db, "book-duplicate-2", "hash-duplicate-group").await;

    let token = admin_session_token(&fixture.app).await;
    let response = request(
        &fixture.app,
        "GET",
        "/api/v1/books/duplicates?unpaged=true",
        &token,
        None,
        &[],
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "duplicates contract requires a persisted duplicate-book surface keyed by BOOK.FILE_HASH groups",
    );
    let payload = response_json(response).await;
    let ids = payload["content"]
        .as_array()
        .expect("duplicates payload should include pageable content")
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert!(
        ids.contains(&"book-duplicate-1") && ids.contains(&"book-duplicate-2"),
        "duplicates contract should include both persisted books sharing the same file hash",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn delete_book_file_route_must_match_kotlin_async_mutation_semantics() {
    let fixture = BooksMediaContractFixture::new("books-file-delete-mutation").await;
    seed_persisted_book_bundle(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedBookBundle {
            library_id: "library-books",
            series_id: "series-books",
            book_id: "book-persisted-1",
            title: "Persisted Media Contract Book",
            file_name: "persisted-book.cbz",
            created_date: "2024-03-01T00:00:00",
            last_modified_date: "2024-03-03T00:00:00",
        },
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let response = request(
        &fixture.app,
        "DELETE",
        "/api/v1/books/book-persisted-1/file",
        &token,
        None,
        &[],
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::ACCEPTED,
        "delete book file contract requires async accepted status for persisted books",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn book_thumbnail_list_and_batch_routes_must_honor_persisted_mutation_contract() {
    let fixture = BooksMediaContractFixture::new("books-thumbnail-list-batch").await;
    seed_persisted_book_bundle(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedBookBundle {
            library_id: "library-books",
            series_id: "series-books",
            book_id: "book-persisted-1",
            title: "Persisted Media Contract Book",
            file_name: "persisted-book.cbz",
            created_date: "2024-03-01T00:00:00",
            last_modified_date: "2024-03-03T00:00:00",
        },
    )
    .await;
    insert_book_thumbnail_row(
        &fixture.paths.main_db,
        "thumbnail-persisted-1",
        "book-persisted-1",
        true,
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let list_response = request(
        &fixture.app,
        "GET",
        "/api/v1/books/book-persisted-1/thumbnails",
        &token,
        None,
        &[],
    )
    .await;
    assert_eq!(
        list_response.status(),
        StatusCode::OK,
        "book thumbnails list contract requires persisted THUMBNAIL_BOOK rows for persisted book ids",
    );
    let list_payload = response_json(list_response).await;
    assert_eq!(list_payload[0]["id"], "thumbnail-persisted-1");
    assert_eq!(list_payload[0]["selected"], true);

    let batch_response = request(
        &fixture.app,
        "PUT",
        "/api/v1/books/thumbnails",
        &token,
        None,
        &[],
    )
    .await;
    assert_eq!(
        batch_response.status(),
        StatusCode::ACCEPTED,
        "books thumbnail batch contract requires accepted status for regeneration mutation route",
    );

    fixture.cleanup();
}

struct BooksMediaContractFixture {
    paths: persistence_contract_fixture::LegacyDbPaths,
    app: axum::Router,
    library_root: PathBuf,
}

impl BooksMediaContractFixture {
    async fn new(case_id: &str) -> Self {
        compat_auth_env::ensure_compat_auth_env();

        let paths = persistence_contract_fixture::new_legacy_db_paths(case_id)
            .expect("books/media contract db paths should be created");
        persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
            .await
            .expect("main db flyway fixture should be created");
        persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
            .await
            .expect("tasks db flyway fixture should be created");

        fs::create_dir_all(paths.config_dir.join("lucene"))
            .expect("lucene directory should be created for books/media contract fixture");
        fs::create_dir_all(paths.config_dir.join("fonts"))
            .expect("fonts directory should be created for books/media contract fixture");

        let library_root = create_library_root(&paths.config_dir, "books-media-library");

        let mut config = RuntimeConfig::for_compat_profile(CompatProfile::SnapshotAligned);
        config.config_dir = Some(paths.config_dir.clone());
        config.log_file = paths.config_dir.join("komga.log");
        config.database_file = paths.main_db.clone();
        config.tasks_db_file = paths.tasks_db.clone();
        config.lucene_data_directory = paths.config_dir.join("lucene");
        config.fonts_data_directory = paths.config_dir.join("fonts");

        let app = komga_rust::app::build_router_with_config(&config);

        Self {
            paths,
            app,
            library_root,
        }
    }

    fn cleanup(self) {
        persistence_contract_fixture::cleanup(self.paths);
    }
}

struct SeedBookBundle<'a> {
    library_id: &'a str,
    series_id: &'a str,
    book_id: &'a str,
    title: &'a str,
    file_name: &'a str,
    created_date: &'a str,
    last_modified_date: &'a str,
}

struct SeedAdditionalBook<'a> {
    library_id: &'a str,
    series_id: &'a str,
    book_id: &'a str,
    title: &'a str,
    file_name: &'a str,
    number: i32,
    created_date: &'a str,
    last_modified_date: &'a str,
}

fn create_library_root(config_dir: &Path, name: &str) -> PathBuf {
    let root = config_dir.join(name);
    fs::create_dir_all(&root).expect("library root fixture directory should be created");
    root
}

async fn seed_persisted_book_bundle(main_db: &Path, library_root: &Path, bundle: SeedBookBundle<'_>) {
    let media_file_path = library_root.join(bundle.file_name);
    fs::write(&media_file_path, b"persisted-media-payload")
        .expect("persisted media fixture file should be written");

    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for books/media fixture seeding");

    sqlx::query(
        "INSERT INTO LIBRARY (ID, NAME, ROOT, SCAN_STARTUP, EMPTY_TRASH_AFTER_SCAN, ONESHOTS_DIRECTORY) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(bundle.library_id)
    .bind("Books Media Library")
    .bind(library_root.to_string_lossy().to_string())
    .bind(false)
    .bind(false)
    .bind(None::<String>)
    .execute(&pool)
    .await
    .expect("library fixture row should insert");

    sqlx::query(
        "INSERT INTO SERIES (ID, CREATED_DATE, LAST_MODIFIED_DATE, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, ONESHOT, DELETED_DATE) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(bundle.series_id)
    .bind(bundle.created_date)
    .bind(bundle.last_modified_date)
    .bind(bundle.last_modified_date)
    .bind("Books Media Series")
    .bind(format!("/library/{}/series/{}", bundle.library_id, bundle.series_id))
    .bind(bundle.library_id)
    .bind(false)
    .bind(None::<String>)
    .execute(&pool)
    .await
    .expect("series fixture row should insert");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (CREATED_DATE, LAST_MODIFIED_DATE, STATUS, TITLE, TITLE_SORT, SUMMARY, LANGUAGE, PUBLISHER, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(bundle.created_date)
    .bind(bundle.last_modified_date)
    .bind("ONGOING")
    .bind("Books Media Series")
    .bind("Books Media Series")
    .bind("books media series summary")
    .bind("en")
    .bind("Komga Press")
    .bind(bundle.series_id)
    .execute(&pool)
    .await
    .expect("series metadata fixture row should insert");

    sqlx::query(
        "INSERT INTO BOOK (ID, CREATED_DATE, LAST_MODIFIED_DATE, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, ONESHOT, DELETED_DATE) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(bundle.book_id)
    .bind(bundle.created_date)
    .bind(bundle.last_modified_date)
    .bind(bundle.last_modified_date)
    .bind(bundle.file_name)
    .bind(format!("/library/{}/books/{}", bundle.library_id, bundle.file_name))
    .bind(bundle.series_id)
    .bind(22_i64)
    .bind(1_i32)
    .bind(bundle.library_id)
    .bind(false)
    .bind(None::<String>)
    .execute(&pool)
    .await
    .expect("book fixture row should insert");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (CREATED_DATE, LAST_MODIFIED_DATE, NUMBER, NUMBER_SORT, TITLE, SUMMARY, BOOK_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(bundle.created_date)
    .bind(bundle.last_modified_date)
    .bind("1")
    .bind(1.0_f64)
    .bind(bundle.title)
    .bind("persisted media contract summary")
    .bind(bundle.book_id)
    .execute(&pool)
    .await
    .expect("book metadata fixture row should insert");

    sqlx::query(
        "INSERT INTO MEDIA (STATUS, MEDIA_TYPE, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)",
    )
    .bind("READY")
    .bind("application/vnd.comicbook+zip")
    .bind(bundle.book_id)
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("media fixture row should insert");

    pool.close().await;
}

async fn seed_persisted_book_in_existing_series(
    main_db: &Path,
    library_root: &Path,
    bundle: SeedAdditionalBook<'_>,
) {
    let media_file_path = library_root.join(bundle.file_name);
    fs::write(&media_file_path, b"persisted-media-payload")
        .expect("persisted media fixture file should be written");

    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for books/media fixture seeding");

    sqlx::query(
        "INSERT INTO BOOK (ID, CREATED_DATE, LAST_MODIFIED_DATE, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, ONESHOT, DELETED_DATE) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(bundle.book_id)
    .bind(bundle.created_date)
    .bind(bundle.last_modified_date)
    .bind(bundle.last_modified_date)
    .bind(bundle.file_name)
    .bind(format!("/library/{}/books/{}", bundle.library_id, bundle.file_name))
    .bind(bundle.series_id)
    .bind(22_i64)
    .bind(bundle.number)
    .bind(bundle.library_id)
    .bind(false)
    .bind(None::<String>)
    .execute(&pool)
    .await
    .expect("additional book fixture row should insert");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (CREATED_DATE, LAST_MODIFIED_DATE, NUMBER, NUMBER_SORT, TITLE, SUMMARY, BOOK_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(bundle.created_date)
    .bind(bundle.last_modified_date)
    .bind(bundle.number.to_string())
    .bind(bundle.number as f64)
    .bind(bundle.title)
    .bind("persisted media contract summary")
    .bind(bundle.book_id)
    .execute(&pool)
    .await
    .expect("additional book metadata fixture row should insert");

    sqlx::query(
        "INSERT INTO MEDIA (STATUS, MEDIA_TYPE, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)",
    )
    .bind("READY")
    .bind("application/vnd.comicbook+zip")
    .bind(bundle.book_id)
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("additional media fixture row should insert");

    pool.close().await;
}

async fn current_user_id(app: &axum::Router, token: &str) -> String {
    let response = request(app, "GET", "/api/v2/users/me", token, None, &[]).await;
    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    payload["id"]
        .as_str()
        .expect("users/me payload should include user id")
        .to_string()
}

async fn ensure_user_row_exists(main_db: &Path, user_id: &str) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for user fixture seeding");
    sqlx::query("INSERT OR IGNORE INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES) VALUES (?, ?, ?, ?)")
        .bind(user_id)
        .bind(format!("{user_id}@compat.local"))
        .bind("test-password")
        .bind(true)
        .execute(&pool)
        .await
        .expect("user fixture row should insert or already exist");
    pool.close().await;
}

async fn upsert_read_progress(
    main_db: &Path,
    book_id: &str,
    user_id: &str,
    page: i64,
    completed: bool,
) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for read-progress fixture seeding");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?) ON CONFLICT(BOOK_ID, USER_ID) DO UPDATE SET PAGE=excluded.PAGE, COMPLETED=excluded.COMPLETED, LAST_MODIFIED_DATE=CURRENT_TIMESTAMP",
    )
    .bind(book_id)
    .bind(user_id)
    .bind(page)
    .bind(completed)
    .execute(&pool)
    .await
    .expect("read-progress fixture row should upsert");
    pool.close().await;
}

async fn persisted_read_progress_for_book(
    main_db: &Path,
    book_id: &str,
    user_id: &str,
) -> Option<(i64, bool)> {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for read-progress inspection");
    let row = sqlx::query("SELECT PAGE, COMPLETED FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ?")
        .bind(book_id)
        .bind(user_id)
        .fetch_optional(&pool)
        .await
        .expect("read-progress row should be queryable");
    pool.close().await;

    row.map(|row| {
        let page = row.get::<i64, _>("PAGE");
        let completed_raw = row.get::<i64, _>("COMPLETED");
        (page, completed_raw != 0)
    })
}

async fn set_book_file_hash(main_db: &Path, book_id: &str, file_hash: &str) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for duplicate fixture update");
    sqlx::query("UPDATE BOOK SET FILE_HASH = ? WHERE ID = ?")
        .bind(file_hash)
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("book file hash should update");
    pool.close().await;
}

async fn insert_book_thumbnail_row(main_db: &Path, thumbnail_id: &str, book_id: &str, selected: bool) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for thumbnail fixture seeding");
    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, THUMBNAIL, SELECTED, TYPE, BOOK_ID, WIDTH, HEIGHT, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(thumbnail_id)
    .bind(PLACEHOLDER_THUMBNAIL_BYTES)
    .bind(selected)
    .bind("SIDECAR")
    .bind(book_id)
    .bind(300_i32)
    .bind(450_i32)
    .bind("image/jpeg")
    .bind(PLACEHOLDER_THUMBNAIL_BYTES.len() as i64)
    .execute(&pool)
    .await
    .expect("thumbnail fixture row should insert");
    pool.close().await;
}

async fn book_row_count(main_db: &Path, book_id: &str) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for book fixture inspection");
    let count = sqlx::query("SELECT COUNT(*) AS COUNT FROM BOOK WHERE ID = ?")
        .bind(book_id)
        .fetch_one(&pool)
        .await
        .expect("book row count should be queryable")
        .get::<i64, _>("COUNT");
    pool.close().await;
    count
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

async fn request(
    app: &axum::Router,
    method: &str,
    path: &str,
    token: &str,
    body: Option<Value>,
    extra_headers: &[(&str, &str)],
) -> axum::response::Response {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("X-Auth-Token", token);

    for (name, value) in extra_headers {
        builder = builder.header(*name, *value);
    }

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

async fn response_bytes(response: axum::response::Response) -> axum::body::Bytes {
    axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable")
}
