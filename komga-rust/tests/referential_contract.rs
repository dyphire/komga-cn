use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use komga_compat_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::config::{CompatProfile, RuntimeConfig};
use komga_rust::persistence::sqlite::connect_pool;
use serde_json::Value;
use sqlx::Row;
use tower::ServiceExt;

#[path = "compat/auth_env.rs"]
mod compat_auth_env;

#[path = "support/persistence_contract_fixture.rs"]
mod persistence_contract_fixture;

const SEARCH_OWNERSHIP_HEADER: &str = "x-komga-compat-search-ownership";
const NATIVE_OWNERSHIP_MARKER: &str = "native-rust-owned";
const LIBRARY_ID: &str = "library-referential";
const SERIES_ID: &str = "series-referential";
const BOOK_ID: &str = "book-referential";
const AUTHOR_NAME: &str = "Persisted Referential Author";
const AUTHOR_ROLE: &str = "writer";
const GENRE: &str = "Persisted Referential Genre";
const SERIES_TAG: &str = "Persisted Series Tag";
const BOOK_TAG: &str = "Persisted Book Tag";
const LANGUAGE: &str = "fr";
const PUBLISHER: &str = "Persisted Referential Press";
const CATALOG_ADMIN_USER_ID: &str = "catalog-admin";
const CATALOG_ADMIN_EMAIL: &str = "catalog-admin@example.org";
const CATALOG_ADMIN_PASSWORD_BCRYPT: &str =
    "$2a$10$x7NyXzncFgR/Nd/VR8eYde9njk/JaWz1X05C1wkk1G89dZnmVpw3e";
const CATALOG_USER_ID: &str = "catalog-user";
const CATALOG_USER_EMAIL: &str = "catalog-user@example.org";
const CATALOG_USER_PASSWORD: &str = "db-user-password";
const CATALOG_USER_PASSWORD_BCRYPT: &str =
    "$2a$10$6uBfM3Iovphyo.x1KDYFa.kdgG/Wth9mRYP9wQDTwYF0ShEXc6/4m";
const LABEL_ALLOW: &str = "safe";
const LABEL_EXCLUDE: &str = "adult";
const USER_AGE_RESTRICTION: i64 = 16;

#[test]
fn referential_contract_target_is_registered() {
    assert_required_target_declared("referential", "referential_contract");
}

#[tokio::test]
async fn persisted_referential_metadata_endpoints_return_non_empty_catalog_values() {
    let fixture = ReferentialContractFixture::new("referential-persisted-metadata").await;
    seed_referential_catalog(&fixture.paths.main_db, &fixture.library_root).await;

    let persisted = persisted_referential_values(&fixture.paths.main_db, LIBRARY_ID).await;
    assert_eq!(
        persisted.author_pairs,
        BTreeSet::from([(AUTHOR_NAME.to_string(), AUTHOR_ROLE.to_string())]),
        "fixture seeding must persist author metadata before the referential contract hits HTTP routes",
    );
    assert_eq!(
        persisted.genres,
        BTreeSet::from([GENRE.to_string()]),
        "fixture seeding must persist genre metadata before the referential contract hits HTTP routes",
    );
    assert_eq!(
        persisted.tags,
        BTreeSet::from([BOOK_TAG.to_string(), SERIES_TAG.to_string()]),
        "fixture seeding must persist both series and book tags so GET /api/v1/tags cannot legally answer with an empty placeholder list",
    );
    assert_eq!(
        persisted.languages,
        BTreeSet::from([LANGUAGE.to_string()]),
        "fixture seeding must persist language metadata before the referential contract hits HTTP routes",
    );
    assert_eq!(
        persisted.publishers,
        BTreeSet::from([PUBLISHER.to_string()]),
        "fixture seeding must persist publisher metadata before the referential contract hits HTTP routes",
    );

    let token = admin_session_token(&fixture.app).await;

    let authors = request_json(
        &fixture.app,
        "GET",
        &format!("/api/v1/authors?library_id={LIBRARY_ID}"),
        &token,
    )
    .await;
    let genres = request_json(
        &fixture.app,
        "GET",
        &format!("/api/v1/genres?library_id={LIBRARY_ID}"),
        &token,
    )
    .await;
    let tags = request_json(
        &fixture.app,
        "GET",
        &format!("/api/v1/tags?library_id={LIBRARY_ID}"),
        &token,
    )
    .await;
    let languages = request_json(
        &fixture.app,
        "GET",
        &format!("/api/v1/languages?library_id={LIBRARY_ID}"),
        &token,
    )
    .await;
    let publishers = request_json(
        &fixture.app,
        "GET",
        &format!("/api/v1/publishers?library_id={LIBRARY_ID}"),
        &token,
    )
    .await;

    assert_eq!(
        author_pairs(&authors),
        persisted.author_pairs,
        "GET /api/v1/authors must enumerate persisted author rows for the requested library instead of serving a snapshot or empty placeholder payload",
    );
    assert!(!value_array_is_empty(&authors));

    assert_eq!(
        string_set(&genres),
        persisted.genres,
        "GET /api/v1/genres must come from SERIES_METADATA_GENRE rows for the requested persisted library",
    );
    assert!(!value_array_is_empty(&genres));

    assert_eq!(
        string_set(&tags),
        persisted.tags,
        "GET /api/v1/tags must merge persisted series and book tags instead of returning an empty placeholder list",
    );
    assert!(!value_array_is_empty(&tags));

    assert_eq!(
        string_set(&languages),
        persisted.languages,
        "GET /api/v1/languages must surface persisted SERIES_METADATA.LANGUAGE values for the requested library",
    );
    assert!(!value_array_is_empty(&languages));

    assert_eq!(
        string_set(&publishers),
        persisted.publishers,
        "GET /api/v1/publishers must surface persisted SERIES_METADATA.PUBLISHER values for the requested library",
    );
    assert!(!value_array_is_empty(&publishers));

    fixture.cleanup();
}

#[tokio::test]
async fn book_tags_endpoint_rejects_empty_placeholder_payloads_when_persisted_rows_exist() {
    let fixture = ReferentialContractFixture::new("referential-book-tags").await;
    seed_referential_catalog(&fixture.paths.main_db, &fixture.library_root).await;

    let token = admin_session_token(&fixture.app).await;
    let book_tags = request_json(
        &fixture.app,
        "GET",
        &format!("/api/v1/tags/book?series_id={SERIES_ID}"),
        &token,
    )
    .await;

    assert_eq!(
        string_set(&book_tags),
        BTreeSet::from([BOOK_TAG.to_string()]),
        "GET /api/v1/tags/book must enumerate persisted BOOK_METADATA_TAG rows for the requested series instead of returning the current empty placeholder array",
    );
    assert!(
        !value_array_is_empty(&book_tags),
        "GET /api/v1/tags/book must not stay empty once persisted book tags exist",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn book_tags_endpoint_returns_persisted_rows_for_requested_library() {
    let fixture = ReferentialContractFixture::new("referential-book-tags-library").await;
    seed_referential_catalog(&fixture.paths.main_db, &fixture.library_root).await;

    let token = admin_session_token(&fixture.app).await;
    let book_tags = request_json(
        &fixture.app,
        "GET",
        &format!("/api/v1/tags/book?library_id={LIBRARY_ID}"),
        &token,
    )
    .await;

    assert_eq!(
        string_set(&book_tags),
        BTreeSet::from([BOOK_TAG.to_string()]),
        "GET /api/v1/tags/book must scope persisted BOOK_METADATA_TAG rows to the requested library instead of returning an empty or placeholder payload",
    );
    assert!(
        !value_array_is_empty(&book_tags),
        "GET /api/v1/tags/book must not stay empty for a library with persisted book tags",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn users_list_prefers_persisted_rows_over_configured_placeholder_users_when_db_users_exist() {
    let fixture = ReferentialContractFixture::new("referential-users-list").await;
    seed_referential_catalog(&fixture.paths.main_db, &fixture.library_root).await;
    let token = admin_session_token(&fixture.app).await;
    seed_users(&fixture.paths.main_db).await;

    let persisted_users = persisted_user_rows(&fixture.paths.main_db).await;
    assert_eq!(
        persisted_users,
        vec![
            PersistedUserRow {
                id: CATALOG_ADMIN_USER_ID.to_string(),
                email: CATALOG_ADMIN_EMAIL.to_string(),
                roles: BTreeSet::from(["ADMIN".to_string(), "USER".to_string()]),
                shared_all_libraries: true,
                shared_library_ids: BTreeSet::new(),
                labels_allow: BTreeSet::new(),
                labels_exclude: BTreeSet::new(),
                age_restriction: None,
            },
            PersistedUserRow {
                id: CATALOG_USER_ID.to_string(),
                email: CATALOG_USER_EMAIL.to_string(),
                roles: BTreeSet::from(["USER".to_string()]),
                shared_all_libraries: false,
                shared_library_ids: BTreeSet::from([LIBRARY_ID.to_string()]),
                labels_allow: BTreeSet::from([LABEL_ALLOW.to_string()]),
                labels_exclude: BTreeSet::from([LABEL_EXCLUDE.to_string()]),
                age_restriction: Some(PersistedAgeRestriction {
                    age: USER_AGE_RESTRICTION,
                    restriction: "EXCLUDE".to_string(),
                }),
            },
        ],
        "fixture seeding must write USER and USER_ROLE rows before the residual user metadata contract hits HTTP routes",
    );

    let users = request_json(&fixture.app, "GET", "/api/v2/users", &token).await;

    assert_eq!(
        user_rows(&users),
        persisted_users,
        "GET /api/v2/users must prefer persisted USER metadata, sharing rows, and restrictions over configured placeholder auth users once catalog-compatible user rows exist",
    );
    assert!(
        !string_set_by_key(&users, "email").contains("admin@example.org"),
        "configured placeholder auth users must disappear from GET /api/v2/users once real persisted rows exist",
    );
    assert_eq!(
        json_user_by_email(&users, CATALOG_USER_EMAIL),
        Some(PersistedUserRow {
            id: CATALOG_USER_ID.to_string(),
            email: CATALOG_USER_EMAIL.to_string(),
            roles: BTreeSet::from(["USER".to_string()]),
            shared_all_libraries: false,
            shared_library_ids: BTreeSet::from([LIBRARY_ID.to_string()]),
            labels_allow: BTreeSet::from([LABEL_ALLOW.to_string()]),
            labels_exclude: BTreeSet::from([LABEL_EXCLUDE.to_string()]),
            age_restriction: Some(PersistedAgeRestriction {
                age: USER_AGE_RESTRICTION,
                restriction: "EXCLUDE".to_string(),
            }),
        }),
        "GET /api/v2/users must surface persisted sharedLibrariesIds, labels, and ageRestriction values instead of leaking placeholder defaults for catalog users",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn users_me_returns_persisted_current_user_metadata_instead_of_placeholder_defaults() {
    let fixture = ReferentialContractFixture::new("referential-users-me").await;
    seed_referential_catalog(&fixture.paths.main_db, &fixture.library_root).await;
    seed_users(&fixture.paths.main_db).await;

    let persisted_catalog_user = persisted_user_rows(&fixture.paths.main_db)
        .await
        .into_iter()
        .find(|row| row.email == CATALOG_USER_EMAIL)
        .expect("fixture seeding must persist the catalog user before GET /api/v2/users/me runs");

    let current_user = request_json_with_basic_auth(
        &fixture.app,
        "GET",
        "/api/v2/users/me",
        CATALOG_USER_EMAIL,
        CATALOG_USER_PASSWORD,
    )
    .await;

    assert_eq!(
        user_row(&current_user),
        persisted_catalog_user,
        "GET /api/v2/users/me must return persisted current-user metadata from USER, USER_LIBRARY_SHARING, and USER_SHARING rows instead of falling back to placeholder defaults",
    );
    assert_ne!(
        current_user["email"],
        Value::String("admin@example.org".to_string()),
        "configured placeholder admin identity must not leak into GET /api/v2/users/me once a persisted catalog user authenticates",
    );

    fixture.cleanup();
}

struct ReferentialContractFixture {
    paths: persistence_contract_fixture::LegacyDbPaths,
    app: axum::Router,
    library_root: PathBuf,
}

impl ReferentialContractFixture {
    async fn new(case_id: &str) -> Self {
        compat_auth_env::ensure_compat_auth_env();

        let paths = persistence_contract_fixture::new_legacy_db_paths(case_id)
            .expect("referential contract db paths should be created");
        persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
            .await
            .expect("main db flyway fixture should be created");
        persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
            .await
            .expect("tasks db flyway fixture should be created");

        let library_root = paths.config_dir.join("referential-library-root");
        fs::create_dir_all(&library_root)
            .expect("referential library root fixture directory should be created");
        fs::create_dir_all(paths.config_dir.join("lucene"))
            .expect("lucene directory should be created for referential contract fixture");
        fs::create_dir_all(paths.config_dir.join("fonts"))
            .expect("fonts directory should be created for referential contract fixture");

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

#[derive(Debug, Clone, Eq, PartialEq)]
struct PersistedReferentialValues {
    author_pairs: BTreeSet<(String, String)>,
    genres: BTreeSet<String>,
    tags: BTreeSet<String>,
    languages: BTreeSet<String>,
    publishers: BTreeSet<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct PersistedUserRow {
    id: String,
    email: String,
    roles: BTreeSet<String>,
    shared_all_libraries: bool,
    shared_library_ids: BTreeSet<String>,
    labels_allow: BTreeSet<String>,
    labels_exclude: BTreeSet<String>,
    age_restriction: Option<PersistedAgeRestriction>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct PersistedAgeRestriction {
    age: i64,
    restriction: String,
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
                        compat_auth_env::COMPAT_ADMIN_BASIC_AUTH_BASE64,
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

async fn request_json(app: &axum::Router, method: &str, path: &str, token: &str) -> Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("X-Auth-Token", token)
                .header(SEARCH_OWNERSHIP_HEADER, NATIVE_OWNERSHIP_MARKER)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "unexpected status for {method} {path}",
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn request_json_with_basic_auth(
    app: &axum::Router,
    method: &str,
    path: &str,
    email: &str,
    password: &str,
) -> Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header(
                    header::AUTHORIZATION,
                    format!("Basic {}", STANDARD.encode(format!("{email}:{password}"))),
                )
                .header("X-Auth-Token", "")
                .header(SEARCH_OWNERSHIP_HEADER, NATIVE_OWNERSHIP_MARKER)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "unexpected status for {method} {path} with persisted basic auth",
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn seed_referential_catalog(main_db: &Path, library_root: &Path) {
    let media_file_path = library_root.join("referential.cbz");
    fs::write(&media_file_path, b"referential-contract-media")
        .expect("referential media fixture file should be written");

    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for referential fixture seeding");

    sqlx::query(
        "INSERT INTO LIBRARY (ID, NAME, ROOT, SCAN_STARTUP, EMPTY_TRASH_AFTER_SCAN, ONESHOTS_DIRECTORY) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(LIBRARY_ID)
    .bind("Referential Library")
    .bind(library_root.to_string_lossy().to_string())
    .bind(false)
    .bind(false)
    .bind(None::<String>)
    .execute(&pool)
    .await
    .expect("referential library fixture row should insert");

    sqlx::query(
        "INSERT INTO SERIES (ID, CREATED_DATE, LAST_MODIFIED_DATE, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, ONESHOT, DELETED_DATE) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(SERIES_ID)
    .bind("2024-01-01T00:00:00")
    .bind("2024-01-02T00:00:00")
    .bind("2024-01-02T00:00:00")
    .bind("Referential Series")
    .bind(format!("/library/{LIBRARY_ID}/series/{SERIES_ID}"))
    .bind(LIBRARY_ID)
    .bind(false)
    .bind(None::<String>)
    .execute(&pool)
    .await
    .expect("referential series fixture row should insert");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (CREATED_DATE, LAST_MODIFIED_DATE, STATUS, TITLE, TITLE_SORT, SUMMARY, LANGUAGE, PUBLISHER, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("2024-01-01T00:00:00")
    .bind("2024-01-02T00:00:00")
    .bind("ONGOING")
    .bind("Referential Series")
    .bind("Referential Series")
    .bind("referential summary")
    .bind(LANGUAGE)
    .bind(PUBLISHER)
    .bind(SERIES_ID)
    .execute(&pool)
    .await
    .expect("referential series metadata fixture row should insert");

    sqlx::query("INSERT INTO SERIES_METADATA_GENRE (GENRE, SERIES_ID) VALUES (?, ?)")
        .bind(GENRE)
        .bind(SERIES_ID)
        .execute(&pool)
        .await
        .expect("referential series genre fixture row should insert");

    sqlx::query("INSERT INTO SERIES_METADATA_TAG (TAG, SERIES_ID) VALUES (?, ?)")
        .bind(SERIES_TAG)
        .bind(SERIES_ID)
        .execute(&pool)
        .await
        .expect("referential series tag fixture row should insert");

    sqlx::query(
        "INSERT INTO BOOK (ID, CREATED_DATE, LAST_MODIFIED_DATE, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, ONESHOT, DELETED_DATE) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(BOOK_ID)
    .bind("2024-01-01T00:00:00")
    .bind("2024-01-02T00:00:00")
    .bind("2024-01-02T00:00:00")
    .bind("referential.cbz")
    .bind(format!("/library/{LIBRARY_ID}/books/{BOOK_ID}"))
    .bind(SERIES_ID)
    .bind(22_i64)
    .bind(1_i32)
    .bind(LIBRARY_ID)
    .bind(false)
    .bind(None::<String>)
    .execute(&pool)
    .await
    .expect("referential book fixture row should insert");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (CREATED_DATE, LAST_MODIFIED_DATE, NUMBER, NUMBER_SORT, TITLE, SUMMARY, BOOK_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("2024-01-01T00:00:00")
    .bind("2024-01-02T00:00:00")
    .bind("1")
    .bind(1.0_f64)
    .bind("Referential Book")
    .bind("referential book summary")
    .bind(BOOK_ID)
    .execute(&pool)
    .await
    .expect("referential book metadata fixture row should insert");

    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (NAME, ROLE, BOOK_ID) VALUES (?, ?, ?)")
        .bind(AUTHOR_NAME)
        .bind(AUTHOR_ROLE)
        .bind(BOOK_ID)
        .execute(&pool)
        .await
        .expect("referential book author fixture row should insert");

    sqlx::query("INSERT INTO BOOK_METADATA_TAG (TAG, BOOK_ID) VALUES (?, ?)")
        .bind(BOOK_TAG)
        .bind(BOOK_ID)
        .execute(&pool)
        .await
        .expect("referential book tag fixture row should insert");

    sqlx::query("INSERT INTO BOOK_METADATA_AGGREGATION (SERIES_ID) VALUES (?)")
        .bind(SERIES_ID)
        .execute(&pool)
        .await
        .expect("referential book metadata aggregation fixture row should insert");

    sqlx::query("INSERT INTO BOOK_METADATA_AGGREGATION_AUTHOR (NAME, ROLE, SERIES_ID) VALUES (?, ?, ?)")
        .bind(AUTHOR_NAME)
        .bind(AUTHOR_ROLE)
        .bind(SERIES_ID)
        .execute(&pool)
        .await
        .expect("referential book metadata aggregation author fixture row should insert");

    sqlx::query("INSERT INTO BOOK_METADATA_AGGREGATION_TAG (TAG, SERIES_ID) VALUES (?, ?)")
        .bind(BOOK_TAG)
        .bind(SERIES_ID)
        .execute(&pool)
        .await
        .expect("referential book metadata aggregation tag fixture row should insert");

    sqlx::query("INSERT INTO MEDIA (STATUS, MEDIA_TYPE, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("READY")
        .bind("application/vnd.comicbook+zip")
        .bind(BOOK_ID)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("referential media fixture row should insert");

    pool.close().await;
}

async fn persisted_referential_values(main_db: &Path, library_id: &str) -> PersistedReferentialValues {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for referential fixture inspection");

    let author_pairs = sqlx::query(
        "SELECT a.NAME, a.ROLE
         FROM BOOK_METADATA_AUTHOR a
         JOIN BOOK b ON b.ID = a.BOOK_ID
         WHERE b.LIBRARY_ID = ?
         ORDER BY lower(a.NAME), lower(a.ROLE)",
    )
    .bind(library_id)
    .fetch_all(&pool)
    .await
    .expect("referential author rows should be queryable")
    .into_iter()
    .map(|row| {
        (
            row.get::<String, _>("NAME"),
            row.get::<String, _>("ROLE"),
        )
    })
    .collect();

    let genres = sqlx::query(
        "SELECT g.GENRE
         FROM SERIES_METADATA_GENRE g
         JOIN SERIES s ON s.ID = g.SERIES_ID
         WHERE s.LIBRARY_ID = ?
         ORDER BY lower(g.GENRE)",
    )
    .bind(library_id)
    .fetch_all(&pool)
    .await
    .expect("referential genre rows should be queryable")
    .into_iter()
    .map(|row| row.get::<String, _>("GENRE"))
    .collect();

    let tags = sqlx::query(
        "SELECT TAG FROM (
             SELECT st.TAG AS TAG
             FROM SERIES_METADATA_TAG st
             JOIN SERIES s ON s.ID = st.SERIES_ID
             WHERE s.LIBRARY_ID = ?
             UNION
             SELECT bt.TAG AS TAG
             FROM BOOK_METADATA_TAG bt
             JOIN BOOK b ON b.ID = bt.BOOK_ID
             WHERE b.LIBRARY_ID = ?
         )
         ORDER BY lower(TAG)",
    )
    .bind(library_id)
    .bind(library_id)
    .fetch_all(&pool)
    .await
    .expect("referential tag rows should be queryable")
    .into_iter()
    .map(|row| row.get::<String, _>("TAG"))
    .collect();

    let languages = sqlx::query(
        "SELECT DISTINCT sm.LANGUAGE
         FROM SERIES_METADATA sm
         JOIN SERIES s ON s.ID = sm.SERIES_ID
         WHERE s.LIBRARY_ID = ?
         ORDER BY lower(sm.LANGUAGE)",
    )
    .bind(library_id)
    .fetch_all(&pool)
    .await
    .expect("referential language rows should be queryable")
    .into_iter()
    .map(|row| row.get::<String, _>("LANGUAGE"))
    .collect();

    let publishers = sqlx::query(
        "SELECT DISTINCT sm.PUBLISHER
         FROM SERIES_METADATA sm
         JOIN SERIES s ON s.ID = sm.SERIES_ID
         WHERE s.LIBRARY_ID = ?
         ORDER BY lower(sm.PUBLISHER)",
    )
    .bind(library_id)
    .fetch_all(&pool)
    .await
    .expect("referential publisher rows should be queryable")
    .into_iter()
    .map(|row| row.get::<String, _>("PUBLISHER"))
    .collect();

    pool.close().await;

    PersistedReferentialValues {
        author_pairs,
        genres,
        tags,
        languages,
        publishers,
    }
}

async fn seed_users(main_db: &Path) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for residual user fixture seeding");

    for (id, email, password_hash, shared_all_libraries, age_restriction, age_restriction_allow_only) in [
        (
            CATALOG_ADMIN_USER_ID,
            CATALOG_ADMIN_EMAIL,
            CATALOG_ADMIN_PASSWORD_BCRYPT,
            true,
            None,
            None,
        ),
        (
            CATALOG_USER_ID,
            CATALOG_USER_EMAIL,
            CATALOG_USER_PASSWORD_BCRYPT,
            false,
            Some(USER_AGE_RESTRICTION),
            Some(false),
        ),
    ] {
        sqlx::query(
            "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, AGE_RESTRICTION_ALLOW_ONLY) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(email)
        .bind(password_hash)
        .bind(shared_all_libraries)
        .bind(age_restriction)
        .bind(age_restriction_allow_only)
        .execute(&pool)
        .await
        .expect("referential residual user fixture row should insert");
    }

    for (user_id, role) in [
        (CATALOG_ADMIN_USER_ID, "ADMIN"),
        (CATALOG_ADMIN_USER_ID, "USER"),
        (CATALOG_USER_ID, "USER"),
    ] {
        sqlx::query("INSERT INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
            .bind(user_id)
            .bind(role)
            .execute(&pool)
            .await
            .expect("referential residual user role fixture row should insert");
    }

    sqlx::query("INSERT INTO USER_LIBRARY_SHARING (USER_ID, LIBRARY_ID) VALUES (?, ?)")
        .bind(CATALOG_USER_ID)
        .bind(LIBRARY_ID)
        .execute(&pool)
        .await
        .expect("referential residual user library sharing row should insert");

    for (allow, label) in [(true, LABEL_ALLOW), (false, LABEL_EXCLUDE)] {
        sqlx::query("INSERT INTO USER_SHARING (LABEL, ALLOW, USER_ID) VALUES (?, ?, ?)")
            .bind(label)
            .bind(allow)
            .bind(CATALOG_USER_ID)
            .execute(&pool)
            .await
            .expect("referential residual user sharing restriction row should insert");
    }

    pool.close().await;
}

async fn persisted_user_rows(main_db: &Path) -> Vec<PersistedUserRow> {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for residual user fixture inspection");

    let user_rows = sqlx::query(
        "SELECT ID, EMAIL, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, AGE_RESTRICTION_ALLOW_ONLY FROM USER WHERE EMAIL LIKE 'catalog-%' ORDER BY EMAIL",
    )
    .fetch_all(&pool)
    .await
    .expect("persisted user rows should be queryable");

    let mut persisted = Vec::with_capacity(user_rows.len());
    for row in user_rows {
        let user_id = row.get::<String, _>("ID");
        let roles = sqlx::query("SELECT ROLE FROM USER_ROLE WHERE USER_ID = ? ORDER BY ROLE")
            .bind(&user_id)
            .fetch_all(&pool)
            .await
            .expect("persisted user roles should be queryable")
            .into_iter()
            .map(|role_row| role_row.get::<String, _>("ROLE"))
            .collect();

        let shared_library_ids = sqlx::query(
            "SELECT LIBRARY_ID FROM USER_LIBRARY_SHARING WHERE USER_ID = ? ORDER BY LIBRARY_ID",
        )
        .bind(&user_id)
        .fetch_all(&pool)
        .await
        .expect("persisted user shared library rows should be queryable")
        .into_iter()
        .map(|sharing_row| sharing_row.get::<String, _>("LIBRARY_ID"))
        .collect();

        let sharing_rows = sqlx::query(
            "SELECT LABEL, ALLOW FROM USER_SHARING WHERE USER_ID = ? ORDER BY ALLOW DESC, LABEL",
        )
        .bind(&user_id)
        .fetch_all(&pool)
        .await
        .expect("persisted user sharing rows should be queryable");

        let labels_allow = sharing_rows
            .iter()
            .filter(|sharing_row| sharing_row.get::<bool, _>("ALLOW"))
            .map(|sharing_row| sharing_row.get::<String, _>("LABEL"))
            .collect();

        let labels_exclude = sharing_rows
            .iter()
            .filter(|sharing_row| !sharing_row.get::<bool, _>("ALLOW"))
            .map(|sharing_row| sharing_row.get::<String, _>("LABEL"))
            .collect();

        let age = row.get::<Option<i64>, _>("AGE_RESTRICTION");
        let allow_only = row.get::<Option<bool>, _>("AGE_RESTRICTION_ALLOW_ONLY");

        persisted.push(PersistedUserRow {
            id: user_id,
            email: row.get::<String, _>("EMAIL"),
            roles,
            shared_all_libraries: row.get::<bool, _>("SHARED_ALL_LIBRARIES"),
            shared_library_ids,
            labels_allow,
            labels_exclude,
            age_restriction: match (age, allow_only) {
                (Some(age), Some(true)) => Some(PersistedAgeRestriction {
                    age,
                    restriction: "ALLOW_ONLY".to_string(),
                }),
                (Some(age), Some(false)) => Some(PersistedAgeRestriction {
                    age,
                    restriction: "EXCLUDE".to_string(),
                }),
                _ => None,
            },
        });
    }

    pool.close().await;
    persisted
}

fn author_pairs(value: &Value) -> BTreeSet<(String, String)> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| {
            (
                entry["name"]
                    .as_str()
                    .expect("author payload should include name")
                    .to_string(),
                entry["role"]
                    .as_str()
                    .expect("author payload should include role")
                    .to_string(),
            )
        })
        .collect()
}

fn user_rows(value: &Value) -> Vec<PersistedUserRow> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(user_payload_row)
        .collect()
}

fn user_row(value: &Value) -> PersistedUserRow {
    user_payload_row(value).expect("user payload should contain a catalog user row")
}

fn json_user_by_email(value: &Value, email: &str) -> Option<PersistedUserRow> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|entry| match user_payload_row(entry) {
            Some(row) if row.email == email => Some(row),
            _ => None,
        })
}

fn user_payload_row(entry: &Value) -> Option<PersistedUserRow> {
    let email = entry["email"].as_str()?.to_string();
    if !email.starts_with("catalog-") {
        return None;
    }

    Some(PersistedUserRow {
        id: entry["id"]
            .as_str()
            .expect("users payload should include id")
            .to_string(),
        email,
        roles: entry["roles"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|role| {
                role.as_str()
                    .expect("users payload role should be a string")
                    .to_string()
            })
            .collect(),
        shared_all_libraries: entry["sharedAllLibraries"]
            .as_bool()
            .expect("users payload should include sharedAllLibraries"),
        shared_library_ids: entry["sharedLibrariesIds"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|library_id| {
                library_id
                    .as_str()
                    .expect("users payload sharedLibrariesIds should be strings")
                    .to_string()
            })
            .collect(),
        labels_allow: entry["labelsAllow"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|label| {
                label
                    .as_str()
                    .expect("users payload labelsAllow should be strings")
                    .to_string()
            })
            .collect(),
        labels_exclude: entry["labelsExclude"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|label| {
                label
                    .as_str()
                    .expect("users payload labelsExclude should be strings")
                    .to_string()
            })
            .collect(),
        age_restriction: age_restriction_from_json(&entry["ageRestriction"]),
    })
}

fn age_restriction_from_json(value: &Value) -> Option<PersistedAgeRestriction> {
    if value.is_null() {
        return None;
    }

    Some(PersistedAgeRestriction {
        age: value["age"]
            .as_i64()
            .expect("ageRestriction.age should be a number"),
        restriction: value["restriction"]
            .as_str()
            .expect("ageRestriction.restriction should be a string")
            .to_string(),
    })
}

fn string_set(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| {
            entry
                .as_str()
                .expect("referential payload should be an array of strings")
                .to_string()
        })
        .collect()
}

fn string_set_by_key(value: &Value, key: &str) -> BTreeSet<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry[key].as_str().map(str::to_string))
        .collect()
}

fn value_array_is_empty(value: &Value) -> bool {
    value
        .as_array()
        .expect("referential payload should be a JSON array")
        .is_empty()
}
