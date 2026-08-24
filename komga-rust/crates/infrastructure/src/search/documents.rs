use anyhow::Context;
use sqlx::Row;

use crate::persistence::sqlite::codecs::parse_sqlite_group_concat_values;
use crate::search::lifecycle::{SearchDocument, SearchEntityType, SearchField, SearchFieldEntry};

const AUTHOR_ROLE_DELIMITER: &str = "::";

fn search_field(field: SearchField, value: String) -> SearchFieldEntry {
    SearchFieldEntry::new(field, value)
}

fn search_fields(field: SearchField, values: String) -> Vec<SearchFieldEntry> {
    parse_sqlite_group_concat_values(&values)
        .into_iter()
        .map(|value| SearchFieldEntry::new(field, value))
        .collect()
}

pub(super) async fn load_rebuild_search_documents(
    pool: sqlx::SqlitePool,
) -> anyhow::Result<Vec<SearchDocument>> {
    let mut docs = load_all_book_search_documents(pool.clone()).await?;
    docs.extend(load_all_series_search_documents(pool.clone()).await?);
    docs.extend(load_all_collection_search_documents(pool.clone()).await?);
    docs.extend(load_all_readlist_search_documents(pool).await?);
    Ok(docs)
}

pub(super) async fn load_rebuild_search_documents_for_entities(
    pool: sqlx::SqlitePool,
    entity_types: &[SearchEntityType],
) -> anyhow::Result<Vec<SearchDocument>> {
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
) -> anyhow::Result<Option<SearchDocument>> {
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
             CASE
                 WHEN b.oneshot = 1 THEN COALESCE(
                     (SELECT GROUP_CONCAT(g.GENRE, char(30))
                      FROM SERIES_METADATA_GENRE g
                      WHERE g.SERIES_ID = s.ID),
                     ''
                 )
                 ELSE ''
             END AS ONESHOT_GENRES,
             CASE
                 WHEN b.oneshot = 1 THEN COALESCE(
                     (SELECT GROUP_CONCAT(sh.LABEL, char(30))
                      FROM SERIES_METADATA_SHARING sh
                      WHERE sh.SERIES_ID = s.ID),
                     ''
                 )
                 ELSE ''
             END AS ONESHOT_SHARING_LABELS,
             COALESCE(
                 (SELECT GROUP_CONCAT(bt.TAG, char(30))
                  FROM BOOK_METADATA_TAG bt
                  WHERE bt.BOOK_ID = b.ID),
                 ''
             ) AS BOOK_TAGS,
             COALESCE(
                 (SELECT GROUP_CONCAT(ba.NAME, char(30))
                  FROM BOOK_METADATA_AUTHOR ba
                  WHERE ba.BOOK_ID = b.ID),
                 ''
             ) AS AUTHORS,
             COALESCE(
                 (SELECT GROUP_CONCAT(COALESCE(ba.ROLE, '') || '::' || ba.NAME, char(30))
                  FROM BOOK_METADATA_AUTHOR ba
                  WHERE ba.BOOK_ID = b.ID),
                 ''
             ) AS AUTHOR_ROLES,
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
    .context("failed to load BOOK row for search upsert")?;

    Ok(row.map(build_book_document))
}

pub(super) async fn load_oneshot_book_search_documents(
    pool: sqlx::SqlitePool,
    series_id: &str,
) -> anyhow::Result<Vec<SearchDocument>> {
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
             CASE
                 WHEN b.oneshot = 1 THEN COALESCE(
                     (SELECT GROUP_CONCAT(g.GENRE, char(30))
                      FROM SERIES_METADATA_GENRE g
                      WHERE g.SERIES_ID = s.ID),
                     ''
                 )
                 ELSE ''
             END AS ONESHOT_GENRES,
             CASE
                 WHEN b.oneshot = 1 THEN COALESCE(
                     (SELECT GROUP_CONCAT(sh.LABEL, char(30))
                      FROM SERIES_METADATA_SHARING sh
                      WHERE sh.SERIES_ID = s.ID),
                     ''
                 )
                 ELSE ''
             END AS ONESHOT_SHARING_LABELS,
             COALESCE(
                 (SELECT GROUP_CONCAT(bt.TAG, char(30))
                  FROM BOOK_METADATA_TAG bt
                  WHERE bt.BOOK_ID = b.ID),
                 ''
             ) AS BOOK_TAGS,
             COALESCE(
                 (SELECT GROUP_CONCAT(ba.NAME, char(30))
                  FROM BOOK_METADATA_AUTHOR ba
                  WHERE ba.BOOK_ID = b.ID),
                 ''
             ) AS AUTHORS,
             COALESCE(
                 (SELECT GROUP_CONCAT(COALESCE(ba.ROLE, '') || '::' || ba.NAME, char(30))
                  FROM BOOK_METADATA_AUTHOR ba
                  WHERE ba.BOOK_ID = b.ID),
                 ''
             ) AS AUTHOR_ROLES,
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
    .context("failed to load oneshot BOOK rows for search upsert")?;

    Ok(rows.into_iter().map(build_book_document).collect())
}

pub(super) async fn load_series_search_document(
    pool: sqlx::SqlitePool,
    series_id: &str,
) -> anyhow::Result<Option<SearchDocument>> {
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
             COALESCE(
                 (SELECT GROUP_CONCAT(st.TAG, char(30))
                  FROM SERIES_METADATA_TAG st
                  WHERE st.SERIES_ID = s.ID),
                 ''
             ) AS SERIES_TAGS,
             COALESCE(
                 (SELECT GROUP_CONCAT(bmat.TAG, char(30))
                  FROM BOOK_METADATA_AGGREGATION_TAG bmat
                  WHERE bmat.SERIES_ID = s.ID),
                 ''
             ) AS BOOK_TAGS,
             COALESCE(
                 (SELECT GROUP_CONCAT(sg.GENRE, char(30))
                  FROM SERIES_METADATA_GENRE sg
                  WHERE sg.SERIES_ID = s.ID),
                 ''
             ) AS GENRES,
             COALESCE(
                 (SELECT GROUP_CONCAT(ss.LABEL, char(30))
                  FROM SERIES_METADATA_SHARING ss
                  WHERE ss.SERIES_ID = s.ID),
                 ''
             ) AS SHARING_LABELS,
             COALESCE(
                 (SELECT GROUP_CONCAT(baa.NAME, char(30))
                  FROM BOOK_METADATA_AGGREGATION_AUTHOR baa
                  WHERE baa.SERIES_ID = s.ID),
                 ''
             ) AS AUTHORS,
             COALESCE(
                 (SELECT GROUP_CONCAT(COALESCE(baa.ROLE, '') || '::' || baa.NAME, char(30))
                  FROM BOOK_METADATA_AGGREGATION_AUTHOR baa
                  WHERE baa.SERIES_ID = s.ID),
                 ''
             ) AS AUTHOR_ROLES,
             COALESCE(sm.TITLE_SORT, '') AS TITLE_SORT,
             COALESCE(
                 (SELECT GROUP_CONCAT(sat.TITLE, char(30))
                  FROM SERIES_METADATA_ALTERNATE_TITLE sat
                  WHERE sat.SERIES_ID = s.ID),
                 ''
             ) AS ALTERNATE_TITLES
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
    .context("failed to load SERIES row for search upsert")?;

    Ok(row.map(build_series_document))
}

pub(super) async fn load_collection_search_document(
    pool: sqlx::SqlitePool,
    collection_id: &str,
) -> anyhow::Result<Option<SearchDocument>> {
    let row = sqlx::query("SELECT ID, NAME FROM COLLECTION WHERE ID = ? LIMIT 1")
        .bind(collection_id)
        .fetch_optional(&pool)
        .await
        .context("failed to load COLLECTION row for search upsert")?;

    Ok(row.map(|row| build_named_document(row, SearchEntityType::Collection)))
}

pub(super) async fn load_readlist_search_document(
    pool: sqlx::SqlitePool,
    readlist_id: &str,
) -> anyhow::Result<Option<SearchDocument>> {
    let row = sqlx::query("SELECT ID, NAME FROM READLIST WHERE ID = ? LIMIT 1")
        .bind(readlist_id)
        .fetch_optional(&pool)
        .await
        .context("failed to load READLIST row for search upsert")?;

    Ok(row.map(|row| build_named_document(row, SearchEntityType::ReadList)))
}

async fn load_all_book_search_documents(
    pool: sqlx::SqlitePool,
) -> anyhow::Result<Vec<SearchDocument>> {
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
             CASE
                 WHEN b.oneshot = 1 THEN COALESCE(
                     (SELECT GROUP_CONCAT(g.GENRE, char(30))
                      FROM SERIES_METADATA_GENRE g
                      WHERE g.SERIES_ID = s.ID),
                     ''
                 )
                 ELSE ''
             END AS ONESHOT_GENRES,
             CASE
                 WHEN b.oneshot = 1 THEN COALESCE(
                     (SELECT GROUP_CONCAT(sh.LABEL, char(30))
                      FROM SERIES_METADATA_SHARING sh
                      WHERE sh.SERIES_ID = s.ID),
                     ''
                 )
                 ELSE ''
             END AS ONESHOT_SHARING_LABELS,
             COALESCE(
                 (SELECT GROUP_CONCAT(bt.TAG, char(30))
                  FROM BOOK_METADATA_TAG bt
                  WHERE bt.BOOK_ID = b.ID),
                 ''
             ) AS BOOK_TAGS,
             COALESCE(
                 (SELECT GROUP_CONCAT(ba.NAME, char(30))
                  FROM BOOK_METADATA_AUTHOR ba
                  WHERE ba.BOOK_ID = b.ID),
                 ''
             ) AS AUTHORS,
             COALESCE(
                 (SELECT GROUP_CONCAT(COALESCE(ba.ROLE, '') || '::' || ba.NAME, char(30))
                  FROM BOOK_METADATA_AUTHOR ba
                  WHERE ba.BOOK_ID = b.ID),
                 ''
             ) AS AUTHOR_ROLES,
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
    .context("failed to read BOOK rows for index rebuild")?;

    Ok(book_rows.into_iter().map(build_book_document).collect())
}

async fn load_all_series_search_documents(
    pool: sqlx::SqlitePool,
) -> anyhow::Result<Vec<SearchDocument>> {
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
             COALESCE(
                 (SELECT GROUP_CONCAT(st.TAG, char(30))
                  FROM SERIES_METADATA_TAG st
                  WHERE st.SERIES_ID = s.ID),
                 ''
             ) AS SERIES_TAGS,
             COALESCE(
                 (SELECT GROUP_CONCAT(bmat.TAG, char(30))
                  FROM BOOK_METADATA_AGGREGATION_TAG bmat
                  WHERE bmat.SERIES_ID = s.ID),
                 ''
             ) AS BOOK_TAGS,
             COALESCE(
                 (SELECT GROUP_CONCAT(sg.GENRE, char(30))
                  FROM SERIES_METADATA_GENRE sg
                  WHERE sg.SERIES_ID = s.ID),
                 ''
             ) AS GENRES,
             COALESCE(
                 (SELECT GROUP_CONCAT(ss.LABEL, char(30))
                  FROM SERIES_METADATA_SHARING ss
                  WHERE ss.SERIES_ID = s.ID),
                 ''
             ) AS SHARING_LABELS,
             COALESCE(
                 (SELECT GROUP_CONCAT(baa.NAME, char(30))
                  FROM BOOK_METADATA_AGGREGATION_AUTHOR baa
                  WHERE baa.SERIES_ID = s.ID),
                 ''
             ) AS AUTHORS,
             COALESCE(
                 (SELECT GROUP_CONCAT(COALESCE(baa.ROLE, '') || '::' || baa.NAME, char(30))
                  FROM BOOK_METADATA_AGGREGATION_AUTHOR baa
                  WHERE baa.SERIES_ID = s.ID),
                 ''
             ) AS AUTHOR_ROLES,
             COALESCE(sm.TITLE_SORT, '') AS TITLE_SORT,
             COALESCE(
                 (SELECT GROUP_CONCAT(sat.TITLE, char(30))
                  FROM SERIES_METADATA_ALTERNATE_TITLE sat
                  WHERE sat.SERIES_ID = s.ID),
                 ''
             ) AS ALTERNATE_TITLES
          FROM SERIES s
          LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
          LEFT JOIN BOOK_METADATA_AGGREGATION bma ON bma.SERIES_ID = s.ID
         "#,
    )
    .fetch_all(&pool)
    .await
    .context("failed to read SERIES rows for index rebuild")?;

    Ok(series_rows.into_iter().map(build_series_document).collect())
}

async fn load_all_collection_search_documents(
    pool: sqlx::SqlitePool,
) -> anyhow::Result<Vec<SearchDocument>> {
    let rows = sqlx::query("SELECT ID, NAME FROM COLLECTION")
        .fetch_all(&pool)
        .await
        .context("failed to read COLLECTION rows for index rebuild: ")?;

    Ok(rows
        .into_iter()
        .map(|row| build_named_document(row, SearchEntityType::Collection))
        .collect())
}

async fn load_all_readlist_search_documents(
    pool: sqlx::SqlitePool,
) -> anyhow::Result<Vec<SearchDocument>> {
    let rows = sqlx::query("SELECT ID, NAME FROM READLIST")
        .fetch_all(&pool)
        .await
        .context("failed to read READLIST rows for index rebuild")?;

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
    for value in parse_sqlite_group_concat_values(&values) {
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use sqlx::SqlitePool;

    use super::*;
    use crate::persistence::sqlite::{connect_test_pool, schema};

    fn temp_db_path(case_id: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "komga-rust-search-documents-{case_id}-{nanos}.sqlite"
        ))
    }

    async fn open_bootstrapped_pool(case_id: &str) -> (PathBuf, SqlitePool) {
        let db_path = temp_db_path(case_id);
        let pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open");
        schema::bootstrap_pool(&pool)
            .await
            .expect("temporary sqlite db should bootstrap main schema");
        (db_path, pool)
    }

    async fn seed_library(pool: &SqlitePool) {
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
            .bind("library-1")
            .bind("Library 1")
            .bind("/tmp")
            .execute(pool)
            .await
            .expect("library row should be inserted");
    }

    async fn seed_series(pool: &SqlitePool, series_id: &str, oneshot: bool) {
        sqlx::query(
            r#"
            INSERT INTO SERIES (
                ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, oneshot
            )
            VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, ?)
            "#,
        )
        .bind(series_id)
        .bind(0_i64)
        .bind(series_id)
        .bind(format!("series/{series_id}"))
        .bind("library-1")
        .bind(oneshot)
        .execute(pool)
        .await
        .expect("series row should be inserted");
    }

    async fn seed_series_metadata(pool: &SqlitePool, series_id: &str, status: &str) {
        sqlx::query(
            r#"
            INSERT INTO SERIES_METADATA (
                STATUS, TITLE, TITLE_SORT, PUBLISHER, SERIES_ID
            )
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(status)
        .bind(series_id)
        .bind(series_id)
        .bind("Publisher")
        .bind(series_id)
        .execute(pool)
        .await
        .expect("series metadata row should be inserted");
    }

    async fn seed_book(pool: &SqlitePool, book_id: &str, series_id: &str, oneshot: bool) {
        sqlx::query(
            r#"
            INSERT INTO BOOK (
                ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE,
                NUMBER, LIBRARY_ID, oneshot
            )
            VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(book_id)
        .bind(0_i64)
        .bind(book_id)
        .bind(format!("books/{book_id}.cbz"))
        .bind(series_id)
        .bind(0_i64)
        .bind(1_i64)
        .bind("library-1")
        .bind(oneshot)
        .execute(pool)
        .await
        .expect("book row should be inserted");
    }

    async fn seed_book_metadata(pool: &SqlitePool, book_id: &str) {
        sqlx::query(
            "INSERT INTO BOOK_METADATA (BOOK_ID, TITLE, NUMBER, NUMBER_SORT) VALUES (?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind(book_id)
        .bind("1")
        .bind(1.0_f64)
        .execute(pool)
        .await
        .expect("book metadata row should be inserted");
    }

    fn search_field_values(document: &SearchDocument, field: SearchField) -> Vec<&str> {
        document
            .fields
            .iter()
            .filter(|entry| entry.field == field)
            .map(|entry| entry.value.as_str())
            .collect()
    }

    #[tokio::test]
    async fn search_document_loaders_preserve_separator_characters_in_metadata_values() {
        let (db_path, pool) = open_bootstrapped_pool("separator-values").await;
        seed_library(&pool).await;
        seed_series(&pool, "series-separator", true).await;
        seed_series_metadata(&pool, "series-separator", "ONGOING").await;
        seed_book(&pool, "book-separator", "series-separator", true).await;
        seed_book_metadata(&pool, "book-separator").await;

        sqlx::query("INSERT INTO SERIES_METADATA_GENRE (SERIES_ID, GENRE) VALUES (?, ?)")
            .bind("series-separator")
            .bind("Sci | Fi")
            .execute(&pool)
            .await
            .expect("series genre should be inserted");
        sqlx::query("INSERT INTO SERIES_METADATA_SHARING (SERIES_ID, LABEL) VALUES (?, ?)")
            .bind("series-separator")
            .bind("Family | Kids")
            .execute(&pool)
            .await
            .expect("sharing label should be inserted");
        sqlx::query("INSERT INTO SERIES_METADATA_TAG (SERIES_ID, TAG) VALUES (?, ?)")
            .bind("series-separator")
            .bind("Series | Tag")
            .execute(&pool)
            .await
            .expect("series tag should be inserted");
        sqlx::query(
            "INSERT INTO SERIES_METADATA_ALTERNATE_TITLE (SERIES_ID, LABEL, TITLE) VALUES (?, ?, ?)",
        )
        .bind("series-separator")
        .bind("")
        .bind("Alt | Title")
        .execute(&pool)
        .await
        .expect("alternate title should be inserted");
        sqlx::query("INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) VALUES (?, ?)")
            .bind("book-separator")
            .bind("Action | Comedy")
            .execute(&pool)
            .await
            .expect("book tag should be inserted");
        sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
            .bind("book-separator")
            .bind("Alex | Writer")
            .bind("writer")
            .execute(&pool)
            .await
            .expect("book author should be inserted");
        sqlx::query("INSERT INTO BOOK_METADATA_AGGREGATION (SERIES_ID) VALUES (?)")
            .bind("series-separator")
            .execute(&pool)
            .await
            .expect("series metadata aggregation should be inserted");
        sqlx::query("INSERT INTO BOOK_METADATA_AGGREGATION_TAG (SERIES_ID, TAG) VALUES (?, ?)")
            .bind("series-separator")
            .bind("Aggregated | Tag")
            .execute(&pool)
            .await
            .expect("aggregated book tag should be inserted");
        sqlx::query(
            "INSERT INTO BOOK_METADATA_AGGREGATION_AUTHOR (SERIES_ID, NAME, ROLE) VALUES (?, ?, ?)",
        )
        .bind("series-separator")
        .bind("Dana | Editor")
        .bind("editor")
        .execute(&pool)
        .await
        .expect("aggregated author should be inserted");

        let book_document = load_book_search_document(pool.clone(), "book-separator")
            .await
            .expect("book search document should load")
            .expect("book search document should exist");
        assert_eq!(
            search_field_values(&book_document, SearchField::BookTag),
            vec!["Action | Comedy"]
        );
        assert_eq!(
            search_field_values(&book_document, SearchField::Writer),
            vec!["Alex | Writer"]
        );
        assert_eq!(
            search_field_values(&book_document, SearchField::Genre),
            vec!["Sci | Fi"]
        );
        assert_eq!(
            search_field_values(&book_document, SearchField::SharingLabel),
            vec!["Family | Kids"]
        );

        let series_document = load_series_search_document(pool.clone(), "series-separator")
            .await
            .expect("series search document should load")
            .expect("series search document should exist");
        assert_eq!(
            search_field_values(&series_document, SearchField::SeriesTag),
            vec!["Series | Tag"]
        );
        assert_eq!(
            search_field_values(&series_document, SearchField::BookTag),
            vec!["Aggregated | Tag"]
        );
        assert_eq!(
            search_field_values(&series_document, SearchField::Editor),
            vec!["Dana | Editor"]
        );
        assert!(search_field_values(&series_document, SearchField::Title).contains(&"Alt | Title"));

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }
}
