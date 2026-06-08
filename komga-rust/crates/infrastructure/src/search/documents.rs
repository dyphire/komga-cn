use sqlx::Row;

use crate::search::index_lifecycle::{
    SearchDocument, SearchEntityType, SearchField, SearchFieldEntry,
};

const AUTHOR_ROLE_DELIMITER: &str = "::";

fn search_field(field: SearchField, value: String) -> SearchFieldEntry {
    SearchFieldEntry::new(field, value)
}

fn search_fields(field: SearchField, values: String) -> Vec<SearchFieldEntry> {
    values
        .split('|')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| SearchFieldEntry::new(field, value))
        .collect()
}

pub(super) async fn load_rebuild_search_documents(
    pool: sqlx::SqlitePool,
) -> Result<Vec<SearchDocument>, String> {
    let mut docs = load_all_book_search_documents(pool.clone()).await?;
    docs.extend(load_all_series_search_documents(pool.clone()).await?);
    docs.extend(load_all_collection_search_documents(pool.clone()).await?);
    docs.extend(load_all_readlist_search_documents(pool).await?);
    Ok(docs)
}

pub(super) async fn load_rebuild_search_documents_for_entities(
    pool: sqlx::SqlitePool,
    entity_types: &[SearchEntityType],
) -> Result<Vec<SearchDocument>, String> {
    let mut docs = Vec::new();
    for entity_type in entity_types {
        match entity_type {
            SearchEntityType::Book => {
                docs.extend(load_all_book_search_documents(pool.clone()).await?)
            }
            SearchEntityType::Series => {
                docs.extend(load_all_series_search_documents(pool.clone()).await?)
            }
            SearchEntityType::Collection => {
                docs.extend(load_all_collection_search_documents(pool.clone()).await?)
            }
            SearchEntityType::ReadList => {
                docs.extend(load_all_readlist_search_documents(pool.clone()).await?)
            }
        }
    }
    Ok(docs)
}

pub(super) async fn load_book_search_document(
    pool: sqlx::SqlitePool,
    book_id: &str,
) -> Result<Option<SearchDocument>, String> {
    let row = sqlx::query(
        r#"SELECT
             b.ID AS ID,
             COALESCE(bm.TITLE, b.NAME) AS TITLE,
             COALESCE(bm.ISBN, '') AS ISBN,
             COALESCE(m.STATUS, '') AS MEDIA_STATUS,
             CASE WHEN b.oneshot = 1 THEN COALESCE(sm.PUBLISHER, '') ELSE '' END AS ONESHOT_PUBLISHER,
             CASE WHEN b.oneshot = 1 THEN COALESCE(sm.STATUS, '') ELSE '' END AS ONESHOT_STATUS,
             CASE WHEN b.oneshot = 1 THEN COALESCE(sm.READING_DIRECTION, '') ELSE '' END AS ONESHOT_READING_DIRECTION,
             CASE WHEN b.oneshot = 1 THEN COALESCE(CAST(sm.AGE_RATING AS TEXT), '') ELSE '' END AS ONESHOT_AGE_RATING,
             CASE WHEN b.oneshot = 1 THEN COALESCE(sm.LANGUAGE, '') ELSE '' END AS ONESHOT_LANGUAGE,
             CASE WHEN b.oneshot = 1 THEN COALESCE((SELECT GROUP_CONCAT(g.GENRE, '|') FROM SERIES_METADATA_GENRE g WHERE g.SERIES_ID = s.ID), '') ELSE '' END AS ONESHOT_GENRES,
             CASE WHEN b.oneshot = 1 THEN COALESCE((SELECT GROUP_CONCAT(sh.LABEL, '|') FROM SERIES_METADATA_SHARING sh WHERE sh.SERIES_ID = s.ID), '') ELSE '' END AS ONESHOT_SHARING_LABELS,
             COALESCE((SELECT GROUP_CONCAT(bt.TAG, '|') FROM BOOK_METADATA_TAG bt WHERE bt.BOOK_ID = b.ID), '') AS BOOK_TAGS,
             COALESCE((SELECT GROUP_CONCAT(ba.NAME, '|') FROM BOOK_METADATA_AUTHOR ba WHERE ba.BOOK_ID = b.ID), '') AS AUTHORS,
             COALESCE((SELECT GROUP_CONCAT(COALESCE(ba.ROLE, '') || '::' || ba.NAME, '|') FROM BOOK_METADATA_AUTHOR ba WHERE ba.BOOK_ID = b.ID), '') AS AUTHOR_ROLES,
             COALESCE(STRFTIME('%Y', bm.RELEASE_DATE), '') AS RELEASE_YEAR,
             CASE WHEN b.DELETED_DATE IS NULL THEN 'false' ELSE 'true' END AS DELETED,
             CASE WHEN b.oneshot = 1 THEN 'true' ELSE 'false' END AS ONESHOT
          FROM BOOK b
          JOIN SERIES s ON s.ID = b.SERIES_ID
          LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
          LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
          LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
          WHERE b.ID = ?
          LIMIT 1
         "#,
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("failed to load BOOK row for search upsert: {error}"))?;

    Ok(row.map(build_book_document))
}

pub(super) async fn load_oneshot_book_search_documents(
    pool: sqlx::SqlitePool,
    series_id: &str,
) -> Result<Vec<SearchDocument>, String> {
    let rows = sqlx::query(
        r#"SELECT
             b.ID AS ID,
             COALESCE(bm.TITLE, b.NAME) AS TITLE,
             COALESCE(bm.ISBN, '') AS ISBN,
             COALESCE(m.STATUS, '') AS MEDIA_STATUS,
             CASE WHEN b.oneshot = 1 THEN COALESCE(sm.PUBLISHER, '') ELSE '' END AS ONESHOT_PUBLISHER,
             CASE WHEN b.oneshot = 1 THEN COALESCE(sm.STATUS, '') ELSE '' END AS ONESHOT_STATUS,
             CASE WHEN b.oneshot = 1 THEN COALESCE(sm.READING_DIRECTION, '') ELSE '' END AS ONESHOT_READING_DIRECTION,
             CASE WHEN b.oneshot = 1 THEN COALESCE(CAST(sm.AGE_RATING AS TEXT), '') ELSE '' END AS ONESHOT_AGE_RATING,
             CASE WHEN b.oneshot = 1 THEN COALESCE(sm.LANGUAGE, '') ELSE '' END AS ONESHOT_LANGUAGE,
             CASE WHEN b.oneshot = 1 THEN COALESCE((SELECT GROUP_CONCAT(g.GENRE, '|') FROM SERIES_METADATA_GENRE g WHERE g.SERIES_ID = s.ID), '') ELSE '' END AS ONESHOT_GENRES,
             CASE WHEN b.oneshot = 1 THEN COALESCE((SELECT GROUP_CONCAT(sh.LABEL, '|') FROM SERIES_METADATA_SHARING sh WHERE sh.SERIES_ID = s.ID), '') ELSE '' END AS ONESHOT_SHARING_LABELS,
             COALESCE((SELECT GROUP_CONCAT(bt.TAG, '|') FROM BOOK_METADATA_TAG bt WHERE bt.BOOK_ID = b.ID), '') AS BOOK_TAGS,
             COALESCE((SELECT GROUP_CONCAT(ba.NAME, '|') FROM BOOK_METADATA_AUTHOR ba WHERE ba.BOOK_ID = b.ID), '') AS AUTHORS,
             COALESCE((SELECT GROUP_CONCAT(COALESCE(ba.ROLE, '') || '::' || ba.NAME, '|') FROM BOOK_METADATA_AUTHOR ba WHERE ba.BOOK_ID = b.ID), '') AS AUTHOR_ROLES,
             COALESCE(STRFTIME('%Y', bm.RELEASE_DATE), '') AS RELEASE_YEAR,
             CASE WHEN b.DELETED_DATE IS NULL THEN 'false' ELSE 'true' END AS DELETED,
             CASE WHEN b.oneshot = 1 THEN 'true' ELSE 'false' END AS ONESHOT
          FROM BOOK b
          JOIN SERIES s ON s.ID = b.SERIES_ID
          LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
          LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
          LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
          WHERE b.SERIES_ID = ? AND b.oneshot = 1
         "#,
    )
    .bind(series_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("failed to load oneshot BOOK rows for search upsert: {error}"))?;

    Ok(rows.into_iter().map(build_book_document).collect())
}

pub(super) async fn load_series_search_document(
    pool: sqlx::SqlitePool,
    series_id: &str,
) -> Result<Option<SearchDocument>, String> {
    let row = sqlx::query(
        r#"SELECT
             s.ID AS ID,
             COALESCE(sm.TITLE, s.NAME) AS TITLE,
             COALESCE(sm.PUBLISHER, '') AS PUBLISHER,
             COALESCE(sm.STATUS, '') AS STATUS,
             COALESCE(sm.READING_DIRECTION, '') AS READING_DIRECTION,
             COALESCE(CAST(sm.AGE_RATING AS TEXT), '') AS AGE_RATING,
             COALESCE(sm.LANGUAGE, '') AS LANGUAGE,
             COALESCE(STRFTIME('%Y', bma.RELEASE_DATE), '') AS RELEASE_YEAR,
             CASE WHEN s.DELETED_DATE IS NULL THEN 'false' ELSE 'true' END AS DELETED,
             CASE WHEN s.oneshot = 1 THEN 'true' ELSE 'false' END AS ONESHOT,
             COALESCE(CAST(sm.TOTAL_BOOK_COUNT AS TEXT), '') AS TOTAL_BOOK_COUNT,
             COALESCE(CAST(s.BOOK_COUNT AS TEXT), '') AS BOOK_COUNT,
             CASE
                 WHEN sm.TOTAL_BOOK_COUNT IS NOT NULL
                      AND s.BOOK_COUNT IS NOT NULL
                      AND sm.TOTAL_BOOK_COUNT = s.BOOK_COUNT THEN 'true'
                 ELSE ''
             END AS COMPLETE,
             COALESCE((SELECT GROUP_CONCAT(st.TAG, '|') FROM SERIES_METADATA_TAG st WHERE st.SERIES_ID = s.ID), '') AS SERIES_TAGS,
             COALESCE((SELECT GROUP_CONCAT(bmat.TAG, '|') FROM BOOK_METADATA_AGGREGATION_TAG bmat WHERE bmat.SERIES_ID = s.ID), '') AS BOOK_TAGS,
             COALESCE((SELECT GROUP_CONCAT(sg.GENRE, '|') FROM SERIES_METADATA_GENRE sg WHERE sg.SERIES_ID = s.ID), '') AS GENRES,
             COALESCE((SELECT GROUP_CONCAT(ss.LABEL, '|') FROM SERIES_METADATA_SHARING ss WHERE ss.SERIES_ID = s.ID), '') AS SHARING_LABELS,
             COALESCE((SELECT GROUP_CONCAT(baa.NAME, '|') FROM BOOK_METADATA_AGGREGATION_AUTHOR baa WHERE baa.SERIES_ID = s.ID), '') AS AUTHORS,
             COALESCE((SELECT GROUP_CONCAT(COALESCE(baa.ROLE, '') || '::' || baa.NAME, '|') FROM BOOK_METADATA_AGGREGATION_AUTHOR baa WHERE baa.SERIES_ID = s.ID), '') AS AUTHOR_ROLES,
             COALESCE(sm.TITLE_SORT, '') AS TITLE_SORT,
             COALESCE((SELECT GROUP_CONCAT(sat.TITLE, '|') FROM SERIES_METADATA_ALTERNATE_TITLE sat WHERE sat.SERIES_ID = s.ID), '') AS ALTERNATE_TITLES
          FROM SERIES s
          LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
          LEFT JOIN BOOK_METADATA_AGGREGATION bma ON bma.SERIES_ID = s.ID
          WHERE s.ID = ?
          LIMIT 1
         "#,
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("failed to load SERIES row for search upsert: {error}"))?;

    Ok(row.map(build_series_document))
}

pub(super) async fn load_collection_search_document(
    pool: sqlx::SqlitePool,
    collection_id: &str,
) -> Result<Option<SearchDocument>, String> {
    let row = sqlx::query("SELECT ID, NAME FROM COLLECTION WHERE ID = ? LIMIT 1")
        .bind(collection_id)
        .fetch_optional(&pool)
        .await
        .map_err(|error| format!("failed to load COLLECTION row for search upsert: {error}"))?;

    Ok(row.map(|row| build_named_document(row, SearchEntityType::Collection)))
}

pub(super) async fn load_readlist_search_document(
    pool: sqlx::SqlitePool,
    readlist_id: &str,
) -> Result<Option<SearchDocument>, String> {
    let row = sqlx::query("SELECT ID, NAME FROM READLIST WHERE ID = ? LIMIT 1")
        .bind(readlist_id)
        .fetch_optional(&pool)
        .await
        .map_err(|error| format!("failed to load READLIST row for search upsert: {error}"))?;

    Ok(row.map(|row| build_named_document(row, SearchEntityType::ReadList)))
}

async fn load_all_book_search_documents(
    pool: sqlx::SqlitePool,
) -> Result<Vec<SearchDocument>, String> {
    let book_rows = sqlx::query(
        r#"SELECT
             b.ID AS ID,
             COALESCE(bm.TITLE, b.NAME) AS TITLE,
             COALESCE(bm.ISBN, '') AS ISBN,
             COALESCE(m.STATUS, '') AS MEDIA_STATUS,
             CASE WHEN b.oneshot = 1 THEN COALESCE(sm.PUBLISHER, '') ELSE '' END AS ONESHOT_PUBLISHER,
             CASE WHEN b.oneshot = 1 THEN COALESCE(sm.STATUS, '') ELSE '' END AS ONESHOT_STATUS,
             CASE WHEN b.oneshot = 1 THEN COALESCE(sm.READING_DIRECTION, '') ELSE '' END AS ONESHOT_READING_DIRECTION,
             CASE WHEN b.oneshot = 1 THEN COALESCE(CAST(sm.AGE_RATING AS TEXT), '') ELSE '' END AS ONESHOT_AGE_RATING,
             CASE WHEN b.oneshot = 1 THEN COALESCE(sm.LANGUAGE, '') ELSE '' END AS ONESHOT_LANGUAGE,
             CASE WHEN b.oneshot = 1 THEN COALESCE((SELECT GROUP_CONCAT(g.GENRE, '|') FROM SERIES_METADATA_GENRE g WHERE g.SERIES_ID = s.ID), '') ELSE '' END AS ONESHOT_GENRES,
             CASE WHEN b.oneshot = 1 THEN COALESCE((SELECT GROUP_CONCAT(sh.LABEL, '|') FROM SERIES_METADATA_SHARING sh WHERE sh.SERIES_ID = s.ID), '') ELSE '' END AS ONESHOT_SHARING_LABELS,
             COALESCE((SELECT GROUP_CONCAT(bt.TAG, '|') FROM BOOK_METADATA_TAG bt WHERE bt.BOOK_ID = b.ID), '') AS BOOK_TAGS,
             COALESCE((SELECT GROUP_CONCAT(ba.NAME, '|') FROM BOOK_METADATA_AUTHOR ba WHERE ba.BOOK_ID = b.ID), '') AS AUTHORS,
             COALESCE((SELECT GROUP_CONCAT(COALESCE(ba.ROLE, '') || '::' || ba.NAME, '|') FROM BOOK_METADATA_AUTHOR ba WHERE ba.BOOK_ID = b.ID), '') AS AUTHOR_ROLES,
             COALESCE(STRFTIME('%Y', bm.RELEASE_DATE), '') AS RELEASE_YEAR,
             CASE WHEN b.DELETED_DATE IS NULL THEN 'false' ELSE 'true' END AS DELETED,
             CASE WHEN b.oneshot = 1 THEN 'true' ELSE 'false' END AS ONESHOT
          FROM BOOK b
          JOIN SERIES s ON s.ID = b.SERIES_ID
          LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
          LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
          LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
         "#,
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("failed to read BOOK rows for index rebuild: {error}"))?;

    Ok(book_rows.into_iter().map(build_book_document).collect())
}

async fn load_all_series_search_documents(
    pool: sqlx::SqlitePool,
) -> Result<Vec<SearchDocument>, String> {
    let series_rows = sqlx::query(
        r#"SELECT
             s.ID AS ID,
             COALESCE(sm.TITLE, s.NAME) AS TITLE,
             COALESCE(sm.PUBLISHER, '') AS PUBLISHER,
             COALESCE(sm.STATUS, '') AS STATUS,
             COALESCE(sm.READING_DIRECTION, '') AS READING_DIRECTION,
             COALESCE(CAST(sm.AGE_RATING AS TEXT), '') AS AGE_RATING,
             COALESCE(sm.LANGUAGE, '') AS LANGUAGE,
             COALESCE(STRFTIME('%Y', bma.RELEASE_DATE), '') AS RELEASE_YEAR,
             CASE WHEN s.DELETED_DATE IS NULL THEN 'false' ELSE 'true' END AS DELETED,
             CASE WHEN s.oneshot = 1 THEN 'true' ELSE 'false' END AS ONESHOT,
             COALESCE(CAST(sm.TOTAL_BOOK_COUNT AS TEXT), '') AS TOTAL_BOOK_COUNT,
             COALESCE(CAST(s.BOOK_COUNT AS TEXT), '') AS BOOK_COUNT,
             CASE
                 WHEN sm.TOTAL_BOOK_COUNT IS NOT NULL
                      AND s.BOOK_COUNT IS NOT NULL
                      AND sm.TOTAL_BOOK_COUNT = s.BOOK_COUNT THEN 'true'
                 ELSE ''
             END AS COMPLETE,
             COALESCE((SELECT GROUP_CONCAT(st.TAG, '|') FROM SERIES_METADATA_TAG st WHERE st.SERIES_ID = s.ID), '') AS SERIES_TAGS,
             COALESCE((SELECT GROUP_CONCAT(bmat.TAG, '|') FROM BOOK_METADATA_AGGREGATION_TAG bmat WHERE bmat.SERIES_ID = s.ID), '') AS BOOK_TAGS,
             COALESCE((SELECT GROUP_CONCAT(sg.GENRE, '|') FROM SERIES_METADATA_GENRE sg WHERE sg.SERIES_ID = s.ID), '') AS GENRES,
             COALESCE((SELECT GROUP_CONCAT(ss.LABEL, '|') FROM SERIES_METADATA_SHARING ss WHERE ss.SERIES_ID = s.ID), '') AS SHARING_LABELS,
             COALESCE((SELECT GROUP_CONCAT(baa.NAME, '|') FROM BOOK_METADATA_AGGREGATION_AUTHOR baa WHERE baa.SERIES_ID = s.ID), '') AS AUTHORS,
             COALESCE((SELECT GROUP_CONCAT(COALESCE(baa.ROLE, '') || '::' || baa.NAME, '|') FROM BOOK_METADATA_AGGREGATION_AUTHOR baa WHERE baa.SERIES_ID = s.ID), '') AS AUTHOR_ROLES,
             COALESCE(sm.TITLE_SORT, '') AS TITLE_SORT,
             COALESCE((SELECT GROUP_CONCAT(sat.TITLE, '|') FROM SERIES_METADATA_ALTERNATE_TITLE sat WHERE sat.SERIES_ID = s.ID), '') AS ALTERNATE_TITLES
          FROM SERIES s
          LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
          LEFT JOIN BOOK_METADATA_AGGREGATION bma ON bma.SERIES_ID = s.ID
         "#,
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("failed to read SERIES rows for index rebuild: {error}"))?;

    Ok(series_rows.into_iter().map(build_series_document).collect())
}

async fn load_all_collection_search_documents(
    pool: sqlx::SqlitePool,
) -> Result<Vec<SearchDocument>, String> {
    let rows = sqlx::query("SELECT ID, NAME FROM COLLECTION")
        .fetch_all(&pool)
        .await
        .map_err(|error| format!("failed to read COLLECTION rows for index rebuild: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| build_named_document(row, SearchEntityType::Collection))
        .collect())
}

async fn load_all_readlist_search_documents(
    pool: sqlx::SqlitePool,
) -> Result<Vec<SearchDocument>, String> {
    let rows = sqlx::query("SELECT ID, NAME FROM READLIST")
        .fetch_all(&pool)
        .await
        .map_err(|error| format!("failed to read READLIST rows for index rebuild: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| build_named_document(row, SearchEntityType::ReadList))
        .collect())
}

fn build_book_document(row: sqlx::sqlite::SqliteRow) -> SearchDocument {
    let mut fields = vec![
        search_field(SearchField::Isbn, row.get::<String, _>("ISBN")),
        search_field(SearchField::Status, row.get::<String, _>("MEDIA_STATUS")),
        search_field(
            SearchField::Publisher,
            row.get::<String, _>("ONESHOT_PUBLISHER"),
        ),
        search_field(SearchField::Status, row.get::<String, _>("ONESHOT_STATUS")),
        search_field(
            SearchField::ReadingDirection,
            row.get::<String, _>("ONESHOT_READING_DIRECTION"),
        ),
        search_field(
            SearchField::AgeRating,
            row.get::<String, _>("ONESHOT_AGE_RATING"),
        ),
        search_field(
            SearchField::Language,
            row.get::<String, _>("ONESHOT_LANGUAGE"),
        ),
        search_field(
            SearchField::ReleaseDate,
            row.get::<String, _>("RELEASE_YEAR"),
        ),
        search_field(SearchField::Deleted, row.get::<String, _>("DELETED")),
        search_field(SearchField::Oneshot, row.get::<String, _>("ONESHOT")),
    ];
    fields.extend(search_fields(
        SearchField::BookTag,
        row.get::<String, _>("BOOK_TAGS"),
    ));
    fields.extend(search_fields(
        SearchField::Tag,
        row.get::<String, _>("BOOK_TAGS"),
    ));
    fields.extend(search_fields(
        SearchField::Author,
        row.get::<String, _>("AUTHORS"),
    ));
    fields.extend(search_role_author_fields(
        row.get::<String, _>("AUTHOR_ROLES"),
    ));
    fields.extend(search_fields(
        SearchField::Genre,
        row.get::<String, _>("ONESHOT_GENRES"),
    ));
    fields.extend(search_fields(
        SearchField::SharingLabel,
        row.get::<String, _>("ONESHOT_SHARING_LABELS"),
    ));

    SearchDocument {
        entity_type: SearchEntityType::Book,
        id: row.get::<String, _>("ID"),
        title: row.get::<String, _>("TITLE"),
        fields,
    }
}

fn build_series_document(row: sqlx::sqlite::SqliteRow) -> SearchDocument {
    let mut fields = vec![
        search_field(SearchField::Title, row.get::<String, _>("TITLE_SORT")),
        search_field(SearchField::Publisher, row.get::<String, _>("PUBLISHER")),
        search_field(SearchField::Status, row.get::<String, _>("STATUS")),
        search_field(
            SearchField::ReadingDirection,
            row.get::<String, _>("READING_DIRECTION"),
        ),
        search_field(SearchField::AgeRating, row.get::<String, _>("AGE_RATING")),
        search_field(SearchField::Language, row.get::<String, _>("LANGUAGE")),
        search_field(
            SearchField::ReleaseDate,
            row.get::<String, _>("RELEASE_YEAR"),
        ),
        search_field(SearchField::Deleted, row.get::<String, _>("DELETED")),
        search_field(SearchField::Oneshot, row.get::<String, _>("ONESHOT")),
        search_field(SearchField::Complete, row.get::<String, _>("COMPLETE")),
        search_field(
            SearchField::TotalBookCount,
            row.get::<String, _>("TOTAL_BOOK_COUNT"),
        ),
        search_field(SearchField::BookCount, row.get::<String, _>("BOOK_COUNT")),
    ];
    fields.extend(search_fields(
        SearchField::SeriesTag,
        row.get::<String, _>("SERIES_TAGS"),
    ));
    fields.extend(search_fields(
        SearchField::BookTag,
        row.get::<String, _>("BOOK_TAGS"),
    ));
    fields.extend(search_fields(
        SearchField::Tag,
        row.get::<String, _>("SERIES_TAGS"),
    ));
    fields.extend(search_fields(
        SearchField::Tag,
        row.get::<String, _>("BOOK_TAGS"),
    ));
    fields.extend(search_fields(
        SearchField::Genre,
        row.get::<String, _>("GENRES"),
    ));
    fields.extend(search_fields(
        SearchField::SharingLabel,
        row.get::<String, _>("SHARING_LABELS"),
    ));
    fields.extend(search_fields(
        SearchField::Author,
        row.get::<String, _>("AUTHORS"),
    ));
    fields.extend(search_fields(
        SearchField::Title,
        row.get::<String, _>("ALTERNATE_TITLES"),
    ));
    fields.extend(search_role_author_fields(
        row.get::<String, _>("AUTHOR_ROLES"),
    ));

    SearchDocument {
        entity_type: SearchEntityType::Series,
        id: row.get::<String, _>("ID"),
        title: row.get::<String, _>("TITLE"),
        fields,
    }
}

fn build_named_document(
    row: sqlx::sqlite::SqliteRow,
    entity_type: SearchEntityType,
) -> SearchDocument {
    SearchDocument {
        entity_type,
        id: row.get::<String, _>("ID"),
        title: row.get::<String, _>("NAME"),
        fields: vec![search_field(
            SearchField::Name,
            row.get::<String, _>("NAME"),
        )],
    }
}

fn search_role_author_fields(values: String) -> Vec<SearchFieldEntry> {
    let mut fields = Vec::new();
    for value in values
        .split('|')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let Some((role, name)) = value.split_once(AUTHOR_ROLE_DELIMITER) else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        for role_field in normalize_author_role_fields(role) {
            fields.push(search_field(*role_field, name.to_string()));
        }
    }
    fields
}

fn normalize_author_role_fields(role: &str) -> &'static [SearchField] {
    match role.trim().to_ascii_lowercase().as_str() {
        "writer" => &[SearchField::Writer],
        "penciller" => &[SearchField::Penciller, SearchField::Penciler],
        "penciler" => &[SearchField::Penciler, SearchField::Penciller],
        "inker" => &[SearchField::Inker],
        "colorist" => &[SearchField::Colorist],
        "letterer" => &[SearchField::Letterer],
        "cover" => &[SearchField::Cover],
        "editor" => &[SearchField::Editor],
        "translator" => &[SearchField::Translator],
        _ => &[],
    }
}
