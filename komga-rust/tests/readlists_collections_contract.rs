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

const READLIST_THUMBNAIL_BYTES: &[u8] = b"\xff\xd8\xff\xdb\x00C\x00readlist-contract-jpeg\xff\xd9";

#[test]
fn readlists_collections_contract_target_is_registered() {
    assert_required_target_declared("readlists/collections", "readlists_collections_contract");
}

#[tokio::test]
async fn readlists_listing_reflects_persisted_rows() {
    let fixture = ReadlistsCollectionsContractFixture::new("readlists-list").await;

    seed_library(
        &fixture.paths.main_db,
        "library-readlists",
        &fixture.library_root,
    )
    .await;
    seed_series_with_book(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedSeriesWithBook {
            library_id: "library-readlists",
            series_id: "series-readlists-a",
            series_title: "Readlists Source A",
            book_id: "book-readlists-a",
            book_title: "Readlists Book A",
            file_name: "readlists-book-a.cbz",
            number: 1,
        },
    )
    .await;
    seed_series_with_book(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedSeriesWithBook {
            library_id: "library-readlists",
            series_id: "series-readlists-b",
            series_title: "Readlists Source B",
            book_id: "book-readlists-b",
            book_title: "Readlists Book B",
            file_name: "readlists-book-b.cbz",
            number: 1,
        },
    )
    .await;
    seed_readlist(
        &fixture.paths.main_db,
        SeedReadlistRow {
            id: "readlist-alpha",
            name: "Alpha Queue",
            summary: "persisted alpha summary",
            ordered: true,
            created_date: "2024-02-01T00:00:00",
            last_modified_date: "2024-02-02T00:00:00",
            books: &[("book-readlists-a", 0), ("book-readlists-b", 1)],
        },
    )
    .await;
    seed_readlist(
        &fixture.paths.main_db,
        SeedReadlistRow {
            id: "readlist-zulu",
            name: "Zulu Queue",
            summary: "persisted zulu summary",
            ordered: false,
            created_date: "2024-02-03T00:00:00",
            last_modified_date: "2024-02-04T00:00:00",
            books: &[("book-readlists-b", 0)],
        },
    )
    .await;

    assert_eq!(
        readlist_row_count(&fixture.paths.main_db, "readlist-alpha").await,
        1
    );
    assert_eq!(
        readlist_row_count(&fixture.paths.main_db, "readlist-zulu").await,
        1
    );

    let token = admin_session_token(&fixture.app).await;
    let list = request_json(
        &fixture.app,
        "GET",
        "/api/v1/readlists?unpaged=true",
        &token,
        None,
    )
    .await;
    assert_eq!(list["totalElements"], Value::from(2));
    assert_eq!(
        readlist_names(&list),
        vec!["Alpha Queue".to_string(), "Zulu Queue".to_string()]
    );

    fixture.cleanup();
}

#[tokio::test]
async fn readlist_detail_reads_persisted_row_instead_of_seeded_snapshot_record() {
    let fixture = ReadlistsCollectionsContractFixture::new("readlists-detail").await;

    seed_library(
        &fixture.paths.main_db,
        "library-readlists",
        &fixture.library_root,
    )
    .await;
    seed_series_with_book(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedSeriesWithBook {
            library_id: "library-readlists",
            series_id: "series-readlists-a",
            series_title: "Readlists Source A",
            book_id: "book-readlists-a",
            book_title: "Readlists Book A",
            file_name: "readlists-book-a.cbz",
            number: 1,
        },
    )
    .await;
    seed_series_with_book(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedSeriesWithBook {
            library_id: "library-readlists",
            series_id: "series-readlists-b",
            series_title: "Readlists Source B",
            book_id: "book-readlists-b",
            book_title: "Readlists Book B",
            file_name: "readlists-book-b.cbz",
            number: 1,
        },
    )
    .await;
    seed_readlist(
        &fixture.paths.main_db,
        SeedReadlistRow {
            id: "readlist-alpha",
            name: "Alpha Queue",
            summary: "persisted alpha summary",
            ordered: true,
            created_date: "2024-02-01T00:00:00",
            last_modified_date: "2024-02-02T00:00:00",
            books: &[("book-readlists-a", 0), ("book-readlists-b", 1)],
        },
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let detail = request(
        &fixture.app,
        "GET",
        "/api/v1/readlists/readlist-alpha",
        &token,
        None,
    )
    .await;
    assert_eq!(
        detail.status(),
        StatusCode::OK,
        "GET /api/v1/readlists/{{id}} must resolve persisted readlist detail rows instead of snapshot-seeded ids only",
    );
    let detail = response_json(detail).await;

    assert_eq!(detail["id"], "readlist-alpha");
    assert_eq!(detail["name"], "Alpha Queue");
    assert_eq!(detail["summary"], "persisted alpha summary");
    assert_eq!(detail["ordered"], Value::Bool(true));
    assert_eq!(
        detail["bookIds"],
        json!(["book-readlists-a", "book-readlists-b"])
    );

    fixture.cleanup();
}

#[tokio::test]
async fn readlist_create_round_trips_through_follow_up_reads_instead_of_request_echo() {
    let fixture = ReadlistsCollectionsContractFixture::new("readlists-create").await;

    seed_library(
        &fixture.paths.main_db,
        "library-readlists",
        &fixture.library_root,
    )
    .await;
    seed_series_with_book(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedSeriesWithBook {
            library_id: "library-readlists",
            series_id: "series-create-a",
            series_title: "Create Source A",
            book_id: "book-create-a",
            book_title: "Create Book A",
            file_name: "create-book-a.cbz",
            number: 1,
        },
    )
    .await;
    seed_series_with_book(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedSeriesWithBook {
            library_id: "library-readlists",
            series_id: "series-create-b",
            series_title: "Create Source B",
            book_id: "book-create-b",
            book_title: "Create Book B",
            file_name: "create-book-b.cbz",
            number: 1,
        },
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let create_body = json!({
        "name": "Created Readlist",
        "summary": "must survive follow-up read",
        "ordered": false,
        "bookIds": ["book-create-b", "book-create-a"],
    });

    let create_response = request(
        &fixture.app,
        "POST",
        "/api/v1/readlists",
        &token,
        Some(create_body.clone()),
    )
    .await;
    assert_eq!(
        create_response.status(),
        StatusCode::OK,
        "POST /api/v1/readlists must return a created readlist DTO sourced from persisted state",
    );
    let created = response_json(create_response).await;
    let created_id = created["id"]
        .as_str()
        .expect("created readlist response should include an id")
        .to_string();

    let follow_up_detail = request(
        &fixture.app,
        "GET",
        &format!("/api/v1/readlists/{created_id}"),
        &token,
        None,
    )
    .await;
    assert_eq!(
        follow_up_detail.status(),
        StatusCode::OK,
        "create contract rejects echo-only behavior: the returned readlist id must resolve on a fresh GET /api/v1/readlists/{{id}}",
    );
    let follow_up_detail = response_json(follow_up_detail).await;

    let follow_up_list = request_json(
        &fixture.app,
        "GET",
        "/api/v1/readlists?unpaged=true",
        &token,
        None,
    )
    .await;

    assert_eq!(follow_up_detail["name"], create_body["name"]);
    assert_eq!(follow_up_detail["summary"], create_body["summary"]);
    assert_eq!(follow_up_detail["ordered"], create_body["ordered"]);
    assert_eq!(follow_up_detail["bookIds"], create_body["bookIds"]);
    assert!(
        readlist_ids(&follow_up_list).contains(&created_id),
        "created readlist must appear in a fresh list response after the write completes",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn readlist_update_changes_follow_up_read_instead_of_accepting_only_seeded_ids() {
    let fixture = ReadlistsCollectionsContractFixture::new("readlists-update").await;

    seed_library(
        &fixture.paths.main_db,
        "library-readlists",
        &fixture.library_root,
    )
    .await;
    seed_series_with_book(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedSeriesWithBook {
            library_id: "library-readlists",
            series_id: "series-update-a",
            series_title: "Update Source A",
            book_id: "book-update-a",
            book_title: "Update Book A",
            file_name: "update-book-a.cbz",
            number: 1,
        },
    )
    .await;
    seed_series_with_book(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedSeriesWithBook {
            library_id: "library-readlists",
            series_id: "series-update-b",
            series_title: "Update Source B",
            book_id: "book-update-b",
            book_title: "Update Book B",
            file_name: "update-book-b.cbz",
            number: 1,
        },
    )
    .await;
    seed_readlist(
        &fixture.paths.main_db,
        SeedReadlistRow {
            id: "readlist-update-target",
            name: "Before Update",
            summary: "before summary",
            ordered: true,
            created_date: "2024-03-01T00:00:00",
            last_modified_date: "2024-03-02T00:00:00",
            books: &[("book-update-a", 0)],
        },
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let patch_response = request(
        &fixture.app,
        "PATCH",
        "/api/v1/readlists/readlist-update-target",
        &token,
        Some(json!({
            "name": "After Update",
            "summary": "after summary",
            "ordered": false,
            "bookIds": ["book-update-b", "book-update-a"],
        })),
    )
    .await;
    assert_eq!(
        patch_response.status(),
        StatusCode::NO_CONTENT,
        "PATCH /api/v1/readlists/{{id}} must update arbitrary persisted readlist ids, not only seeded placeholders",
    );

    let detail = request_json(
        &fixture.app,
        "GET",
        "/api/v1/readlists/readlist-update-target",
        &token,
        None,
    )
    .await;

    assert_eq!(detail["name"], "After Update");
    assert_eq!(detail["summary"], "after summary");
    assert_eq!(detail["ordered"], Value::Bool(false));
    assert_eq!(detail["bookIds"], json!(["book-update-b", "book-update-a"]));

    fixture.cleanup();
}

#[tokio::test]
async fn readlist_delete_removes_persisted_row_and_follow_up_read_stops_resolving() {
    let fixture = ReadlistsCollectionsContractFixture::new("readlists-delete").await;

    seed_library(
        &fixture.paths.main_db,
        "library-readlists",
        &fixture.library_root,
    )
    .await;
    seed_series_with_book(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedSeriesWithBook {
            library_id: "library-readlists",
            series_id: "series-delete-a",
            series_title: "Delete Source A",
            book_id: "book-delete-a",
            book_title: "Delete Book A",
            file_name: "delete-book-a.cbz",
            number: 1,
        },
    )
    .await;
    seed_readlist(
        &fixture.paths.main_db,
        SeedReadlistRow {
            id: "readlist-delete-target",
            name: "Delete Target",
            summary: "delete me",
            ordered: true,
            created_date: "2024-03-03T00:00:00",
            last_modified_date: "2024-03-04T00:00:00",
            books: &[("book-delete-a", 0)],
        },
    )
    .await;

    assert_eq!(
        readlist_row_count(&fixture.paths.main_db, "readlist-delete-target").await,
        1
    );

    let token = admin_session_token(&fixture.app).await;
    let delete_response = request(
        &fixture.app,
        "DELETE",
        "/api/v1/readlists/readlist-delete-target",
        &token,
        None,
    )
    .await;
    assert_eq!(
        delete_response.status(),
        StatusCode::NO_CONTENT,
        "DELETE /api/v1/readlists/{{id}} must remove persisted readlists rather than only acknowledging seeded ids",
    );
    assert_eq!(
        readlist_row_count(&fixture.paths.main_db, "readlist-delete-target").await,
        0
    );

    let detail_after_delete = request(
        &fixture.app,
        "GET",
        "/api/v1/readlists/readlist-delete-target",
        &token,
        None,
    )
    .await;
    assert_eq!(detail_after_delete.status(), StatusCode::NOT_FOUND);

    fixture.cleanup();
}

#[tokio::test]
async fn readlist_thumbnail_routes_use_persisted_state_when_present() {
    let fixture = ReadlistsCollectionsContractFixture::new("readlists-thumbnail").await;

    seed_library(
        &fixture.paths.main_db,
        "library-readlists",
        &fixture.library_root,
    )
    .await;
    seed_series_with_book(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedSeriesWithBook {
            library_id: "library-readlists",
            series_id: "series-media-a",
            series_title: "Media Source A",
            book_id: "book-media-a",
            book_title: "Media Book A",
            file_name: "media-book-a.cbz",
            number: 1,
        },
    )
    .await;
    seed_readlist(
        &fixture.paths.main_db,
        SeedReadlistRow {
            id: "readlist-media-target",
            name: "Persisted Readlist Export",
            summary: "thumbnail and zip contract",
            ordered: true,
            created_date: "2024-03-05T00:00:00",
            last_modified_date: "2024-03-06T00:00:00",
            books: &[("book-media-a", 0)],
        },
    )
    .await;
    insert_readlist_thumbnail(
        &fixture.paths.main_db,
        "thumbnail-readlist-persisted",
        "readlist-media-target",
        true,
    )
    .await;

    let token = admin_session_token(&fixture.app).await;

    let thumbnail_response = request(
        &fixture.app,
        "GET",
        "/api/v1/readlists/readlist-media-target/thumbnail",
        &token,
        None,
    )
    .await;
    assert_eq!(
        thumbnail_response.status(),
        StatusCode::OK,
        "readlist thumbnail contract requires persisted readlists to resolve /thumbnail rather than fixed seeded ids only",
    );
    assert_eq!(
        thumbnail_response
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("thumbnail response should include content type"),
        "image/jpeg",
    );
    assert_eq!(
        response_bytes(thumbnail_response).await.as_ref(),
        READLIST_THUMBNAIL_BYTES
    );

    let thumbnails_list = request_json(
        &fixture.app,
        "GET",
        "/api/v1/readlists/readlist-media-target/thumbnails",
        &token,
        None,
    )
    .await;
    assert_eq!(thumbnails_list[0]["id"], "thumbnail-readlist-persisted");
    assert_eq!(thumbnails_list[0]["selected"], Value::Bool(true));

    let thumbnail_by_id = request(
        &fixture.app,
        "GET",
        "/api/v1/readlists/readlist-media-target/thumbnails/thumbnail-readlist-persisted",
        &token,
        None,
    )
    .await;
    assert_eq!(thumbnail_by_id.status(), StatusCode::OK);
    assert_eq!(
        response_bytes(thumbnail_by_id).await.as_ref(),
        READLIST_THUMBNAIL_BYTES
    );

    fixture.cleanup();
}

#[tokio::test]
async fn readlist_export_route_uses_persisted_state_when_present() {
    let fixture = ReadlistsCollectionsContractFixture::new("readlists-export").await;

    seed_library(
        &fixture.paths.main_db,
        "library-readlists",
        &fixture.library_root,
    )
    .await;
    seed_series_with_book(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedSeriesWithBook {
            library_id: "library-readlists",
            series_id: "series-media-a",
            series_title: "Media Source A",
            book_id: "book-media-a",
            book_title: "Media Book A",
            file_name: "media-book-a.cbz",
            number: 1,
        },
    )
    .await;
    seed_readlist(
        &fixture.paths.main_db,
        SeedReadlistRow {
            id: "readlist-media-target",
            name: "Persisted Readlist Export",
            summary: "thumbnail and zip contract",
            ordered: true,
            created_date: "2024-03-05T00:00:00",
            last_modified_date: "2024-03-06T00:00:00",
            books: &[("book-media-a", 0)],
        },
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let file_response = request(
        &fixture.app,
        "GET",
        "/api/v1/readlists/readlist-media-target/file",
        &token,
        None,
    )
    .await;
    assert_eq!(
        file_response.status(),
        StatusCode::OK,
        "readlist export contract requires persisted readlists to be downloadable through /file",
    );
    assert_eq!(
        file_response
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("file response should include content type"),
        "application/zip",
    );
    let disposition = file_response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .expect("file response should include content disposition")
        .to_str()
        .expect("content disposition should be utf-8");
    assert!(
        disposition.contains("Persisted%20Readlist%20Export.zip")
            || disposition.contains("Persisted Readlist Export.zip"),
        "download filename must derive from the persisted readlist name",
    );
    assert!(
        !response_bytes(file_response).await.is_empty(),
        "download route must stream a non-empty zip payload for the persisted readlist",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn collections_listing_reflects_persisted_rows() {
    let fixture = ReadlistsCollectionsContractFixture::new("collections-list").await;

    seed_library(
        &fixture.paths.main_db,
        "library-collections",
        &fixture.library_root,
    )
    .await;
    seed_series_with_book(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedSeriesWithBook {
            library_id: "library-collections",
            series_id: "series-collections-a",
            series_title: "Collections Source A",
            book_id: "book-collections-a",
            book_title: "Collections Book A",
            file_name: "collections-book-a.cbz",
            number: 1,
        },
    )
    .await;
    seed_series_with_book(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedSeriesWithBook {
            library_id: "library-collections",
            series_id: "series-collections-b",
            series_title: "Collections Source B",
            book_id: "book-collections-b",
            book_title: "Collections Book B",
            file_name: "collections-book-b.cbz",
            number: 1,
        },
    )
    .await;
    seed_collection(
        &fixture.paths.main_db,
        SeedCollectionRow {
            id: "collection-alpha",
            name: "Alpha Collection",
            ordered: true,
            created_date: "2024-04-01T00:00:00",
            last_modified_date: "2024-04-02T00:00:00",
            series: &[("series-collections-a", 0), ("series-collections-b", 1)],
        },
    )
    .await;
    seed_collection(
        &fixture.paths.main_db,
        SeedCollectionRow {
            id: "collection-zulu",
            name: "Zulu Collection",
            ordered: false,
            created_date: "2024-04-03T00:00:00",
            last_modified_date: "2024-04-04T00:00:00",
            series: &[("series-collections-b", 0)],
        },
    )
    .await;

    assert_eq!(
        collection_row_count(&fixture.paths.main_db, "collection-alpha").await,
        1
    );
    assert_eq!(
        collection_row_count(&fixture.paths.main_db, "collection-zulu").await,
        1
    );

    let token = admin_session_token(&fixture.app).await;
    let list_response = request(
        &fixture.app,
        "GET",
        "/api/v1/collections?unpaged=true",
        &token,
        None,
    )
    .await;
    assert_eq!(
        list_response.status(),
        StatusCode::OK,
        "GET /api/v1/collections must list persisted collection rows in Kotlin-visible shape",
    );
    let list = response_json(list_response).await;

    assert_eq!(list["totalElements"], Value::from(2));
    assert_eq!(
        collection_names(&list),
        vec![
            "Alpha Collection".to_string(),
            "Zulu Collection".to_string()
        ]
    );

    fixture.cleanup();
}

#[tokio::test]
async fn collection_detail_reads_persisted_row_instead_of_missing_route() {
    let fixture = ReadlistsCollectionsContractFixture::new("collections-detail").await;

    seed_library(
        &fixture.paths.main_db,
        "library-collections",
        &fixture.library_root,
    )
    .await;
    seed_series_with_book(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedSeriesWithBook {
            library_id: "library-collections",
            series_id: "series-collections-a",
            series_title: "Collections Source A",
            book_id: "book-collections-a",
            book_title: "Collections Book A",
            file_name: "collections-book-a.cbz",
            number: 1,
        },
    )
    .await;
    seed_series_with_book(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedSeriesWithBook {
            library_id: "library-collections",
            series_id: "series-collections-b",
            series_title: "Collections Source B",
            book_id: "book-collections-b",
            book_title: "Collections Book B",
            file_name: "collections-book-b.cbz",
            number: 1,
        },
    )
    .await;
    seed_collection(
        &fixture.paths.main_db,
        SeedCollectionRow {
            id: "collection-alpha",
            name: "Alpha Collection",
            ordered: true,
            created_date: "2024-04-01T00:00:00",
            last_modified_date: "2024-04-02T00:00:00",
            series: &[("series-collections-a", 0), ("series-collections-b", 1)],
        },
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let detail = request(
        &fixture.app,
        "GET",
        "/api/v1/collections/collection-alpha",
        &token,
        None,
    )
    .await;
    assert_eq!(
        detail.status(),
        StatusCode::OK,
        "GET /api/v1/collections/{{id}} must resolve persisted collection detail rows instead of being absent",
    );
    let detail = response_json(detail).await;

    assert_eq!(detail["id"], "collection-alpha");
    assert_eq!(detail["name"], "Alpha Collection");
    assert_eq!(detail["ordered"], Value::Bool(true));
    assert_eq!(
        detail["seriesIds"],
        json!(["series-collections-a", "series-collections-b"])
    );

    fixture.cleanup();
}

#[tokio::test]
async fn collection_create_round_trips_through_follow_up_reads_instead_of_request_echo() {
    let fixture = ReadlistsCollectionsContractFixture::new("collections-create").await;

    seed_library(
        &fixture.paths.main_db,
        "library-collections",
        &fixture.library_root,
    )
    .await;
    seed_series_with_book(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedSeriesWithBook {
            library_id: "library-collections",
            series_id: "series-create-a",
            series_title: "Collection Create A",
            book_id: "book-collection-create-a",
            book_title: "Collection Create Book A",
            file_name: "collection-create-a.cbz",
            number: 1,
        },
    )
    .await;
    seed_series_with_book(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedSeriesWithBook {
            library_id: "library-collections",
            series_id: "series-create-b",
            series_title: "Collection Create B",
            book_id: "book-collection-create-b",
            book_title: "Collection Create Book B",
            file_name: "collection-create-b.cbz",
            number: 1,
        },
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let create_body = json!({
        "name": "Created Collection",
        "ordered": false,
        "seriesIds": ["series-create-b", "series-create-a"],
    });
    let create_response = request(
        &fixture.app,
        "POST",
        "/api/v1/collections",
        &token,
        Some(create_body.clone()),
    )
    .await;
    assert_eq!(
        create_response.status(),
        StatusCode::OK,
        "POST /api/v1/collections must return a created collection DTO backed by persisted state",
    );
    let created = response_json(create_response).await;
    let created_id = created["id"]
        .as_str()
        .expect("created collection response should include an id")
        .to_string();

    let follow_up_detail = request(
        &fixture.app,
        "GET",
        &format!("/api/v1/collections/{created_id}"),
        &token,
        None,
    )
    .await;
    assert_eq!(
        follow_up_detail.status(),
        StatusCode::OK,
        "collection create contract rejects echo-only behavior: the returned id must resolve via follow-up detail reads",
    );
    let follow_up_detail = response_json(follow_up_detail).await;
    let follow_up_list = request_json(
        &fixture.app,
        "GET",
        "/api/v1/collections?unpaged=true",
        &token,
        None,
    )
    .await;

    assert_eq!(follow_up_detail["name"], create_body["name"]);
    assert_eq!(follow_up_detail["ordered"], create_body["ordered"]);
    assert_eq!(follow_up_detail["seriesIds"], create_body["seriesIds"]);
    assert!(
        collection_ids(&follow_up_list).contains(&created_id),
        "created collection must appear in a fresh collections list after the write completes",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn collection_update_changes_follow_up_read_instead_of_accepting_only_seeded_ids() {
    let fixture = ReadlistsCollectionsContractFixture::new("collections-update").await;

    seed_library(
        &fixture.paths.main_db,
        "library-collections",
        &fixture.library_root,
    )
    .await;
    seed_series_with_book(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedSeriesWithBook {
            library_id: "library-collections",
            series_id: "series-update-a",
            series_title: "Collection Update A",
            book_id: "book-collection-update-a",
            book_title: "Collection Update Book A",
            file_name: "collection-update-a.cbz",
            number: 1,
        },
    )
    .await;
    seed_series_with_book(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedSeriesWithBook {
            library_id: "library-collections",
            series_id: "series-update-b",
            series_title: "Collection Update B",
            book_id: "book-collection-update-b",
            book_title: "Collection Update Book B",
            file_name: "collection-update-b.cbz",
            number: 1,
        },
    )
    .await;
    seed_collection(
        &fixture.paths.main_db,
        SeedCollectionRow {
            id: "collection-update-target",
            name: "Before Collection Update",
            ordered: true,
            created_date: "2024-04-05T00:00:00",
            last_modified_date: "2024-04-06T00:00:00",
            series: &[("series-update-a", 0)],
        },
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let patch_response = request(
        &fixture.app,
        "PATCH",
        "/api/v1/collections/collection-update-target",
        &token,
        Some(json!({
            "name": "After Collection Update",
            "ordered": false,
            "seriesIds": ["series-update-b", "series-update-a"],
        })),
    )
    .await;
    assert_eq!(
        patch_response.status(),
        StatusCode::NO_CONTENT,
        "PATCH /api/v1/collections/{{id}} must update arbitrary persisted collections, not only hardcoded placeholders",
    );

    let detail = request_json(
        &fixture.app,
        "GET",
        "/api/v1/collections/collection-update-target",
        &token,
        None,
    )
    .await;
    assert_eq!(detail["name"], "After Collection Update");
    assert_eq!(detail["ordered"], Value::Bool(false));
    assert_eq!(
        detail["seriesIds"],
        json!(["series-update-b", "series-update-a"])
    );

    fixture.cleanup();
}

#[tokio::test]
async fn collection_delete_removes_persisted_row_and_follow_up_read_stops_resolving() {
    let fixture = ReadlistsCollectionsContractFixture::new("collections-delete").await;

    seed_library(
        &fixture.paths.main_db,
        "library-collections",
        &fixture.library_root,
    )
    .await;
    seed_series_with_book(
        &fixture.paths.main_db,
        &fixture.library_root,
        SeedSeriesWithBook {
            library_id: "library-collections",
            series_id: "series-delete-a",
            series_title: "Collection Delete A",
            book_id: "book-collection-delete-a",
            book_title: "Collection Delete Book A",
            file_name: "collection-delete-a.cbz",
            number: 1,
        },
    )
    .await;
    seed_collection(
        &fixture.paths.main_db,
        SeedCollectionRow {
            id: "collection-delete-target",
            name: "Delete Collection",
            ordered: true,
            created_date: "2024-04-07T00:00:00",
            last_modified_date: "2024-04-08T00:00:00",
            series: &[("series-delete-a", 0)],
        },
    )
    .await;

    assert_eq!(
        collection_row_count(&fixture.paths.main_db, "collection-delete-target").await,
        1
    );

    let token = admin_session_token(&fixture.app).await;
    let delete_response = request(
        &fixture.app,
        "DELETE",
        "/api/v1/collections/collection-delete-target",
        &token,
        None,
    )
    .await;
    assert_eq!(
        delete_response.status(),
        StatusCode::NO_CONTENT,
        "DELETE /api/v1/collections/{{id}} must remove persisted collections rather than acknowledge only a fixed seeded id",
    );
    assert_eq!(
        collection_row_count(&fixture.paths.main_db, "collection-delete-target").await,
        0
    );

    let detail_after_delete = request(
        &fixture.app,
        "GET",
        "/api/v1/collections/collection-delete-target",
        &token,
        None,
    )
    .await;
    assert_eq!(detail_after_delete.status(), StatusCode::NOT_FOUND);

    fixture.cleanup();
}

struct ReadlistsCollectionsContractFixture {
    paths: persistence_contract_fixture::LegacyDbPaths,
    app: axum::Router,
    library_root: PathBuf,
}

impl ReadlistsCollectionsContractFixture {
    async fn new(case_id: &str) -> Self {
        compat_auth_env::ensure_compat_auth_env();

        let paths = persistence_contract_fixture::new_legacy_db_paths(case_id)
            .expect("readlists/collections contract db paths should be created");
        persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
            .await
            .expect("main db flyway fixture should be created");
        persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
            .await
            .expect("tasks db flyway fixture should be created");

        fs::create_dir_all(paths.config_dir.join("lucene")).expect(
            "lucene directory should be created for readlists/collections contract fixture",
        );
        fs::create_dir_all(paths.config_dir.join("fonts"))
            .expect("fonts directory should be created for readlists/collections contract fixture");

        let library_root = create_library_root(&paths.config_dir, "readlists-collections-library");

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

struct SeedSeriesWithBook<'a> {
    library_id: &'a str,
    series_id: &'a str,
    series_title: &'a str,
    book_id: &'a str,
    book_title: &'a str,
    file_name: &'a str,
    number: i32,
}

struct SeedReadlistRow<'a> {
    id: &'a str,
    name: &'a str,
    summary: &'a str,
    ordered: bool,
    created_date: &'a str,
    last_modified_date: &'a str,
    books: &'a [(&'a str, i32)],
}

struct SeedCollectionRow<'a> {
    id: &'a str,
    name: &'a str,
    ordered: bool,
    created_date: &'a str,
    last_modified_date: &'a str,
    series: &'a [(&'a str, i32)],
}

fn create_library_root(config_dir: &Path, name: &str) -> PathBuf {
    let root = config_dir.join(name);
    fs::create_dir_all(&root).expect("library root fixture directory should be created");
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
                .expect("users/me login request should build"),
        )
        .await
        .expect("users/me login request should succeed");

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
        .oneshot(
            builder
                .body(request_body)
                .expect("contract request should build"),
        )
        .await
        .expect("contract request should execute")
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&body).expect("response body should be valid json")
}

async fn response_bytes(response: axum::response::Response) -> axum::body::Bytes {
    axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable")
}

async fn seed_library(main_db: &Path, library_id: &str, root: &Path) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for library fixture seeding");

    sqlx::query(
        "INSERT INTO LIBRARY (ID, NAME, ROOT, SCAN_STARTUP, EMPTY_TRASH_AFTER_SCAN, ONESHOTS_DIRECTORY) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(library_id)
    .bind("Readlists Collections Library")
    .bind(root.to_string_lossy().to_string())
    .bind(false)
    .bind(false)
    .bind(None::<String>)
    .execute(&pool)
    .await
    .expect("library fixture row should insert");

    pool.close().await;
}

async fn seed_series_with_book(main_db: &Path, library_root: &Path, row: SeedSeriesWithBook<'_>) {
    fs::write(library_root.join(row.file_name), b"persisted-media-payload")
        .expect("persisted media fixture file should be written");

    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for series fixture seeding");

    sqlx::query(
        "INSERT INTO SERIES (ID, CREATED_DATE, LAST_MODIFIED_DATE, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, ONESHOT, DELETED_DATE) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(row.series_id)
    .bind("2024-01-01T00:00:00")
    .bind("2024-01-02T00:00:00")
    .bind("2024-01-02T00:00:00")
    .bind(row.series_title)
    .bind(format!("/library/{}/series/{}", row.library_id, row.series_id))
    .bind(row.library_id)
    .bind(false)
    .bind(None::<String>)
    .execute(&pool)
    .await
    .expect("series fixture row should insert");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (CREATED_DATE, LAST_MODIFIED_DATE, STATUS, TITLE, TITLE_SORT, SUMMARY, LANGUAGE, PUBLISHER, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("2024-01-01T00:00:00")
    .bind("2024-01-02T00:00:00")
    .bind("ONGOING")
    .bind(row.series_title)
    .bind(row.series_title)
    .bind(format!("summary for {}", row.series_title))
    .bind("en")
    .bind("Komga Press")
    .bind(row.series_id)
    .execute(&pool)
    .await
    .expect("series metadata fixture row should insert");

    sqlx::query(
        "INSERT INTO BOOK (ID, CREATED_DATE, LAST_MODIFIED_DATE, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, ONESHOT, DELETED_DATE) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(row.book_id)
    .bind("2024-01-01T00:00:00")
    .bind("2024-01-02T00:00:00")
    .bind("2024-01-02T00:00:00")
    .bind(row.file_name)
    .bind(format!("/library/{}/books/{}", row.library_id, row.file_name))
    .bind(row.series_id)
    .bind(22_i64)
    .bind(row.number)
    .bind(row.library_id)
    .bind(false)
    .bind(None::<String>)
    .execute(&pool)
    .await
    .expect("book fixture row should insert");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (CREATED_DATE, LAST_MODIFIED_DATE, NUMBER, NUMBER_SORT, TITLE, SUMMARY, BOOK_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("2024-01-01T00:00:00")
    .bind("2024-01-02T00:00:00")
    .bind(row.number.to_string())
    .bind(row.number as f64)
    .bind(row.book_title)
    .bind(format!("summary for {}", row.book_title))
    .bind(row.book_id)
    .execute(&pool)
    .await
    .expect("book metadata fixture row should insert");

    sqlx::query("INSERT INTO MEDIA (STATUS, MEDIA_TYPE, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("READY")
        .bind("application/vnd.comicbook+zip")
        .bind(row.book_id)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("media fixture row should insert");

    pool.close().await;
}

async fn seed_readlist(main_db: &Path, row: SeedReadlistRow<'_>) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for readlist fixture seeding");

    sqlx::query(
        "INSERT INTO READLIST (ID, NAME, BOOK_COUNT, CREATED_DATE, LAST_MODIFIED_DATE, SUMMARY, ORDERED) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(row.id)
    .bind(row.name)
    .bind(row.books.len() as i32)
    .bind(row.created_date)
    .bind(row.last_modified_date)
    .bind(row.summary)
    .bind(row.ordered)
    .execute(&pool)
    .await
    .expect("readlist fixture row should insert");

    for (book_id, number) in row.books {
        sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
            .bind(row.id)
            .bind(book_id)
            .bind(number)
            .execute(&pool)
            .await
            .expect("readlist membership fixture row should insert");
    }

    pool.close().await;
}

async fn seed_collection(main_db: &Path, row: SeedCollectionRow<'_>) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for collection fixture seeding");

    sqlx::query(
        "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT, CREATED_DATE, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(row.id)
    .bind(row.name)
    .bind(row.ordered)
    .bind(row.series.len() as i32)
    .bind(row.created_date)
    .bind(row.last_modified_date)
    .execute(&pool)
    .await
    .expect("collection fixture row should insert");

    for (series_id, number) in row.series {
        sqlx::query(
            "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
        )
        .bind(row.id)
        .bind(series_id)
        .bind(number)
        .execute(&pool)
        .await
        .expect("collection membership fixture row should insert");
    }

    pool.close().await;
}

async fn insert_readlist_thumbnail(
    main_db: &Path,
    thumbnail_id: &str,
    readlist_id: &str,
    selected: bool,
) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for readlist thumbnail fixture seeding");

    sqlx::query(
        "INSERT INTO THUMBNAIL_READLIST (ID, THUMBNAIL, SELECTED, TYPE, READLIST_ID, WIDTH, HEIGHT, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(thumbnail_id)
    .bind(READLIST_THUMBNAIL_BYTES)
    .bind(selected)
    .bind("SIDECAR")
    .bind(readlist_id)
    .bind(300_i32)
    .bind(450_i32)
    .bind("image/jpeg")
    .bind(READLIST_THUMBNAIL_BYTES.len() as i64)
    .execute(&pool)
    .await
    .expect("readlist thumbnail fixture row should insert");

    pool.close().await;
}

async fn readlist_row_count(main_db: &Path, readlist_id: &str) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for readlist count inspection");
    let count = sqlx::query("SELECT COUNT(*) AS COUNT FROM READLIST WHERE ID = ?")
        .bind(readlist_id)
        .fetch_one(&pool)
        .await
        .expect("readlist count should be queryable")
        .get::<i64, _>("COUNT");
    pool.close().await;
    count
}

async fn collection_row_count(main_db: &Path, collection_id: &str) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for collection count inspection");
    let count = sqlx::query("SELECT COUNT(*) AS COUNT FROM COLLECTION WHERE ID = ?")
        .bind(collection_id)
        .fetch_one(&pool)
        .await
        .expect("collection count should be queryable")
        .get::<i64, _>("COUNT");
    pool.close().await;
    count
}

fn readlist_names(payload: &Value) -> Vec<String> {
    payload
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|readlist| readlist.get("name").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

fn readlist_ids(payload: &Value) -> Vec<String> {
    payload
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|readlist| readlist.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

fn collection_names(payload: &Value) -> Vec<String> {
    payload
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|collection| collection.get("name").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

fn collection_ids(payload: &Value) -> Vec<String> {
    payload
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|collection| collection.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}
