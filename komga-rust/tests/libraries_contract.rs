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

const SEARCH_OWNERSHIP_HEADER: &str = "x-komga-compat-search-ownership";
const NATIVE_OWNERSHIP_MARKER: &str = "native-rust-owned";

#[test]
fn libraries_contract_target_is_registered() {
    assert_required_target_declared("libraries", "libraries_contract");
}

#[tokio::test]
async fn libraries_listing_reflects_persisted_library_rows_in_name_sorted_order() {
    let fixture = LibrariesContractFixture::new("libraries-list-detail").await;

    seed_library(
        &fixture.paths.main_db,
        SeedLibraryRow {
            id: "library-zeta",
            name: "zeta shelf",
            root: &fixture.library_zeta_root,
            scan_directory_exclusions: &["#recycle"],
            empty_trash_after_scan: false,
            scan_on_startup: false,
            oneshots_directory: None,
        },
    )
    .await;
    seed_library(
        &fixture.paths.main_db,
        SeedLibraryRow {
            id: "library-alpha",
            name: "Alpha Shelf",
            root: &fixture.library_alpha_root,
            scan_directory_exclusions: &["cache", "tmp"],
            empty_trash_after_scan: true,
            scan_on_startup: true,
            oneshots_directory: Some("oneshots"),
        },
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let libraries =
        request_json(&fixture.app, "GET", "/api/v1/libraries", &token, None, true).await;

    assert_eq!(
        libraries.as_array().map(Vec::len),
        Some(2),
        "list contract requires every persisted library row to be visible through GET /api/v1/libraries",
    );
    assert_eq!(
        library_names(&libraries),
        vec!["Alpha Shelf".to_string(), "zeta shelf".to_string()],
        "Kotlin libraries list sorts by lowercase name, so persisted rows must come back in case-insensitive name order",
    );
    assert_eq!(
        libraries[0]["root"],
        Value::String(fixture.library_alpha_root.to_string_lossy().to_string()),
        "admin list payload must expose persisted root for the first sorted library",
    );
    assert_eq!(
        string_vec(&libraries[0]["scanDirectoryExclusions"]),
        vec!["cache".to_string(), "tmp".to_string()],
        "list payload must read persisted exclusion rows instead of returning snapshot defaults",
    );
    fixture.cleanup();
}

#[tokio::test]
async fn ordinary_libraries_browse_prefers_persisted_rows_without_owned_marker() {
    let fixture = LibrariesContractFixture::new("libraries-list-ordinary-persisted").await;

    seed_library(
        &fixture.paths.main_db,
        SeedLibraryRow {
            id: "library-ordinary",
            name: "Ordinary Persisted Library",
            root: &fixture.library_alpha_root,
            scan_directory_exclusions: &["persisted-only"],
            empty_trash_after_scan: false,
            scan_on_startup: false,
            oneshots_directory: None,
        },
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let libraries = request_json(
        &fixture.app,
        "GET",
        "/api/v1/libraries",
        &token,
        None,
        false,
    )
    .await;

    assert!(
        libraries.as_array().into_iter().flatten().any(
            |library| library.get("id") == Some(&Value::String("library-ordinary".to_string()))
        ),
        "ordinary GET /api/v1/libraries should expose persisted LIBRARY rows even without owned-route marker",
    );
    assert!(
        libraries.as_array().into_iter().flatten().any(|library| {
            library.get("id") == Some(&Value::String("library-ordinary".to_string()))
                && library
                    .get("scanDirectoryExclusions")
                    .is_some_and(|value| value == &json!(["persisted-only"]))
        }),
        "ordinary libraries browse must preserve persisted exclusions instead of snapshot fallback payloads",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn library_detail_reflects_persisted_row_for_requested_id() {
    let fixture = LibrariesContractFixture::new("libraries-detail").await;

    seed_library(
        &fixture.paths.main_db,
        SeedLibraryRow {
            id: "library-alpha",
            name: "Alpha Shelf",
            root: &fixture.library_alpha_root,
            scan_directory_exclusions: &["cache", "tmp"],
            empty_trash_after_scan: true,
            scan_on_startup: true,
            oneshots_directory: Some("oneshots"),
        },
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let detail = request_json(
        &fixture.app,
        "GET",
        "/api/v1/libraries/library-alpha",
        &token,
        None,
        true,
    )
    .await;

    assert_eq!(detail["id"], "library-alpha");
    assert_eq!(detail["name"], "Alpha Shelf");
    assert_eq!(
        detail["root"],
        Value::String(fixture.library_alpha_root.to_string_lossy().to_string())
    );
    assert_eq!(detail["scanOnStartup"], Value::Bool(true));
    assert_eq!(detail["emptyTrashAfterScan"], Value::Bool(true));
    assert_eq!(
        detail["oneshotsDirectory"],
        Value::String("oneshots".to_string())
    );
    assert_eq!(
        string_vec(&detail["scanDirectoryExclusions"]),
        vec!["cache".to_string(), "tmp".to_string()],
        "detail payload must round-trip persisted exclusions and reject hardcoded library detail snapshots",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn library_create_round_trips_through_follow_up_reads_instead_of_request_echo() {
    let fixture = LibrariesContractFixture::new("libraries-create").await;
    let token = admin_session_token(&fixture.app).await;

    let create_body = json!({
        "name": "Created Library",
        "root": fixture.created_root.to_string_lossy().to_string(),
        "scanDirectoryExclusions": ["cache", "tmp"],
        "scanOnStartup": true,
        "emptyTrashAfterScan": true,
        "hashPages": true,
        "oneshotsDirectory": "oneshots",
    });

    let create_response = request(
        &fixture.app,
        "POST",
        "/api/v1/libraries",
        &token,
        Some(create_body.clone()),
        false,
    )
    .await;
    assert_eq!(
        create_response.status(),
        StatusCode::OK,
        "POST /api/v1/libraries must create a persisted library and return the created DTO",
    );
    let created = response_json(create_response).await;

    let created_id = created["id"]
        .as_str()
        .expect("created library response should include an id")
        .to_string();
    assert_eq!(created["name"], create_body["name"]);
    assert_eq!(created["scanOnStartup"], Value::Bool(true));
    assert_eq!(created["emptyTrashAfterScan"], Value::Bool(true));

    let persisted_detail = request_json(
        &fixture.app,
        "GET",
        &format!("/api/v1/libraries/{created_id}"),
        &token,
        None,
        true,
    )
    .await;
    let persisted_list =
        request_json(&fixture.app, "GET", "/api/v1/libraries", &token, None, true).await;

    assert_eq!(persisted_detail["id"], Value::String(created_id.clone()));
    assert_eq!(persisted_detail["name"], create_body["name"]);
    assert_eq!(persisted_detail["root"], create_body["root"]);
    assert_eq!(
        persisted_detail["scanDirectoryExclusions"], create_body["scanDirectoryExclusions"],
        "create contract requires a follow-up read to match persisted state rather than just echoing the request body",
    );
    assert!(
        persisted_list
            .as_array()
            .into_iter()
            .flatten()
            .any(|library| library.get("id") == Some(&Value::String(created_id.clone()))),
        "created library must appear in a fresh GET /api/v1/libraries response after the write completes",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn rejects_fixed_id_update() {
    let fixture = LibrariesContractFixture::new("libraries-update").await;

    seed_library(
        &fixture.paths.main_db,
        SeedLibraryRow {
            id: "library-update-target",
            name: "Original Library",
            root: &fixture.update_original_root,
            scan_directory_exclusions: &["before"],
            empty_trash_after_scan: false,
            scan_on_startup: false,
            oneshots_directory: None,
        },
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let patch_response = request(
        &fixture.app,
        "PATCH",
        "/api/v1/libraries/library-update-target",
        &token,
        Some(json!({
            "name": "Updated Library",
            "root": fixture.update_replacement_root.to_string_lossy().to_string(),
            "scanDirectoryExclusions": ["updated"],
            "emptyTrashAfterScan": true,
            "oneshotsDirectory": "single-issues",
        })),
        false,
    )
    .await;
    assert_eq!(
        patch_response.status(),
        StatusCode::NO_CONTENT,
        "PATCH /api/v1/libraries/{{id}} must update arbitrary persisted library ids, not just a hardcoded fixture id",
    );

    let persisted_detail = request_json(
        &fixture.app,
        "GET",
        "/api/v1/libraries/library-update-target",
        &token,
        None,
        true,
    )
    .await;

    assert_eq!(persisted_detail["name"], "Updated Library");
    assert_eq!(
        persisted_detail["root"],
        Value::String(
            fixture
                .update_replacement_root
                .to_string_lossy()
                .to_string()
        ),
        "follow-up GET must expose the persisted replacement root rather than a stale placeholder payload",
    );
    assert_eq!(
        persisted_detail["scanDirectoryExclusions"],
        json!(["updated"]),
        "patch contract requires persisted exclusions to replace prior exclusions on follow-up read",
    );
    assert_eq!(persisted_detail["emptyTrashAfterScan"], Value::Bool(true));
    assert_eq!(
        persisted_detail["oneshotsDirectory"],
        Value::String("single-issues".to_string())
    );

    fixture.cleanup();
}

#[tokio::test]
async fn library_delete_removes_persisted_row_and_follow_up_reads_return_not_found() {
    let fixture = LibrariesContractFixture::new("libraries-delete").await;

    seed_library(
        &fixture.paths.main_db,
        SeedLibraryRow {
            id: "1",
            name: "Delete Me",
            root: &fixture.delete_root,
            scan_directory_exclusions: &["trash"],
            empty_trash_after_scan: false,
            scan_on_startup: false,
            oneshots_directory: None,
        },
    )
    .await;
    assert_eq!(
        library_row_count(&fixture.paths.main_db, "1").await,
        1,
        "delete fixture must start with a persisted library row before the delete request is issued",
    );

    let token = admin_session_token(&fixture.app).await;
    let delete_response = request(
        &fixture.app,
        "DELETE",
        "/api/v1/libraries/1",
        &token,
        None,
        false,
    )
    .await;
    assert_eq!(
        delete_response.status(),
        StatusCode::NO_CONTENT,
        "DELETE /api/v1/libraries/{{id}} must remove the persisted library row",
    );

    assert_eq!(
        library_row_count(&fixture.paths.main_db, "1").await,
        0,
        "delete contract requires the LIBRARY row to be removed from the Kotlin-compatible table",
    );

    let detail_after_delete = request(
        &fixture.app,
        "GET",
        "/api/v1/libraries/1",
        &token,
        None,
        true,
    )
    .await;
    assert_eq!(
        detail_after_delete.status(),
        StatusCode::NOT_FOUND,
        "follow-up GET must stop resolving deleted libraries; returning a stale snapshot would violate delete parity",
    );

    fixture.cleanup();
}

struct LibrariesContractFixture {
    paths: persistence_contract_fixture::LegacyDbPaths,
    app: axum::Router,
    library_alpha_root: PathBuf,
    library_zeta_root: PathBuf,
    created_root: PathBuf,
    update_original_root: PathBuf,
    update_replacement_root: PathBuf,
    delete_root: PathBuf,
}

impl LibrariesContractFixture {
    async fn new(case_id: &str) -> Self {
        compat_auth_env::ensure_compat_auth_env();

        let paths = persistence_contract_fixture::new_legacy_db_paths(case_id)
            .expect("libraries contract db paths should be created");
        persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
            .await
            .expect("main db flyway fixture should be created");
        persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
            .await
            .expect("tasks db flyway fixture should be created");

        let library_alpha_root = create_library_root(&paths.config_dir, "library-alpha-root");
        let library_zeta_root = create_library_root(&paths.config_dir, "library-zeta-root");
        let created_root = create_library_root(&paths.config_dir, "library-created-root");
        let update_original_root =
            create_library_root(&paths.config_dir, "library-update-original");
        let update_replacement_root =
            create_library_root(&paths.config_dir, "library-update-replacement");
        let delete_root = create_library_root(&paths.config_dir, "library-delete-root");

        fs::create_dir_all(paths.config_dir.join("lucene"))
            .expect("lucene directory should be created for libraries contract fixture");
        fs::create_dir_all(paths.config_dir.join("fonts"))
            .expect("fonts directory should be created for libraries contract fixture");

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
            library_alpha_root,
            library_zeta_root,
            created_root,
            update_original_root,
            update_replacement_root,
            delete_root,
        }
    }

    fn cleanup(self) {
        persistence_contract_fixture::cleanup(self.paths);
    }
}

struct SeedLibraryRow<'a> {
    id: &'a str,
    name: &'a str,
    root: &'a Path,
    scan_directory_exclusions: &'a [&'a str],
    empty_trash_after_scan: bool,
    scan_on_startup: bool,
    oneshots_directory: Option<&'a str>,
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
                    format!("Basic {}", compat_auth_env::COMPAT_ADMIN_BASIC_AUTH_BASE64),
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
    use_native_reads: bool,
) -> Value {
    let response = request(app, method, path, token, body, use_native_reads).await;
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
    use_native_reads: bool,
) -> axum::response::Response {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("X-Auth-Token", token);

    if use_native_reads {
        builder = builder.header(SEARCH_OWNERSHIP_HEADER, NATIVE_OWNERSHIP_MARKER);
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

async fn seed_library(main_db: &Path, library: SeedLibraryRow<'_>) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for library fixture seeding");

    sqlx::query(
        "INSERT INTO LIBRARY (ID, NAME, ROOT, SCAN_STARTUP, EMPTY_TRASH_AFTER_SCAN, ONESHOTS_DIRECTORY) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(library.id)
    .bind(library.name)
    .bind(library.root.to_string_lossy().to_string())
    .bind(library.scan_on_startup)
    .bind(library.empty_trash_after_scan)
    .bind(library.oneshots_directory)
    .execute(&pool)
    .await
    .expect("library fixture row should insert with Kotlin-compatible columns");

    for exclusion in library.scan_directory_exclusions {
        sqlx::query("INSERT INTO LIBRARY_EXCLUSIONS (LIBRARY_ID, EXCLUSION) VALUES (?, ?)")
            .bind(library.id)
            .bind(exclusion)
            .execute(&pool)
            .await
            .expect("library exclusion fixture row should insert");
    }

    pool.close().await;
}

async fn library_row_count(main_db: &Path, library_id: &str) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for library count inspection");
    let count = sqlx::query("SELECT COUNT(*) AS COUNT FROM LIBRARY WHERE ID = ?")
        .bind(library_id)
        .fetch_one(&pool)
        .await
        .expect("library row count should be queryable")
        .get::<i64, _>("COUNT");
    pool.close().await;
    count
}

fn library_names(payload: &Value) -> Vec<String> {
    payload
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|library| library.get("name").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

fn string_vec(payload: &Value) -> Vec<String> {
    let mut values = payload
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    values.sort();
    values
}
