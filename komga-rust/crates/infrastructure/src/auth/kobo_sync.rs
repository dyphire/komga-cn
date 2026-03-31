use std::collections::HashMap;
use std::path::Path;

use komga_application::identity_access::{
    KoboStoreSyncMergeResult, KoboSyncBookSnapshot, KoboSyncDeltas, KoboSyncPointState,
    KoboSyncReadListSnapshot, KoboSyncReadProgressSnapshot, KoboSyncSnapshot,
    decode_or_passthrough_sync_token, now_sync_marker,
};
use reqwest::header::{HeaderName, HeaderValue};
use serde_json::{Value, json};
use sqlx::Row;

use crate::sqlite::connect_pool;

async fn ensure_kobo_sync_state_table(database_file: &Path) -> Result<(), sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS KOBO_SYNC_POINT_STATE ( SYNC_POINT_ID TEXT NOT NULL, USER_ID TEXT NOT NULL, STATE_JSON TEXT NOT NULL, PRIMARY KEY (SYNC_POINT_ID, USER_ID) )",
    )
    .execute(&pool)
    .await?;
    Ok(())
}

pub async fn load_sync_point_state(
    database_file: &Path,
    sync_point_id: &str,
    user_id: &str,
) -> Option<KoboSyncPointState> {
    let _ = ensure_kobo_sync_state_table(database_file).await;
    let pool = connect_pool(database_file, 1).await.ok()?;
    let row = sqlx::query(
        "SELECT STATE_JSON FROM KOBO_SYNC_POINT_STATE WHERE SYNC_POINT_ID = ? AND USER_ID = ? LIMIT 1",
    )
    .bind(sync_point_id)
    .bind(user_id)
    .fetch_optional(&pool)
    .await
    .ok()?;

    row.and_then(|row| {
        serde_json::from_str::<KoboSyncPointState>(row.get::<String, _>("STATE_JSON").as_str()).ok()
    })
}

pub async fn load_sync_point_marker(
    database_file: &Path,
    sync_point_id: &str,
    user_id: &str,
) -> Option<String> {
    load_sync_point_state(database_file, sync_point_id, user_id)
        .await
        .map(|entry| entry.marker)
}

pub async fn save_sync_point(
    database_file: &Path,
    sync_point_id: &str,
    sync_point_state: &KoboSyncPointState,
) -> Result<(), sqlx::Error> {
    ensure_kobo_sync_state_table(database_file).await?;
    let pool = connect_pool(database_file, 1).await?;
    sqlx::query(
        "INSERT INTO KOBO_SYNC_POINT_STATE (SYNC_POINT_ID, USER_ID, STATE_JSON) VALUES (?, ?, ?) ON CONFLICT (SYNC_POINT_ID, USER_ID) DO UPDATE SET STATE_JSON = excluded.STATE_JSON",
    )
    .bind(sync_point_id)
    .bind(sync_point_state.user_id.as_str())
    .bind(serde_json::to_string(sync_point_state).unwrap_or_else(|_| "{}".to_string()))
    .execute(&pool)
    .await?;
    Ok(())
}

pub async fn remove_sync_point(
    database_file: &Path,
    sync_point_id: &str,
) -> Result<(), sqlx::Error> {
    let _ = ensure_kobo_sync_state_table(database_file).await;
    let pool = connect_pool(database_file, 1).await?;
    sqlx::query("DELETE FROM KOBO_SYNC_POINT_STATE WHERE SYNC_POINT_ID = ?")
        .bind(sync_point_id)
        .execute(&pool)
        .await?;
    Ok(())
}

pub async fn load_kobo_sync_snapshot(
    database_file: &Path,
    user_id: &str,
) -> Result<KoboSyncSnapshot, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;

    let books_rows = sqlx::query(
        "SELECT b.ID AS BOOK_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE, COALESCE(bm.SUMMARY, '') AS SUMMARY, bm.RELEASE_DATE AS RELEASE_DATE, COALESCE(sm.LANGUAGE, 'en') AS LANGUAGE, COALESCE(b.FILE_SIZE, 0) AS FILE_SIZE, COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT, COALESCE(b.CREATED_DATE, '') AS CREATED_DATE, COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') AS LAST_MODIFIED_DATE, NULLIF(TRIM(bm.ISBN), '') AS ISBN, NULLIF(TRIM(sm.PUBLISHER), '') AS PUBLISHER_NAME, tb.ID AS COVER_IMAGE_ID, sm.SERIES_ID AS SERIES_ID, sm.TITLE AS SERIES_NAME, NULLIF(TRIM(bm.NUMBER), '') AS SERIES_NUMBER, bm.NUMBER_SORT AS SERIES_NUMBER_FLOAT, COALESCE(b.ONESHOT, FALSE) AS ONESHOT FROM BOOK b LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = b.SERIES_ID LEFT JOIN THUMBNAIL_BOOK tb ON tb.BOOK_ID = b.ID AND tb.SELECTED = TRUE JOIN MEDIA m ON m.BOOK_ID = b.ID WHERE b.DELETED_DATE IS NULL AND m.STATUS = 'READY' AND m.MEDIA_TYPE = 'application/epub+zip' ORDER BY b.ID ASC",
    )
    .fetch_all(&pool)
    .await?;

    let author_rows = sqlx::query(
        "SELECT BOOK_ID, NAME FROM BOOK_METADATA_AUTHOR WHERE NAME IS NOT NULL AND TRIM(NAME) <> '' ORDER BY BOOK_ID ASC, NAME ASC",
    )
    .fetch_all(&pool)
    .await?;

    let progress_rows = sqlx::query(
        "SELECT rp.BOOK_ID AS BOOK_ID, rp.PAGE AS PAGE, rp.COMPLETED AS COMPLETED, COALESCE(rp.CREATED_DATE, '') AS CREATED_DATE, COALESCE(rp.LAST_MODIFIED_DATE, rp.CREATED_DATE, '') AS LAST_MODIFIED_DATE, rp.LOCATOR AS LOCATOR FROM READ_PROGRESS rp JOIN BOOK b ON b.ID = rp.BOOK_ID JOIN MEDIA m ON m.BOOK_ID = b.ID WHERE rp.USER_ID = ? AND b.DELETED_DATE IS NULL AND m.STATUS = 'READY' AND m.MEDIA_TYPE = 'application/epub+zip' ORDER BY rp.BOOK_ID ASC",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await?;

    let readlist_rows = sqlx::query(
        "SELECT rl.ID AS READLIST_ID, rl.NAME AS NAME, COALESCE(rl.CREATED_DATE, '') AS CREATED_DATE, COALESCE(rl.LAST_MODIFIED_DATE, rl.CREATED_DATE, '') AS LAST_MODIFIED_DATE, rb.BOOK_ID AS BOOK_ID, rb.NUMBER AS ORDER_INDEX, b.DELETED_DATE AS BOOK_DELETED_DATE FROM READLIST rl LEFT JOIN READLIST_BOOK rb ON rb.READLIST_ID = rl.ID LEFT JOIN BOOK b ON b.ID = rb.BOOK_ID ORDER BY rl.ID ASC, rb.NUMBER ASC, rb.BOOK_ID ASC",
    )
    .fetch_all(&pool)
    .await?;

    let mut authors_by_book = HashMap::<String, Vec<String>>::new();
    for row in author_rows {
        authors_by_book
            .entry(row.get::<String, _>("BOOK_ID"))
            .or_default()
            .push(row.get::<String, _>("NAME"));
    }

    let mut books = HashMap::new();
    for row in books_rows {
        let id = row.get::<String, _>("BOOK_ID");
        books.insert(
            id.clone(),
            KoboSyncBookSnapshot {
                id: id.clone(),
                title: row.get::<String, _>("TITLE"),
                summary: row.get::<String, _>("SUMMARY"),
                release_date: row.get::<Option<String>, _>("RELEASE_DATE"),
                language: row.get::<String, _>("LANGUAGE"),
                file_size: row.get::<i64, _>("FILE_SIZE").max(0) as u64,
                page_count: row.get::<i64, _>("PAGE_COUNT").max(1) as u64,
                created: row.get::<String, _>("CREATED_DATE"),
                last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
                contributor_names: authors_by_book.remove(&id).unwrap_or_default(),
                isbn: row.get::<Option<String>, _>("ISBN"),
                publisher_name: row.get::<Option<String>, _>("PUBLISHER_NAME"),
                cover_image_id: row.get::<Option<String>, _>("COVER_IMAGE_ID"),
                series_id: row.get::<Option<String>, _>("SERIES_ID"),
                series_name: row.get::<Option<String>, _>("SERIES_NAME"),
                series_number: row.get::<Option<String>, _>("SERIES_NUMBER"),
                series_number_float: row.get::<Option<f64>, _>("SERIES_NUMBER_FLOAT"),
                oneshot: row.get::<bool, _>("ONESHOT"),
            },
        );
    }

    let mut progress = HashMap::new();
    for row in progress_rows {
        let book_id = row.get::<String, _>("BOOK_ID");
        progress.insert(
            book_id.clone(),
            KoboSyncReadProgressSnapshot {
                page: row.get::<i64, _>("PAGE"),
                completed: row.get::<bool, _>("COMPLETED"),
                created: row.get::<String, _>("CREATED_DATE"),
                last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
                locator: row.get::<Option<Vec<u8>>, _>("LOCATOR"),
            },
        );
    }

    let mut readlists = HashMap::<String, KoboSyncReadListSnapshot>::new();
    for row in readlist_rows {
        let readlist_id = row.get::<String, _>("READLIST_ID");
        let entry =
            readlists
                .entry(readlist_id.clone())
                .or_insert_with(|| KoboSyncReadListSnapshot {
                    id: readlist_id,
                    name: row.get::<String, _>("NAME"),
                    created: row.get::<String, _>("CREATED_DATE"),
                    last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
                    items: Vec::new(),
                });

        let book_id = row.get::<Option<String>, _>("BOOK_ID");
        let book_deleted_date = row.get::<Option<String>, _>("BOOK_DELETED_DATE");

        if let Some(book_id) = book_id
            && book_deleted_date.is_none()
            && books.contains_key(&book_id)
        {
            entry.items.push(book_id);
        }
    }

    let ondeck_book_ids = load_kobo_ondeck_book_ids(database_file, user_id).await?;
    let ondeck_book_ids = ondeck_book_ids
        .into_iter()
        .filter(|book_id| books.contains_key(book_id))
        .collect::<Vec<_>>();
    if !ondeck_book_ids.is_empty() {
        let ondeck_last_modified = ondeck_book_ids
            .iter()
            .filter_map(|book_id| books.get(book_id).map(|book| book.last_modified.clone()))
            .max()
            .unwrap_or_else(now_sync_marker);
        readlists.insert(
            "komga-on-deck".to_string(),
            KoboSyncReadListSnapshot {
                id: "komga-on-deck".to_string(),
                name: "On Deck".to_string(),
                created: ondeck_last_modified.clone(),
                last_modified: ondeck_last_modified,
                items: ondeck_book_ids,
            },
        );
    }

    Ok(KoboSyncSnapshot {
        books,
        progress,
        readlists,
    })
}

pub async fn load_kobo_sync_deltas(
    database_file: &Path,
    user_id: &str,
    since: Option<&str>,
) -> Result<KoboSyncDeltas, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let since_value = since.unwrap_or_default();

    let rows = sqlx::query(
        "SELECT b.ID AS BOOK_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE FROM BOOK b LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID WHERE b.DELETED_DATE IS NULL AND (? = '' OR COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') > ?) ORDER BY b.ID ASC",
    )
    .bind(since_value)
    .bind(since_value)
    .fetch_all(&pool)
    .await?;

    let deleted_rows = sqlx::query(
        "SELECT b.ID AS BOOK_ID FROM BOOK b WHERE b.DELETED_DATE IS NOT NULL AND (? = '' OR COALESCE(b.DELETED_DATE, '') > ?) ORDER BY b.DELETED_DATE ASC, b.ID ASC",
    )
    .bind(since_value)
    .bind(since_value)
    .fetch_all(&pool)
    .await?;

    let read_progress_rows = sqlx::query(
        "SELECT rp.BOOK_ID AS BOOK_ID, rp.PAGE AS PAGE, rp.COMPLETED AS COMPLETED, COALESCE(rp.LAST_MODIFIED_DATE, rp.CREATED_DATE, '') AS LAST_MODIFIED_DATE FROM READ_PROGRESS rp JOIN BOOK b ON b.ID = rp.BOOK_ID WHERE rp.USER_ID = ? AND b.DELETED_DATE IS NULL AND (? = '' OR COALESCE(rp.LAST_MODIFIED_DATE, rp.CREATED_DATE, '') > ?) ORDER BY LAST_MODIFIED_DATE ASC, rp.BOOK_ID ASC",
    )
    .bind(user_id)
    .bind(since_value)
    .bind(since_value)
    .fetch_all(&pool)
    .await?;

    let tag_rows = sqlx::query(
        "SELECT DISTINCT bt.TAG AS TAG FROM BOOK_METADATA_TAG bt JOIN BOOK b ON b.ID = bt.BOOK_ID WHERE b.DELETED_DATE IS NULL AND (? = '' OR COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') > ?) ORDER BY lower(bt.TAG), bt.TAG",
    )
    .bind(since_value)
    .bind(since_value)
    .fetch_all(&pool)
    .await?;

    let mut entitlement = Vec::with_capacity(rows.len());
    let mut metadata = Vec::with_capacity(rows.len());
    let mut deleted_entitlement = Vec::with_capacity(deleted_rows.len());
    let mut deleted_book_metadata = Vec::with_capacity(deleted_rows.len());
    let mut new_reading_state = Vec::with_capacity(read_progress_rows.len());
    let mut new_tag = Vec::with_capacity(tag_rows.len());

    for row in rows {
        let book_id = row.get::<String, _>("BOOK_ID");
        let title = row.get::<String, _>("TITLE");
        entitlement.push(json!({
            "BookId": book_id,
            "BookMetadataId": book_id,
            "IsRemoved": false,
        }));
        metadata.push(json!({
            "BookId": book_id,
            "Title": title,
        }));
    }

    for row in deleted_rows {
        let book_id = row.get::<String, _>("BOOK_ID");
        deleted_entitlement.push(json!({
            "BookId": book_id,
            "BookMetadataId": book_id,
        }));
        deleted_book_metadata.push(json!({
            "BookId": book_id,
        }));
    }

    for row in read_progress_rows {
        let book_id = row.get::<String, _>("BOOK_ID");
        let page = row.get::<i64, _>("PAGE").max(0) as u64;
        let completed = row.get::<bool, _>("COMPLETED");
        let last_modified = row.get::<String, _>("LAST_MODIFIED_DATE");
        new_reading_state.push(json!({
            "EntitlementId": book_id,
            "LastModified": last_modified,
            "StatusInfo": {
                "Status": if completed { "Finished" } else { "Reading" },
                "TimesStartedReading": if page > 0 { 1 } else { 0 },
            },
        }));
    }

    for row in tag_rows {
        let tag = row.get::<String, _>("TAG");
        new_tag.push(json!({
            "Name": tag,
            "Type": "BookTag",
        }));
    }

    Ok(KoboSyncDeltas {
        new_entitlement: entitlement,
        deleted_entitlement,
        new_tag,
        deleted_tag: vec![],
        new_book_metadata: metadata,
        deleted_book_metadata,
        new_reading_state,
        deleted_reading_state: vec![],
    })
}

pub async fn proxy_kobo_store_library_sync(
    forwarded_headers: &[(String, String)],
    query: Option<&str>,
    raw_sync_token: &str,
) -> Result<KoboStoreSyncMergeResult, ()> {
    let mut target = String::from("https://storeapi.kobo.com/v1/library/sync");
    if let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) {
        target.push('?');
        target.push_str(query);
    }

    let client = reqwest::Client::builder().build().map_err(|_| ())?;
    let mut request = client.get(target);
    for (name, value) in forwarded_headers {
        let lower = name.to_ascii_lowercase();
        if lower == "host" || lower == "content-length" || lower == "x-kobo-synctoken" {
            continue;
        }
        let Ok(header_name) = HeaderName::from_bytes(name.as_bytes()) else {
            continue;
        };
        let Ok(header_value) = HeaderValue::from_str(value) else {
            continue;
        };
        request = request.header(header_name, header_value);
    }
    request = request.header("x-kobo-synctoken", raw_sync_token);

    let response = request.send().await.map_err(|_| ())?;
    if !response.status().is_success() {
        return Err(());
    }

    let headers = response.headers().clone();
    let body = response.json::<Value>().await.map_err(|_| ())?;
    let events = body.as_array().cloned().unwrap_or_default();
    let should_continue = headers
        .get("x-kobo-sync")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case("continue"));
    let raw_sync_token = headers
        .get("x-kobo-synctoken")
        .and_then(|value| value.to_str().ok())
        .and_then(decode_or_passthrough_sync_token);

    Ok(KoboStoreSyncMergeResult {
        events,
        raw_sync_token,
        should_continue,
    })
}

async fn load_kobo_ondeck_book_ids(
    database_file: &Path,
    user_id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT b.ID, b.SERIES_ID, b.NUMBER FROM BOOK b JOIN MEDIA m ON m.BOOK_ID = b.ID WHERE b.DELETED_DATE IS NULL AND m.STATUS = 'READY' AND m.MEDIA_TYPE = 'application/epub+zip' AND b.SERIES_ID IN ( SELECT DISTINCT b_done.SERIES_ID FROM BOOK b_done JOIN READ_PROGRESS rp_done ON rp_done.BOOK_ID = b_done.ID WHERE rp_done.USER_ID = ? AND rp_done.COMPLETED = 1 ) AND b.SERIES_ID NOT IN ( SELECT DISTINCT b_prog.SERIES_ID FROM BOOK b_prog JOIN READ_PROGRESS rp_prog ON rp_prog.BOOK_ID = b_prog.ID WHERE rp_prog.USER_ID = ? AND rp_prog.COMPLETED = 0 ) AND NOT EXISTS ( SELECT 1 FROM READ_PROGRESS rp_seen WHERE rp_seen.BOOK_ID = b.ID AND rp_seen.USER_ID = ? AND rp_seen.COMPLETED = 1 ) ORDER BY b.SERIES_ID ASC, b.NUMBER ASC",
    )
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .fetch_all(&pool)
    .await?;

    let mut first_per_series = HashMap::<String, String>::new();
    for row in rows {
        let series_id = row.get::<String, _>("SERIES_ID");
        let book_id = row.get::<String, _>("ID");
        first_per_series.entry(series_id).or_insert(book_id);
    }

    let mut ondeck = first_per_series.into_values().collect::<Vec<_>>();
    ondeck.sort();
    Ok(ondeck)
}
