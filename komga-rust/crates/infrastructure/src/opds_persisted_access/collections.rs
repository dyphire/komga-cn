use std::collections::HashSet;
use std::path::Path;

use crate::sqlite::connect_read_pool;
use icu::collator::{
    Collator,
    options::{CollatorOptions, Strength},
};
use icu::locale::locale;
use sqlx::Row;
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

use super::records::{
    PersistedBookFeedRecord, PersistedNamedRecord, PersistedSeriesRecord, parsed_age_rating,
    parsed_sharing_labels,
};

pub async fn load_publishers(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
) -> Result<Vec<String>, sqlx::Error> {
    let pool = connect_read_pool(database_file).await?;
    let rows = sqlx::query(
        r#"SELECT DISTINCT sm.PUBLISHER AS PUBLISHER, s.LIBRARY_ID AS LIBRARY_ID
FROM SERIES_METADATA sm
JOIN SERIES s ON s.ID = sm.SERIES_ID
WHERE sm.PUBLISHER IS NOT NULL
  AND trim(sm.PUBLISHER) != ''
ORDER BY lower(sm.PUBLISHER), sm.PUBLISHER"#,
    )
    .fetch_all(&pool)
    .await?;

    let mut values = Vec::new();
    let mut seen = HashSet::new();
    for row in rows {
        let library_id = row.get::<String, _>("LIBRARY_ID");
        let visible = match allowed_library_ids {
            None => true,
            Some(ids) => ids.contains(&library_id),
        };
        if !visible {
            continue;
        }
        let publisher = row.get::<String, _>("PUBLISHER");
        if seen.insert(publisher.clone()) {
            values.push(publisher);
        }
    }

    values.sort_by_cached_key(|value| unicode_collation_sort_key(value));

    Ok(values)
}

pub(super) fn unicode_collation_sort_key(value: &str) -> String {
    value
        .nfd()
        .filter(|ch| !is_combining_mark(*ch))
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn tertiary_unicode_collator() -> icu::collator::CollatorBorrowed<'static> {
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Tertiary);
    Collator::try_new(locale!("und").into(), options)
        .expect("unicode collator for OPDS collection sorting should construct")
}

pub async fn load_collections(
    database_file: &Path,
    library_id: Option<&str>,
) -> Result<Vec<PersistedNamedRecord>, sqlx::Error> {
    let pool = connect_read_pool(database_file).await?;
    let rows = if let Some(library_id) = library_id {
        sqlx::query(
            r#"SELECT DISTINCT c.ID, c.NAME, c.ORDERED,
       COALESCE(c.LAST_MODIFIED_DATE, c.CREATED_DATE, '') AS LAST_MODIFIED
FROM COLLECTION c
JOIN COLLECTION_SERIES cs ON cs.COLLECTION_ID = c.ID
JOIN SERIES s ON s.ID = cs.SERIES_ID
WHERE s.LIBRARY_ID = ?
ORDER BY c.NAME COLLATE NOCASE ASC, c.ID ASC"#,
        )
        .bind(library_id)
        .fetch_all(&pool)
        .await?
    } else {
        sqlx::query(
            r#"SELECT ID, NAME, ORDERED, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED
FROM COLLECTION
ORDER BY NAME COLLATE NOCASE ASC, ID ASC"#,
        )
        .fetch_all(&pool)
        .await?
    };

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

    let collator = tertiary_unicode_collator();
    records.sort_by(|left, right| {
        let ordering = collator.compare(left.name.as_str(), right.name.as_str());
        if ordering.is_eq() {
            left.id.cmp(&right.id)
        } else {
            ordering
        }
    });

    Ok(records)
}

pub async fn load_collection(
    database_file: &Path,
    collection_id: &str,
) -> Result<Option<PersistedNamedRecord>, sqlx::Error> {
    let pool = connect_read_pool(database_file).await?;
    let row = sqlx::query(
        r#"SELECT ID, NAME, ORDERED, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED
FROM COLLECTION
WHERE ID = ?
LIMIT 1"#,
    )
    .bind(collection_id)
    .fetch_optional(&pool)
    .await?;

    Ok(row.map(|row| PersistedNamedRecord {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
        last_modified: row
            .try_get::<String, _>("LAST_MODIFIED")
            .unwrap_or_default(),
        ordered: row.try_get::<bool, _>("ORDERED").unwrap_or(false),
    }))
}

pub async fn load_collection_books(
    database_file: &Path,
    collection_id: &str,
) -> Result<Vec<PersistedBookFeedRecord>, sqlx::Error> {
    let pool = connect_read_pool(database_file).await?;
    let rows = sqlx::query(
        r#"SELECT b.ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE, b.NAME AS FILE_NAME,
       COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
       COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
       COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS,
       COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') AS LAST_MODIFIED
FROM COLLECTION_SERIES cs
JOIN BOOK b ON b.SERIES_ID = cs.SERIES_ID
LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
LEFT JOIN SERIES s ON s.ID = b.SERIES_ID
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
WHERE cs.COLLECTION_ID = ?
  AND b.DELETED_DATE IS NULL
GROUP BY b.ID, b.LIBRARY_ID, TITLE, FILE_NAME, MEDIA_TYPE, AGE_RATING, LAST_MODIFIED
ORDER BY cs.NUMBER ASC, COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0) ASC,
         b.ID ASC"#,
    )
    .bind(collection_id)
    .fetch_all(&pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedBookFeedRecord {
            id: row.get::<String, _>("ID"),
            title: row.get::<String, _>("TITLE"),
            file_name: row.get::<String, _>("FILE_NAME"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: parsed_age_rating(&row),
            sharing_labels: parsed_sharing_labels(&row),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

pub async fn load_collection_series(
    database_file: &Path,
    collection_id: &str,
    ordered: bool,
) -> Result<Vec<PersistedSeriesRecord>, sqlx::Error> {
    let pool = connect_read_pool(database_file).await?;
    let query = if ordered {
        r#"SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE,
       COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
       COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS,
       COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED
FROM COLLECTION_SERIES cs
JOIN SERIES s ON s.ID = cs.SERIES_ID
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
WHERE cs.COLLECTION_ID = ?
  AND s.DELETED_DATE IS NULL
GROUP BY s.ID, s.LIBRARY_ID, TITLE, AGE_RATING, LAST_MODIFIED
ORDER BY cs.NUMBER ASC, COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE NOCASE ASC,
         s.ID ASC"#
    } else {
        r#"SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE,
       COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
       COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS,
       COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED
FROM COLLECTION_SERIES cs
JOIN SERIES s ON s.ID = cs.SERIES_ID
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
WHERE cs.COLLECTION_ID = ?
  AND s.DELETED_DATE IS NULL
GROUP BY s.ID, s.LIBRARY_ID, TITLE, AGE_RATING, LAST_MODIFIED
ORDER BY COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE NOCASE ASC, s.ID ASC"#
    };
    let rows = sqlx::query(query)
        .bind(collection_id)
        .fetch_all(&pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedSeriesRecord {
            id: row.get::<String, _>("ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            title: row.get::<String, _>("TITLE"),
            summary: String::new(),
            age_rating: row
                .try_get::<i64, _>("AGE_RATING")
                .ok()
                .and_then(|value| u16::try_from(value).ok()),
            sharing_labels: row
                .get::<String, _>("SHARING_LABELS")
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect(),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}
