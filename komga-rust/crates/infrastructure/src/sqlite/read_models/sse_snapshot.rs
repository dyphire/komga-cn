use std::collections::HashMap;
use std::path::Path;

use sqlx::Row;

use crate::sqlite::connect_pool;

#[derive(Clone, Default)]
pub struct SseSnapshot {
    pub libraries: HashMap<String, LibrarySnapshot>,
    pub series: HashMap<String, SeriesSnapshot>,
    pub books: HashMap<String, BookSnapshot>,
    pub readlists: HashMap<String, ReadListSnapshot>,
    pub collections: HashMap<String, CollectionSnapshot>,
    pub thumbnails_book: HashMap<String, ThumbnailBookSnapshot>,
    pub thumbnails_series: HashMap<String, ThumbnailSnapshot>,
    pub thumbnails_collection: HashMap<String, ThumbnailCollectionSnapshot>,
    pub thumbnails_readlist: HashMap<String, ThumbnailReadListSnapshot>,
    pub read_progress: HashMap<String, String>,
    pub read_progress_series: HashMap<String, String>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct LibrarySnapshot {
    pub last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
pub struct SeriesSnapshot {
    pub library_id: String,
    pub last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
pub struct BookSnapshot {
    pub series_id: String,
    pub library_id: String,
    pub last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ReadListSnapshot {
    pub book_ids: Vec<String>,
    pub last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
pub struct CollectionSnapshot {
    pub series_ids: Vec<String>,
    pub last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ThumbnailBookSnapshot {
    pub book_id: String,
    pub series_id: String,
    pub selected: bool,
    pub last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ThumbnailSnapshot {
    pub selected: bool,
    pub last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ThumbnailReadListSnapshot {
    pub readlist_id: String,
    pub selected: bool,
    pub last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ThumbnailCollectionSnapshot {
    pub collection_id: String,
    pub selected: bool,
    pub last_modified: String,
}

pub async fn load_sse_snapshot(database_file: &Path, user_id: &str) -> SseSnapshot {
    let pool = match connect_pool(database_file, 1).await {
        Ok(pool) => pool,
        Err(_) => return SseSnapshot::default(),
    };

    let libraries_rows = sqlx::query(
        "SELECT ID, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM LIBRARY",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let libraries = libraries_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("ID"),
                LibrarySnapshot {
                    last_modified: row.get::<String, _>("LAST_MODIFIED"),
                },
            )
        })
        .collect::<HashMap<_, _>>();

    let series_rows = sqlx::query(
        "SELECT ID, LIBRARY_ID, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM SERIES \
         WHERE DELETED_DATE IS NULL",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let series = series_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("ID"),
                SeriesSnapshot {
                    library_id: row.get::<String, _>("LIBRARY_ID"),
                    last_modified: row.get::<String, _>("LAST_MODIFIED"),
                },
            )
        })
        .collect::<HashMap<_, _>>();

    let books_rows = sqlx::query(
        "SELECT ID, SERIES_ID, LIBRARY_ID, \
                COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM BOOK \
         WHERE DELETED_DATE IS NULL",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let books = books_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("ID"),
                BookSnapshot {
                    series_id: row.get::<String, _>("SERIES_ID"),
                    library_id: row.get::<String, _>("LIBRARY_ID"),
                    last_modified: row.get::<String, _>("LAST_MODIFIED"),
                },
            )
        })
        .collect::<HashMap<_, _>>();

    let readlist_rows = sqlx::query(
        "SELECT rl.ID AS ID, \
                COALESCE(rl.LAST_MODIFIED_DATE, rl.CREATED_DATE, '') AS LAST_MODIFIED, \
                rb.BOOK_ID AS BOOK_ID \
         FROM READLIST rl \
         LEFT JOIN READLIST_BOOK rb ON rb.READLIST_ID = rl.ID \
         ORDER BY rl.ID ASC, rb.NUMBER ASC, rb.BOOK_ID ASC",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let mut readlists = HashMap::<String, ReadListSnapshot>::new();
    for row in readlist_rows {
        let readlist_id = row.get::<String, _>("ID");
        let snapshot = readlists
            .entry(readlist_id)
            .or_insert_with(|| ReadListSnapshot {
                book_ids: Vec::new(),
                last_modified: row.get::<String, _>("LAST_MODIFIED"),
            });
        if let Ok(book_id) = row.try_get::<String, _>("BOOK_ID") {
            snapshot.book_ids.push(book_id);
        }
    }

    let collection_rows = sqlx::query(
        "SELECT c.ID AS ID, \
                COALESCE(c.LAST_MODIFIED_DATE, c.CREATED_DATE, '') AS LAST_MODIFIED, \
                cs.SERIES_ID AS SERIES_ID \
         FROM COLLECTION c \
         LEFT JOIN COLLECTION_SERIES cs ON cs.COLLECTION_ID = c.ID \
         ORDER BY c.ID ASC, cs.NUMBER ASC, cs.SERIES_ID ASC",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let mut collections = HashMap::<String, CollectionSnapshot>::new();
    for row in collection_rows {
        let collection_id = row.get::<String, _>("ID");
        let snapshot = collections
            .entry(collection_id)
            .or_insert_with(|| CollectionSnapshot {
                series_ids: Vec::new(),
                last_modified: row.get::<String, _>("LAST_MODIFIED"),
            });
        if let Ok(series_id) = row.try_get::<String, _>("SERIES_ID") {
            snapshot.series_ids.push(series_id);
        }
    }

    let read_progress_rows = sqlx::query(
        "SELECT BOOK_ID, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM READ_PROGRESS \
         WHERE USER_ID = ?",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let read_progress = read_progress_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("BOOK_ID"),
                row.get::<String, _>("LAST_MODIFIED"),
            )
        })
        .collect::<HashMap<_, _>>();

    let read_progress_series_rows = sqlx::query(
        "SELECT SERIES_ID, \
                COALESCE(LAST_MODIFIED_DATE, MOST_RECENT_READ_DATE, '') AS LAST_MODIFIED \
         FROM READ_PROGRESS_SERIES \
         WHERE USER_ID = ?",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let read_progress_series = read_progress_series_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("SERIES_ID"),
                row.get::<String, _>("LAST_MODIFIED"),
            )
        })
        .collect::<HashMap<_, _>>();

    let thumbnail_book_rows = sqlx::query(
        "SELECT tb.ID, \
                tb.BOOK_ID AS BOOK_ID, \
                COALESCE(b.SERIES_ID, '') AS SERIES_ID, \
                tb.SELECTED AS SELECTED, \
                COALESCE(tb.LAST_MODIFIED_DATE, tb.CREATED_DATE, '') AS LAST_MODIFIED \
         FROM THUMBNAIL_BOOK tb \
         LEFT JOIN BOOK b ON b.ID = tb.BOOK_ID \
         ORDER BY tb.BOOK_ID ASC, tb.SELECTED ASC, COALESCE(tb.LAST_MODIFIED_DATE, tb.CREATED_DATE, '') ASC, tb.ID ASC",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let thumbnails_book = thumbnail_book_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("ID"),
                ThumbnailBookSnapshot {
                    book_id: row.get::<String, _>("BOOK_ID"),
                    series_id: row.get::<String, _>("SERIES_ID"),
                    selected: row.get::<bool, _>("SELECTED"),
                    last_modified: row.get::<String, _>("LAST_MODIFIED"),
                },
            )
        })
        .collect::<HashMap<_, _>>();

    let thumbnail_series_rows = sqlx::query(
        "SELECT SERIES_ID AS ID, SELECTED, \
                COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM THUMBNAIL_SERIES \
         ORDER BY SERIES_ID ASC, SELECTED ASC, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') ASC, ID ASC",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let thumbnails_series = thumbnail_series_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("ID"),
                ThumbnailSnapshot {
                    selected: row.get::<bool, _>("SELECTED"),
                    last_modified: row.get::<String, _>("LAST_MODIFIED"),
                },
            )
        })
        .collect::<HashMap<_, _>>();

    let thumbnail_collection_rows = sqlx::query(
        "SELECT ID, COLLECTION_ID, SELECTED, \
                COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM THUMBNAIL_COLLECTION \
         ORDER BY COLLECTION_ID ASC, SELECTED ASC, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') ASC, ID ASC",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let thumbnails_collection = thumbnail_collection_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("ID"),
                ThumbnailCollectionSnapshot {
                    collection_id: row.get::<String, _>("COLLECTION_ID"),
                    selected: row.get::<bool, _>("SELECTED"),
                    last_modified: row.get::<String, _>("LAST_MODIFIED"),
                },
            )
        })
        .collect::<HashMap<_, _>>();

    let thumbnail_readlist_rows = sqlx::query(
        "SELECT ID, READLIST_ID, SELECTED, \
                COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM THUMBNAIL_READLIST \
         ORDER BY READLIST_ID ASC, SELECTED ASC, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') ASC, ID ASC",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let thumbnails_readlist = thumbnail_readlist_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("ID"),
                ThumbnailReadListSnapshot {
                    readlist_id: row.get::<String, _>("READLIST_ID"),
                    selected: row.get::<bool, _>("SELECTED"),
                    last_modified: row.get::<String, _>("LAST_MODIFIED"),
                },
            )
        })
        .collect::<HashMap<_, _>>();

    SseSnapshot {
        libraries,
        series,
        books,
        readlists,
        collections,
        thumbnails_book,
        thumbnails_series,
        thumbnails_collection,
        thumbnails_readlist,
        read_progress,
        read_progress_series,
    }
}
