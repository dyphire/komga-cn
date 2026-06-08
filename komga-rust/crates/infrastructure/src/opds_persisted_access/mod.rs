use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use async_trait::async_trait;
use komga_application::opds::OpdsPersistedPort;
use sqlx::{Row, SqlitePool};

use crate::database_handle::DatabaseHandle;
use crate::search::engine::SearchIndexEngine;
use crate::search::index_lifecycle::SearchEntityType;

mod collections;
mod records;

pub use collections::{
    load_collection, load_collection_books, load_collection_series, load_collections,
    load_publishers,
};
pub use records::{
    PersistedBookAuthorRecord, PersistedBookFeedRecord, PersistedBookSearchRecord,
    PersistedLibraryRecord, PersistedNamedRecord, PersistedReadlistBookRecord,
    PersistedReadlistRecord, PersistedSeriesBookRecord, PersistedSeriesRecord,
    PersistedSeriesSearchRecord,
};

use collections::unicode_collation_sort_key;
use records::{
    parsed_age_rating, parsed_book_author_records, parsed_book_tags, parsed_sharing_labels,
    placeholder_list,
};

const OPDS_SEARCH_GROUP_LIMIT: i64 = 20;

#[derive(Clone)]
pub struct OpdsPersistedAccess {
    db: DatabaseHandle,
    search: SearchIndexEngine,
}

impl OpdsPersistedAccess {
    pub fn new(db: DatabaseHandle, lucene_data_directory: PathBuf) -> Self {
        let search = SearchIndexEngine::read_only(db.read_pool().clone(), lucene_data_directory);
        Self { db, search }
    }
}

#[async_trait]
impl OpdsPersistedPort for OpdsPersistedAccess {
    async fn load_libraries(&self) -> Result<Vec<PersistedLibraryRecord>, String> {
        load_libraries(self.db.read_pool())
            .await
            .map_err(|error| error.to_string())
    }

    async fn load_library(
        &self,
        library_id: &str,
    ) -> Result<Option<PersistedLibraryRecord>, String> {
        load_library(self.db.read_pool(), library_id)
            .await
            .map_err(|error| error.to_string())
    }

    async fn load_readlists_for_library(
        &self,
        library_id: &str,
    ) -> Result<Vec<PersistedReadlistRecord>, String> {
        load_readlists_for_library(self.db.read_pool(), library_id)
            .await
            .map_err(|error| error.to_string())
    }

    async fn load_all_readlists(&self) -> Result<Vec<PersistedReadlistRecord>, String> {
        load_all_readlists(self.db.read_pool())
            .await
            .map_err(|error| error.to_string())
    }

    async fn load_series(&self, series_id: &str) -> Result<Option<PersistedSeriesRecord>, String> {
        load_series(self.db.read_pool(), series_id)
            .await
            .map_err(|error| error.to_string())
    }

    async fn load_series_books_paged(
        &self,
        series_id: &str,
        user_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<PersistedSeriesBookRecord>, String> {
        load_series_books_paged(self.db.read_pool(), series_id, user_id, offset, limit)
            .await
            .map_err(|error| error.to_string())
    }

    async fn load_series_tags(&self, series_id: &str) -> Result<Vec<String>, String> {
        load_series_tags(self.db.read_pool(), series_id)
            .await
            .map_err(|error| error.to_string())
    }

    async fn load_readlist(
        &self,
        readlist_id: &str,
    ) -> Result<Option<PersistedReadlistRecord>, String> {
        load_readlist(self.db.read_pool(), readlist_id)
            .await
            .map_err(|error| error.to_string())
    }

    async fn load_readlist_books(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<PersistedReadlistBookRecord>, String> {
        load_readlist_books(self.db.read_pool(), readlist_id)
            .await
            .map_err(|error| error.to_string())
    }

    async fn load_unified_search_results(
        &self,
        query: &str,
    ) -> Result<
        (
            Vec<PersistedSeriesSearchRecord>,
            Vec<PersistedBookSearchRecord>,
            Vec<PersistedNamedRecord>,
            Vec<PersistedNamedRecord>,
        ),
        String,
    > {
        let trimmed_query = query.trim();
        if trimmed_query.is_empty() {
            return load_blank_opds_search_results(self.db.read_pool()).await;
        }

        Ok((
            load_ranked_series_search_results(self.db.read_pool(), &self.search, trimmed_query)
                .await?,
            load_ranked_book_search_results(self.db.read_pool(), &self.search, trimmed_query)
                .await?,
            load_ranked_collection_search_results(self.db.read_pool(), &self.search, trimmed_query)
                .await?,
            load_ranked_readlist_search_results(self.db.read_pool(), &self.search, trimmed_query)
                .await?,
        ))
    }

    async fn load_publishers(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
    ) -> Result<Vec<String>, String> {
        load_publishers(self.db.read_pool(), allowed_library_ids)
            .await
            .map_err(|error| error.to_string())
    }

    async fn load_collections(
        &self,
        library_id: Option<&str>,
    ) -> Result<Vec<PersistedNamedRecord>, String> {
        load_collections(self.db.read_pool(), library_id)
            .await
            .map_err(|error| error.to_string())
    }

    async fn load_collection(
        &self,
        collection_id: &str,
    ) -> Result<Option<PersistedNamedRecord>, String> {
        load_collection(self.db.read_pool(), collection_id)
            .await
            .map_err(|error| error.to_string())
    }

    async fn load_collection_books(
        &self,
        collection_id: &str,
    ) -> Result<Vec<PersistedBookFeedRecord>, String> {
        load_collection_books(self.db.read_pool(), collection_id)
            .await
            .map_err(|error| error.to_string())
    }

    async fn load_collection_series(
        &self,
        collection_id: &str,
        ordered: bool,
    ) -> Result<Vec<PersistedSeriesRecord>, String> {
        load_collection_series(self.db.read_pool(), collection_id, ordered)
            .await
            .map_err(|error| error.to_string())
    }
}

async fn load_blank_opds_search_results(
    pool: &SqlitePool,
) -> Result<
    (
        Vec<PersistedSeriesSearchRecord>,
        Vec<PersistedBookSearchRecord>,
        Vec<PersistedNamedRecord>,
        Vec<PersistedNamedRecord>,
    ),
    String,
> {
    Ok((
        load_series_search_records_limited(pool, OPDS_SEARCH_GROUP_LIMIT)
            .await
            .map_err(|error| format!("load blank OPDS series search rows: {error}"))?,
        load_book_search_records_limited(pool, OPDS_SEARCH_GROUP_LIMIT)
            .await
            .map_err(|error| format!("load blank OPDS book search rows: {error}"))?,
        load_collection_search_records_limited(pool, OPDS_SEARCH_GROUP_LIMIT)
            .await
            .map_err(|error| format!("load blank OPDS collection search rows: {error}"))?,
        load_readlist_search_records_limited(pool, OPDS_SEARCH_GROUP_LIMIT)
            .await
            .map_err(|error| format!("load blank OPDS readlist search rows: {error}"))?,
    ))
}

async fn load_ranked_series_search_results(
    pool: &SqlitePool,
    search: &SearchIndexEngine,
    query: &str,
) -> Result<Vec<PersistedSeriesSearchRecord>, String> {
    let limit = load_series_search_count(pool)
        .await
        .map_err(|error| format!("load OPDS series search count: {error}"))?
        .max(1);
    let ids = search.search_ids_or_empty(query, SearchEntityType::Series, limit);
    ordered_series_search_rows(pool, &ids).await
}

async fn load_ranked_book_search_results(
    pool: &SqlitePool,
    search: &SearchIndexEngine,
    query: &str,
) -> Result<Vec<PersistedBookSearchRecord>, String> {
    let limit = load_book_search_count(pool)
        .await
        .map_err(|error| format!("load OPDS book search count: {error}"))?
        .max(1);
    let ids = search.search_ids_or_empty(query, SearchEntityType::Book, limit);
    ordered_book_search_rows(pool, &ids).await
}

async fn load_ranked_collection_search_results(
    pool: &SqlitePool,
    search: &SearchIndexEngine,
    query: &str,
) -> Result<Vec<PersistedNamedRecord>, String> {
    let limit = load_collection_search_count(pool)
        .await
        .map_err(|error| format!("load OPDS collection search count: {error}"))?
        .max(1);
    let ids = search.search_ids_or_empty(query, SearchEntityType::Collection, limit);
    ordered_collection_search_rows(pool, &ids).await
}

async fn load_ranked_readlist_search_results(
    pool: &SqlitePool,
    search: &SearchIndexEngine,
    query: &str,
) -> Result<Vec<PersistedNamedRecord>, String> {
    let limit = load_readlist_search_count(pool)
        .await
        .map_err(|error| format!("load OPDS readlist search count: {error}"))?
        .max(1);
    let ids = search.search_ids_or_empty(query, SearchEntityType::ReadList, limit);
    ordered_readlist_search_rows(pool, &ids).await
}

async fn ordered_series_search_rows(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<Vec<PersistedSeriesSearchRecord>, String> {
    let rows = load_series_search_records_by_ids(pool, ids)
        .await
        .map_err(|error| format!("load OPDS series search rows by ids: {error}"))?;
    let mut by_id = rows
        .into_iter()
        .map(|row| (row.id.clone(), row))
        .collect::<HashMap<_, _>>();
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}

async fn ordered_book_search_rows(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<Vec<PersistedBookSearchRecord>, String> {
    let rows = load_book_search_records_by_ids(pool, ids)
        .await
        .map_err(|error| format!("load OPDS book search rows by ids: {error}"))?;
    let mut by_id = rows
        .into_iter()
        .map(|row| (row.id.clone(), row))
        .collect::<HashMap<_, _>>();
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}

async fn ordered_collection_search_rows(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<Vec<PersistedNamedRecord>, String> {
    let rows = load_collection_search_records_by_ids(pool, ids)
        .await
        .map_err(|error| format!("load OPDS collection search rows by ids: {error}"))?;
    let mut by_id = rows
        .into_iter()
        .map(|row| (row.id.clone(), row))
        .collect::<HashMap<_, _>>();
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}

async fn ordered_readlist_search_rows(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<Vec<PersistedNamedRecord>, String> {
    let rows = load_readlist_search_records_by_ids(pool, ids)
        .await
        .map_err(|error| format!("load OPDS readlist search rows by ids: {error}"))?;
    let mut by_id = rows
        .into_iter()
        .map(|row| (row.id.clone(), row))
        .collect::<HashMap<_, _>>();
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}

pub async fn load_libraries(pool: &SqlitePool) -> Result<Vec<PersistedLibraryRecord>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT ID, NAME, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED
FROM LIBRARY"#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedLibraryRecord {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

pub async fn load_library(
    pool: &SqlitePool,
    library_id: &str,
) -> Result<Option<PersistedLibraryRecord>, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT ID, NAME, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED
FROM LIBRARY
WHERE ID = ?
LIMIT 1"#,
    )
    .bind(library_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| PersistedLibraryRecord {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
        last_modified: row.get::<String, _>("LAST_MODIFIED"),
    }))
}

pub async fn load_readlists_for_library(
    pool: &SqlitePool,
    library_id: &str,
) -> Result<Vec<PersistedReadlistRecord>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT DISTINCT rl.ID, rl.NAME, rl.ORDERED,
       COALESCE(rl.LAST_MODIFIED_DATE, rl.CREATED_DATE, '') AS LAST_MODIFIED
FROM READLIST rl
JOIN READLIST_BOOK rb ON rb.READLIST_ID = rl.ID
JOIN BOOK b ON b.ID = rb.BOOK_ID
WHERE b.LIBRARY_ID = ?
ORDER BY rl.NAME COLLATE NOCASE ASC, rl.ID ASC"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedReadlistRecord {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
            ordered: row.get::<bool, _>("ORDERED"),
        })
        .collect())
}

pub async fn load_all_readlists(
    pool: &SqlitePool,
) -> Result<Vec<PersistedReadlistRecord>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT
    ID,
    NAME,
    ORDERED,
    COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED
FROM READLIST
ORDER BY NAME COLLATE NOCASE ASC, ID ASC"#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedReadlistRecord {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
            ordered: row.get::<bool, _>("ORDERED"),
        })
        .collect())
}

pub async fn load_series(
    pool: &SqlitePool,
    series_id: &str,
) -> Result<Option<PersistedSeriesRecord>, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE,
       COALESCE(NULLIF(sm.SUMMARY, ''), bma.SUMMARY, '') AS SERIES_SUMMARY,
       COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
       COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS,
       COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED
FROM SERIES s
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN BOOK_METADATA_AGGREGATION bma ON bma.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
WHERE s.ID = ?
GROUP BY s.ID, s.LIBRARY_ID, TITLE, SERIES_SUMMARY, AGE_RATING, LAST_MODIFIED
LIMIT 1"#,
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| PersistedSeriesRecord {
        id: row.get::<String, _>("ID"),
        library_id: row.get::<String, _>("LIBRARY_ID"),
        title: row.get::<String, _>("TITLE"),
        summary: row.get::<String, _>("SERIES_SUMMARY"),
        age_rating: parsed_age_rating(&row),
        sharing_labels: parsed_sharing_labels(&row),
        last_modified: row.get::<String, _>("LAST_MODIFIED"),
    }))
}

pub async fn load_series_books_paged(
    pool: &SqlitePool,
    series_id: &str,
    user_id: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<PersistedSeriesBookRecord>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT b.ID, b.SERIES_ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE,
       COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE,
       COALESCE(bm.NUMBER, CAST(b.NUMBER AS TEXT), '') AS NUMBER,
       COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0) AS NUMBER_SORT,
       COALESCE(bm.SUMMARY, '') AS SUMMARY,
       COALESCE(bm.ISBN, '') AS ISBN,
       COALESCE((SELECT GROUP_CONCAT(NAME || char(31) || COALESCE(ROLE, ''), char(30))
                 FROM BOOK_METADATA_AUTHOR
                 WHERE BOOK_ID = b.ID), '') AS AUTHORS,
       COALESCE((SELECT GROUP_CONCAT(TAG, char(30))
                 FROM (SELECT DISTINCT TAG FROM BOOK_METADATA_TAG WHERE BOOK_ID = b.ID)), '') AS TAGS,
       b.NAME AS FILE_NAME, COALESCE(b.FILE_SIZE, 0) AS FILE_SIZE,
       COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
       m.STATUS AS MEDIA_STATUS,
       COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT,
       COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0) AS EPUB_DIVINA_COMPATIBLE,
       COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
       COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS,
       rp.PAGE AS LAST_READ,
       rp.READ_DATE AS LAST_READ_DATE,
       COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') AS LAST_MODIFIED,
       bm.RELEASE_DATE AS RELEASE_DATE
FROM BOOK b
LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
LEFT JOIN SERIES s ON s.ID = b.SERIES_ID
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID AND rp.USER_ID = ?
WHERE b.SERIES_ID = ?
AND b.DELETED_DATE IS NULL
AND COALESCE(m.STATUS, '') = 'READY'
GROUP BY b.ID, b.SERIES_ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME), COALESCE(sm.TITLE, s.NAME),
         COALESCE(bm.NUMBER, CAST(b.NUMBER AS TEXT), ''), COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0),
         COALESCE(bm.SUMMARY, ''), COALESCE(bm.ISBN, ''), b.NAME, COALESCE(b.FILE_SIZE, 0),
         COALESCE(m.MEDIA_TYPE, 'application/octet-stream'), m.STATUS,
         COALESCE(m.PAGE_COUNT, 0),
         COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0),
         COALESCE(sm.AGE_RATING, NULL),
         rp.PAGE, rp.READ_DATE,
         COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, ''),
         bm.RELEASE_DATE
ORDER BY COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0) ASC, b.ID ASC
LIMIT ?
OFFSET ?"#,
    )
    .bind(user_id)
    .bind(series_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedSeriesBookRecord {
            id: row.get::<String, _>("ID"),
            series_id: row.get::<String, _>("SERIES_ID"),
            title: row.get::<String, _>("TITLE"),
            series_title: row.get::<String, _>("SERIES_TITLE"),
            number: row.get::<String, _>("NUMBER"),
            number_sort: row.get::<f64, _>("NUMBER_SORT"),
            summary: row.get::<String, _>("SUMMARY"),
            isbn: row
                .try_get::<String, _>("ISBN")
                .ok()
                .filter(|value| !value.is_empty()),
            authors: parsed_book_author_records(&row),
            tags: parsed_book_tags(&row),
            file_name: row.get::<String, _>("FILE_NAME"),
            file_size: row.get::<i64, _>("FILE_SIZE"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            media_status: row.try_get::<String, _>("MEDIA_STATUS").ok(),
            page_count: row.get::<i64, _>("PAGE_COUNT"),
            epub_divina_compatible: row.get::<bool, _>("EPUB_DIVINA_COMPATIBLE"),
            last_read: row.try_get::<Option<i64>, _>("LAST_READ").ok().flatten(),
            last_read_date: row
                .try_get::<Option<String>, _>("LAST_READ_DATE")
                .ok()
                .flatten(),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: parsed_age_rating(&row),
            sharing_labels: parsed_sharing_labels(&row),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
            release_date: row.try_get::<String, _>("RELEASE_DATE").ok(),
        })
        .collect())
}

pub async fn load_series_tags(
    pool: &SqlitePool,
    series_id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT DISTINCT bt.TAG AS TAG
FROM BOOK_METADATA_TAG bt
LEFT JOIN BOOK b ON bt.BOOK_ID = b.ID
WHERE b.SERIES_ID = ?
ORDER BY bt.TAG COLLATE NOCASE ASC, bt.TAG ASC"#,
    )
    .bind(series_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("TAG"))
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect())
}

pub async fn load_readlist(
    pool: &SqlitePool,
    readlist_id: &str,
) -> Result<Option<PersistedReadlistRecord>, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT ID, NAME, ORDERED, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED
FROM READLIST
WHERE ID = ?
LIMIT 1"#,
    )
    .bind(readlist_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| PersistedReadlistRecord {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
        last_modified: row.get::<String, _>("LAST_MODIFIED"),
        ordered: row.get::<bool, _>("ORDERED"),
    }))
}

pub async fn load_readlist_books(
    pool: &SqlitePool,
    readlist_id: &str,
) -> Result<Vec<PersistedReadlistBookRecord>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT b.ID, b.SERIES_ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE,
       COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE,
       COALESCE(bm.NUMBER, CAST(b.NUMBER AS TEXT), '') AS NUMBER,
       COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0) AS NUMBER_SORT,
       COALESCE(bm.SUMMARY, '') AS SUMMARY,
       COALESCE(bm.ISBN, '') AS ISBN,
       COALESCE((SELECT GROUP_CONCAT(NAME || char(31) || COALESCE(ROLE, ''), char(30))
                 FROM BOOK_METADATA_AUTHOR
                 WHERE BOOK_ID = b.ID), '') AS AUTHORS,
       COALESCE((SELECT GROUP_CONCAT(TAG, char(30))
                 FROM (SELECT DISTINCT TAG FROM BOOK_METADATA_TAG WHERE BOOK_ID = b.ID)), '') AS TAGS,
       b.NAME AS FILE_NAME, COALESCE(b.FILE_SIZE, 0) AS FILE_SIZE,
       COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
       m.STATUS AS MEDIA_STATUS,
       COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT,
       COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0) AS EPUB_DIVINA_COMPATIBLE,
       COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
       COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS,
       COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') AS LAST_MODIFIED,
       bm.RELEASE_DATE AS RELEASE_DATE
FROM READLIST_BOOK rb
JOIN BOOK b ON b.ID = rb.BOOK_ID
LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
LEFT JOIN SERIES s ON s.ID = b.SERIES_ID
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
WHERE rb.READLIST_ID = ?
AND b.DELETED_DATE IS NULL
GROUP BY b.ID, b.SERIES_ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME),
         COALESCE(sm.TITLE, s.NAME), COALESCE(bm.NUMBER, CAST(b.NUMBER AS TEXT), ''),
         COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0), COALESCE(bm.SUMMARY, ''), COALESCE(bm.ISBN, ''),
         b.NAME, COALESCE(b.FILE_SIZE, 0),
         COALESCE(m.MEDIA_TYPE, 'application/octet-stream'), m.STATUS,
         COALESCE(m.PAGE_COUNT, 0), COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0),
         COALESCE(sm.AGE_RATING, NULL),
         COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, ''),
         bm.RELEASE_DATE
ORDER BY rb.NUMBER ASC"#,
    )
    .bind(readlist_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedReadlistBookRecord {
            id: row.get::<String, _>("ID"),
            series_id: row.get::<String, _>("SERIES_ID"),
            title: row.get::<String, _>("TITLE"),
            series_title: row.get::<String, _>("SERIES_TITLE"),
            number: row.get::<String, _>("NUMBER"),
            number_sort: row.get::<f64, _>("NUMBER_SORT"),
            summary: row.get::<String, _>("SUMMARY"),
            isbn: row
                .try_get::<String, _>("ISBN")
                .ok()
                .filter(|value| !value.is_empty()),
            authors: parsed_book_author_records(&row),
            tags: parsed_book_tags(&row),
            file_name: row.get::<String, _>("FILE_NAME"),
            file_size: row.get::<i64, _>("FILE_SIZE"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            media_status: row.try_get::<String, _>("MEDIA_STATUS").ok(),
            page_count: row.get::<i64, _>("PAGE_COUNT"),
            epub_divina_compatible: row.get::<bool, _>("EPUB_DIVINA_COMPATIBLE"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: parsed_age_rating(&row),
            sharing_labels: parsed_sharing_labels(&row),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
            release_date: row.try_get::<String, _>("RELEASE_DATE").ok(),
        })
        .collect())
}

pub async fn load_series_search_count(pool: &SqlitePool) -> Result<usize, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT COUNT(*) AS TOTAL
FROM SERIES s
WHERE s.DELETED_DATE IS NULL"#,
    )
    .fetch_one(pool)
    .await?;
    Ok(row.get::<i64, _>("TOTAL") as usize)
}

pub async fn load_book_search_count(pool: &SqlitePool) -> Result<usize, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT COUNT(*) AS TOTAL
FROM BOOK b
WHERE b.DELETED_DATE IS NULL"#,
    )
    .fetch_one(pool)
    .await?;
    Ok(row.get::<i64, _>("TOTAL") as usize)
}

pub async fn load_collection_search_count(pool: &SqlitePool) -> Result<usize, sqlx::Error> {
    let row = sqlx::query("SELECT COUNT(*) AS TOTAL FROM COLLECTION")
        .fetch_one(pool)
        .await?;
    Ok(row.get::<i64, _>("TOTAL") as usize)
}

pub async fn load_readlist_search_count(pool: &SqlitePool) -> Result<usize, sqlx::Error> {
    let row = sqlx::query("SELECT COUNT(*) AS TOTAL FROM READLIST")
        .fetch_one(pool)
        .await?;
    Ok(row.get::<i64, _>("TOTAL") as usize)
}

pub async fn load_series_search_records_by_ids(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<Vec<PersistedSeriesSearchRecord>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(vec![]);
    }

    let sql = format!(
        r#"SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE,
       COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
       COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS,
       COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED
FROM SERIES s
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
WHERE s.DELETED_DATE IS NULL
AND COALESCE(s.ONESHOT, 0) = 0
AND s.ID IN ({})
GROUP BY s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME),
         COALESCE(sm.AGE_RATING, NULL),
         COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '')"#,
        placeholder_list(ids.len())
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
    for id in ids {
        query = query.bind(id);
    }

    let rows = query.fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| PersistedSeriesSearchRecord {
            id: row.get::<String, _>("ID"),
            title: row.get::<String, _>("TITLE"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: parsed_age_rating(&row),
            sharing_labels: parsed_sharing_labels(&row),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

pub async fn load_book_search_records_by_ids(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<Vec<PersistedBookSearchRecord>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(vec![]);
    }

    let sql = format!(
        r#"SELECT b.ID, b.SERIES_ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE,
       COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE,
       COALESCE(bm.NUMBER, CAST(b.NUMBER AS TEXT), '') AS NUMBER,
       COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0) AS NUMBER_SORT,
       COALESCE(bm.SUMMARY, '') AS SUMMARY,
       COALESCE(bm.ISBN, '') AS ISBN,
       COALESCE((SELECT GROUP_CONCAT(NAME || char(31) || COALESCE(ROLE, ''), char(30))
                 FROM BOOK_METADATA_AUTHOR
                 WHERE BOOK_ID = b.ID), '') AS AUTHORS,
       COALESCE((SELECT GROUP_CONCAT(TAG, char(30))
                 FROM (SELECT DISTINCT TAG FROM BOOK_METADATA_TAG WHERE BOOK_ID = b.ID)), '') AS TAGS,
       b.NAME AS FILE_NAME, COALESCE(b.FILE_SIZE, 0) AS FILE_SIZE,
       COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
       COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT,
       COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0) AS EPUB_DIVINA_COMPATIBLE,
       COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
       COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS,
       COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') AS LAST_MODIFIED,
       bm.RELEASE_DATE AS RELEASE_DATE
FROM BOOK b
LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
LEFT JOIN SERIES s ON s.ID = b.SERIES_ID
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
WHERE b.DELETED_DATE IS NULL
AND COALESCE(m.STATUS, '') = 'READY'
AND b.ID IN ({})
GROUP BY b.ID, b.SERIES_ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME),
         COALESCE(sm.TITLE, s.NAME),
         COALESCE(bm.NUMBER, CAST(b.NUMBER AS TEXT), ''),
         COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0),
         COALESCE(bm.SUMMARY, ''), COALESCE(bm.ISBN, ''), b.NAME,
         COALESCE(b.FILE_SIZE, 0), COALESCE(m.MEDIA_TYPE, 'application/octet-stream'),
         COALESCE(m.PAGE_COUNT, 0), COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0),
         COALESCE(sm.AGE_RATING, NULL),
         COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, ''), bm.RELEASE_DATE"#,
        placeholder_list(ids.len())
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
    for id in ids {
        query = query.bind(id);
    }

    let rows = query.fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| PersistedBookSearchRecord {
            id: row.get::<String, _>("ID"),
            series_id: row.get::<String, _>("SERIES_ID"),
            title: row.get::<String, _>("TITLE"),
            series_title: row.get::<String, _>("SERIES_TITLE"),
            number: row.get::<String, _>("NUMBER"),
            number_sort: row.get::<f64, _>("NUMBER_SORT"),
            summary: row.get::<String, _>("SUMMARY"),
            isbn: row
                .try_get::<String, _>("ISBN")
                .ok()
                .filter(|value| !value.is_empty()),
            authors: parsed_book_author_records(&row),
            tags: parsed_book_tags(&row),
            file_name: row.get::<String, _>("FILE_NAME"),
            file_size: row.get::<i64, _>("FILE_SIZE"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            page_count: row.get::<i64, _>("PAGE_COUNT"),
            epub_divina_compatible: row.get::<bool, _>("EPUB_DIVINA_COMPATIBLE"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: parsed_age_rating(&row),
            sharing_labels: parsed_sharing_labels(&row),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
            release_date: row.try_get::<String, _>("RELEASE_DATE").ok(),
        })
        .collect())
}

pub async fn load_collection_search_records_by_ids(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<Vec<PersistedNamedRecord>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(vec![]);
    }

    let sql = format!(
        r#"SELECT ID, NAME
FROM COLLECTION
WHERE ID IN ({})"#,
        placeholder_list(ids.len())
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
    for id in ids {
        query = query.bind(id);
    }

    Ok(query
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| PersistedNamedRecord {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            last_modified: row
                .try_get::<String, _>("LAST_MODIFIED")
                .unwrap_or_default(),
            ordered: false,
        })
        .collect())
}

pub async fn load_readlist_search_records_by_ids(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<Vec<PersistedNamedRecord>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(vec![]);
    }

    let sql = format!(
        r#"SELECT ID, NAME
FROM READLIST
WHERE ID IN ({})"#,
        placeholder_list(ids.len())
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
    for id in ids {
        query = query.bind(id);
    }

    let rows = query.fetch_all(pool).await?;
    let mut records = rows
        .into_iter()
        .map(|row| PersistedNamedRecord {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            last_modified: row
                .try_get::<String, _>("LAST_MODIFIED")
                .unwrap_or_default(),
            ordered: row.try_get::<bool, _>("ORDERED").unwrap_or(false),
        })
        .collect::<Vec<_>>();

    records.sort_by_cached_key(|record| unicode_collation_sort_key(record.name.as_str()));

    Ok(records)
}

pub async fn load_series_search_records_limited(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<PersistedSeriesSearchRecord>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE,
       COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
       COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS,
       COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED
FROM SERIES s
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
WHERE s.DELETED_DATE IS NULL
AND COALESCE(s.ONESHOT, 0) = 0
GROUP BY s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME),
         COALESCE(sm.AGE_RATING, NULL),
         COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '')
ORDER BY COALESCE(sm.TITLE, s.NAME) COLLATE NOCASE ASC, s.ID ASC
LIMIT ?"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| PersistedSeriesSearchRecord {
            id: row.get::<String, _>("ID"),
            title: row.get::<String, _>("TITLE"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: parsed_age_rating(&row),
            sharing_labels: parsed_sharing_labels(&row),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

pub async fn load_book_search_records_limited(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<PersistedBookSearchRecord>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT b.ID, b.SERIES_ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE,
       COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE,
       COALESCE(bm.NUMBER, CAST(b.NUMBER AS TEXT), '') AS NUMBER,
       COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0) AS NUMBER_SORT,
       COALESCE(bm.SUMMARY, '') AS SUMMARY,
       COALESCE(bm.ISBN, '') AS ISBN,
       COALESCE((SELECT GROUP_CONCAT(NAME || char(31) || COALESCE(ROLE, ''), char(30))
                 FROM BOOK_METADATA_AUTHOR
                 WHERE BOOK_ID = b.ID), '') AS AUTHORS,
       COALESCE((SELECT GROUP_CONCAT(TAG, char(30))
                 FROM (SELECT DISTINCT TAG FROM BOOK_METADATA_TAG WHERE BOOK_ID = b.ID)), '') AS TAGS,
       b.NAME AS FILE_NAME, COALESCE(b.FILE_SIZE, 0) AS FILE_SIZE,
       COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
       COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT,
       COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0) AS EPUB_DIVINA_COMPATIBLE,
       COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
       COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS,
       COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') AS LAST_MODIFIED,
       bm.RELEASE_DATE AS RELEASE_DATE
FROM BOOK b
LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
LEFT JOIN SERIES s ON s.ID = b.SERIES_ID
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
WHERE b.DELETED_DATE IS NULL
AND COALESCE(m.STATUS, '') = 'READY'
GROUP BY b.ID, b.SERIES_ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME),
         COALESCE(sm.TITLE, s.NAME),
         COALESCE(bm.NUMBER, CAST(b.NUMBER AS TEXT), ''),
         COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0),
         COALESCE(bm.SUMMARY, ''), COALESCE(bm.ISBN, ''), b.NAME,
         COALESCE(b.FILE_SIZE, 0), COALESCE(m.MEDIA_TYPE, 'application/octet-stream'),
         COALESCE(m.PAGE_COUNT, 0), COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0),
         COALESCE(sm.AGE_RATING, NULL),
         COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, ''), bm.RELEASE_DATE
ORDER BY COALESCE(bm.TITLE, b.NAME) COLLATE NOCASE ASC, b.ID ASC
LIMIT ?"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| PersistedBookSearchRecord {
            id: row.get::<String, _>("ID"),
            series_id: row.get::<String, _>("SERIES_ID"),
            title: row.get::<String, _>("TITLE"),
            series_title: row.get::<String, _>("SERIES_TITLE"),
            number: row.get::<String, _>("NUMBER"),
            number_sort: row.get::<f64, _>("NUMBER_SORT"),
            summary: row.get::<String, _>("SUMMARY"),
            isbn: row
                .try_get::<String, _>("ISBN")
                .ok()
                .filter(|value| !value.is_empty()),
            authors: parsed_book_author_records(&row),
            tags: parsed_book_tags(&row),
            file_name: row.get::<String, _>("FILE_NAME"),
            file_size: row.get::<i64, _>("FILE_SIZE"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            page_count: row.get::<i64, _>("PAGE_COUNT"),
            epub_divina_compatible: row.get::<bool, _>("EPUB_DIVINA_COMPATIBLE"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: parsed_age_rating(&row),
            sharing_labels: parsed_sharing_labels(&row),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
            release_date: row.try_get::<String, _>("RELEASE_DATE").ok(),
        })
        .collect())
}

pub async fn load_collection_search_records_limited(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<PersistedNamedRecord>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT ID, NAME
FROM COLLECTION
ORDER BY NAME COLLATE NOCASE ASC, ID ASC
LIMIT ?"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    let mut records = rows
        .into_iter()
        .map(|row| PersistedNamedRecord {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            last_modified: row
                .try_get::<String, _>("LAST_MODIFIED")
                .unwrap_or_default(),
            ordered: row.try_get::<bool, _>("ORDERED").unwrap_or(false),
        })
        .collect::<Vec<_>>();

    records.sort_by_cached_key(|record| unicode_collation_sort_key(record.name.as_str()));

    Ok(records)
}

pub async fn load_readlist_search_records_limited(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<PersistedNamedRecord>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT ID, NAME
FROM READLIST
ORDER BY NAME COLLATE NOCASE ASC, ID ASC
LIMIT ?"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| PersistedNamedRecord {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            last_modified: row
                .try_get::<String, _>("LAST_MODIFIED")
                .unwrap_or_default(),
            ordered: row.try_get::<bool, _>("ORDERED").unwrap_or(false),
        })
        .collect())
}
