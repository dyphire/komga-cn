use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_infrastructure::sqlite::connect_pool;
use komga_server::app::build_router_with_config;
use serde_json::Value;
use tower::util::ServiceExt;

mod support;

use support::runtime_router_contract_support::{
    RuntimeDbPaths, contract_seed::*, fixture_bootstrap::*, media_file_fixtures::*,
    metadata_series_seeding::*, response_helpers::*, user_auth::*,
};

mod opds_contract_cases;

#[test]
fn opds_contract_target_is_registered() {
    assert_required_target_declared("OPDS", "opds_contract");
}

async fn response_text(response: axum::response::Response) -> String {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    String::from_utf8(body.to_vec()).expect("response body should be valid utf-8")
}

async fn update_router_series_publisher(paths: &RuntimeDbPaths, series_id: &str, publisher: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds publisher update db should open");

    sqlx::query("UPDATE SERIES_METADATA SET PUBLISHER = ? WHERE SERIES_ID = ?")
        .bind(publisher)
        .bind(series_id)
        .execute(&pool)
        .await
        .expect("series publisher should be updated");

    pool.close().await;
}

async fn update_router_series_metadata_titles(
    paths: &RuntimeDbPaths,
    series_id: &str,
    title: &str,
    title_sort: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds series metadata update db should open");

    sqlx::query("UPDATE SERIES_METADATA SET TITLE = ?, TITLE_SORT = ? WHERE SERIES_ID = ?")
        .bind(title)
        .bind(title_sort)
        .bind(series_id)
        .execute(&pool)
        .await
        .expect("series metadata titles should be updated");

    pool.close().await;
}

async fn update_router_series_age_rating(paths: &RuntimeDbPaths, series_id: &str, age_rating: i64) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds series age rating update db should open");

    sqlx::query("UPDATE SERIES_METADATA SET AGE_RATING = ? WHERE SERIES_ID = ?")
        .bind(age_rating)
        .bind(series_id)
        .execute(&pool)
        .await
        .expect("series metadata age rating should be updated");

    pool.close().await;
}

async fn update_router_library_name(paths: &RuntimeDbPaths, library_id: &str, name: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds library update db should open");

    sqlx::query("UPDATE LIBRARY SET NAME = ? WHERE ID = ?")
        .bind(name)
        .bind(library_id)
        .execute(&pool)
        .await
        .expect("library name should be updated");

    pool.close().await;
}

async fn update_router_library_last_modified(
    paths: &RuntimeDbPaths,
    library_id: &str,
    last_modified: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds library last-modified update db should open");

    sqlx::query("UPDATE LIBRARY SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind(last_modified)
        .bind(library_id)
        .execute(&pool)
        .await
        .expect("library last modified should be updated");

    pool.close().await;
}

async fn update_router_collection_last_modified(
    paths: &RuntimeDbPaths,
    collection_id: &str,
    last_modified: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds collection last-modified update db should open");

    sqlx::query("UPDATE COLLECTION SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind(last_modified)
        .bind(collection_id)
        .execute(&pool)
        .await
        .expect("collection last modified should be updated");

    pool.close().await;
}

async fn update_router_readlist_last_modified(
    paths: &RuntimeDbPaths,
    readlist_id: &str,
    last_modified: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds readlist last-modified update db should open");

    sqlx::query("UPDATE READLIST SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind(last_modified)
        .bind(readlist_id)
        .execute(&pool)
        .await
        .expect("readlist last modified should be updated");

    pool.close().await;
}

async fn update_router_readlist_ordered(paths: &RuntimeDbPaths, readlist_id: &str, ordered: bool) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds readlist ordered update db should open");

    sqlx::query("UPDATE READLIST SET ORDERED = ? WHERE ID = ?")
        .bind(ordered)
        .bind(readlist_id)
        .execute(&pool)
        .await
        .expect("readlist ordered flag should be updated");

    pool.close().await;
}

async fn clear_router_series_sharing_labels(paths: &RuntimeDbPaths, series_id: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds series sharing labels delete db should open");

    sqlx::query("DELETE FROM SERIES_METADATA_SHARING WHERE SERIES_ID = ?")
        .bind(series_id)
        .execute(&pool)
        .await
        .expect("series sharing labels should be deleted");

    pool.close().await;
}

async fn seed_router_library(paths: &RuntimeDbPaths, library_id: &str, name: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds library seed db should open");

    sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
        .bind(library_id)
        .bind(name)
        .bind(paths.config_dir.to_string_lossy().to_string())
        .execute(&pool)
        .await
        .expect("library row should be inserted");

    pool.close().await;
}
