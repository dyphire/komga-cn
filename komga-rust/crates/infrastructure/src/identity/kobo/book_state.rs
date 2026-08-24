use std::collections::{HashMap, HashSet};

use komga_application::identity_access::{
    KoboSyncBookSnapshot, KoboSyncBookState, KoboSyncPointBook, KoboSyncReadProgressSnapshot,
};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

struct KoboSyncBookMetadataRow {
    book_id: String,
    title: String,
    summary: String,
    release_date: Option<String>,
    created_date: Option<String>,
    language: String,
    file_size: u64,
    contributor_names: Vec<String>,
    isbn: Option<String>,
    publisher_name: Option<String>,
    cover_image_id: Option<String>,
    series_id: Option<String>,
    series_name: Option<String>,
    series_number: Option<String>,
    series_number_float: Option<f64>,
    oneshot: bool,
}

struct ReadProgressRow {
    page: i64,
    completed: bool,
    created: String,
    last_modified: String,
    locator: Option<Vec<u8>>,
}

pub(in crate::identity) async fn load_sync_book_states(
    pool: &SqlitePool,
    books: &[KoboSyncPointBook],
    user_id: &str,
) -> Result<Vec<KoboSyncBookState>, sqlx::Error> {
    if books.is_empty() {
        return Ok(Vec::new());
    }

    let book_ids = unique_book_ids(books);
    let metadata = load_metadata(pool, &book_ids).await?;
    let progress = load_read_progress(pool, &book_ids, user_id).await?;

    Ok(books
        .iter()
        .map(|book| KoboSyncBookState {
            book_id: book.book_id.clone(),
            book: metadata
                .get(&book.book_id)
                .map(|metadata| sync_book_snapshot_from_metadata(book, metadata)),
            progress: progress.get(&book.book_id).map(progress_snapshot),
        })
        .collect())
}

fn unique_book_ids(books: &[KoboSyncPointBook]) -> Vec<&str> {
    let mut seen = HashSet::new();
    books
        .iter()
        .filter_map(|book| {
            let book_id = book.book_id.as_str();
            seen.insert(book_id).then_some(book_id)
        })
        .collect()
}

async fn load_metadata(
    pool: &SqlitePool,
    book_ids: &[&str],
) -> Result<HashMap<String, KoboSyncBookMetadataRow>, sqlx::Error> {
    let mut query = QueryBuilder::<Sqlite>::new(
        r#"SELECT b.ID AS BOOK_ID,
       COALESCE(bm.TITLE, b.NAME) AS TITLE,
       COALESCE(bm.SUMMARY, '') AS SUMMARY,
       bm.RELEASE_DATE AS RELEASE_DATE,
       COALESCE(bm.CREATED_DATE, b.CREATED_DATE, '') AS CREATED_DATE,
       COALESCE(sm.LANGUAGE, 'en') AS LANGUAGE,
       b.FILE_SIZE AS FILE_SIZE,
       NULLIF(TRIM(bm.ISBN), '') AS ISBN,
       NULLIF(TRIM(sm.PUBLISHER), '') AS PUBLISHER_NAME,
       tb.ID AS COVER_IMAGE_ID,
       sm.SERIES_ID AS SERIES_ID,
       sm.TITLE AS SERIES_NAME,
       NULLIF(TRIM(bm.NUMBER), '') AS SERIES_NUMBER,
       bm.NUMBER_SORT AS SERIES_NUMBER_FLOAT,
       b.ONESHOT AS ONESHOT
 FROM BOOK b
  LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
  LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = b.SERIES_ID
  LEFT JOIN THUMBNAIL_BOOK tb ON tb.BOOK_ID = b.ID AND tb.SELECTED = TRUE
 WHERE b.DELETED_DATE IS NULL
   AND bm.BOOK_ID IS NOT NULL
   AND b.ID IN ("#,
    );
    let mut separated = query.separated(", ");
    for book_id in book_ids {
        separated.push_bind(book_id);
    }
    separated.push_unseparated(") ORDER BY b.ID ASC");

    let rows = query.build().fetch_all(pool).await?;
    let contributors = load_contributors(pool, book_ids).await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let book_id = row.get::<String, _>("BOOK_ID");
            let created_date = row.get::<String, _>("CREATED_DATE");
            let created_date = created_date.trim();
            let metadata = KoboSyncBookMetadataRow {
                book_id: book_id.clone(),
                title: row.get::<String, _>("TITLE"),
                summary: row.get::<String, _>("SUMMARY"),
                release_date: row.get::<Option<String>, _>("RELEASE_DATE"),
                created_date: if created_date.is_empty() {
                    None
                } else {
                    Some(created_date.to_string())
                },
                language: row.get::<String, _>("LANGUAGE"),
                file_size: row.get::<i64, _>("FILE_SIZE").max(0) as u64,
                contributor_names: contributors.get(&book_id).cloned().unwrap_or_default(),
                isbn: row.get::<Option<String>, _>("ISBN"),
                publisher_name: row.get::<Option<String>, _>("PUBLISHER_NAME"),
                cover_image_id: row.get::<Option<String>, _>("COVER_IMAGE_ID"),
                series_id: row.get::<Option<String>, _>("SERIES_ID"),
                series_name: row.get::<Option<String>, _>("SERIES_NAME"),
                series_number: row.get::<Option<String>, _>("SERIES_NUMBER"),
                series_number_float: row.get::<Option<f64>, _>("SERIES_NUMBER_FLOAT"),
                oneshot: row.get::<bool, _>("ONESHOT"),
            };
            (metadata.book_id.clone(), metadata)
        })
        .collect())
}

async fn load_contributors(
    pool: &SqlitePool,
    book_ids: &[&str],
) -> Result<HashMap<String, Vec<String>>, sqlx::Error> {
    let mut query = QueryBuilder::<Sqlite>::new(
        r#"SELECT BOOK_ID, NAME
 FROM BOOK_METADATA_AUTHOR
 WHERE NAME IS NOT NULL
   AND TRIM(NAME) <> ''
   AND BOOK_ID IN ("#,
    );
    let mut separated = query.separated(", ");
    for book_id in book_ids {
        separated.push_bind(book_id);
    }
    separated.push_unseparated(") ORDER BY BOOK_ID ASC, NAME ASC");

    let mut contributors = HashMap::<String, Vec<String>>::new();
    for row in query.build().fetch_all(pool).await? {
        contributors
            .entry(row.get::<String, _>("BOOK_ID"))
            .or_default()
            .push(row.get::<String, _>("NAME"));
    }
    Ok(contributors)
}

async fn load_read_progress(
    pool: &SqlitePool,
    book_ids: &[&str],
    user_id: &str,
) -> Result<HashMap<String, ReadProgressRow>, sqlx::Error> {
    let mut query = QueryBuilder::<Sqlite>::new(
        r#"SELECT BOOK_ID, PAGE, COMPLETED, CREATED_DATE, LAST_MODIFIED_DATE, LOCATOR
 FROM READ_PROGRESS
 WHERE USER_ID = "#,
    );
    query.push_bind(user_id);
    query.push(" AND BOOK_ID IN (");
    let mut separated = query.separated(", ");
    for book_id in book_ids {
        separated.push_bind(book_id);
    }
    separated.push_unseparated(")");

    Ok(query
        .build()
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("BOOK_ID"),
                ReadProgressRow {
                    page: row.get::<i64, _>("PAGE"),
                    completed: row.get::<bool, _>("COMPLETED"),
                    created: row.get::<String, _>("CREATED_DATE"),
                    last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
                    locator: row
                        .try_get::<Option<Vec<u8>>, _>("LOCATOR")
                        .or_else(|_| row.try_get::<Option<Vec<u8>>, _>("locator"))
                        .ok()
                        .flatten(),
                },
            )
        })
        .collect())
}

fn sync_book_snapshot_from_metadata(
    book: &KoboSyncPointBook,
    metadata: &KoboSyncBookMetadataRow,
) -> KoboSyncBookSnapshot {
    KoboSyncBookSnapshot {
        id: book.book_id.clone(),
        title: metadata.title.clone(),
        summary: metadata.summary.clone(),
        release_date: metadata.release_date.clone(),
        language: metadata.language.clone(),
        file_size: metadata.file_size,
        page_count: 1,
        created: metadata
            .created_date
            .clone()
            .unwrap_or_else(|| book.created.clone()),
        last_modified: book.file_last_modified.clone(),
        contributor_names: metadata.contributor_names.clone(),
        isbn: metadata.isbn.clone(),
        publisher_name: metadata.publisher_name.clone(),
        cover_image_id: metadata.cover_image_id.clone(),
        series_id: metadata.series_id.clone(),
        series_name: metadata.series_name.clone(),
        series_number: metadata.series_number.clone(),
        series_number_float: metadata.series_number_float,
        oneshot: metadata.oneshot,
    }
}

fn progress_snapshot(row: &ReadProgressRow) -> KoboSyncReadProgressSnapshot {
    KoboSyncReadProgressSnapshot {
        page: row.page,
        completed: row.completed,
        created: row.created.clone(),
        last_modified: row.last_modified.clone(),
        locator: row.locator.clone(),
    }
}
