use std::collections::HashSet;

use sqlx::{Row, SqlitePool};

use komga_application::opds::{
    PersistedBookFeedRecord, PersistedNamedRecord, PersistedSeriesRecord,
};
use komga_domain::discovery::compare_book_names;

use super::records::{parsed_age_rating, parsed_sharing_labels};

pub(super) async fn load_publishers(
    pool: &SqlitePool,
    allowed_library_ids: Option<&HashSet<String>>,
) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT DISTINCT sm.PUBLISHER AS PUBLISHER, s.LIBRARY_ID AS LIBRARY_ID
FROM SERIES_METADATA sm
JOIN SERIES s ON s.ID = sm.SERIES_ID
WHERE sm.PUBLISHER IS NOT NULL
  AND trim(sm.PUBLISHER) != ''"#,
    )
    .fetch_all(pool)
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

    values.sort_by(|left, right| {
        komga_domain::discovery::system_locale_collator().compare(left, right)
    });

    Ok(values)
}

pub(super) async fn load_collections(
    pool: &SqlitePool,
    library_id: Option<&str>,
) -> Result<Vec<PersistedNamedRecord>, sqlx::Error> {
    let rows = if let Some(library_id) = library_id {
        sqlx::query(
            r#"SELECT DISTINCT c.ID, c.NAME, c.ORDERED,
       COALESCE(c.LAST_MODIFIED_DATE, c.CREATED_DATE, '') AS LAST_MODIFIED
FROM COLLECTION c
JOIN COLLECTION_SERIES cs ON cs.COLLECTION_ID = c.ID
JOIN SERIES s ON s.ID = cs.SERIES_ID
WHERE s.LIBRARY_ID = ?"#,
        )
        .bind(library_id)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"SELECT ID, NAME, ORDERED, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED
FROM COLLECTION"#,
        )
        .fetch_all(pool)
        .await?
    };

    let mut records = rows
        .into_iter()
        .map(|row| PersistedNamedRecord {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
            ordered: row.get::<bool, _>("ORDERED"),
        })
        .collect::<Vec<_>>();

    records.sort_by(|left, right| {
        let ordering = compare_book_names(left.name.as_str(), right.name.as_str());
        if ordering.is_eq() {
            left.id.cmp(&right.id)
        } else {
            ordering
        }
    });

    Ok(records)
}

pub(super) async fn load_collection(
    pool: &SqlitePool,
    collection_id: &str,
) -> Result<Option<PersistedNamedRecord>, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT ID, NAME, ORDERED, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED
FROM COLLECTION
WHERE ID = ?
LIMIT 1"#,
    )
    .bind(collection_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| PersistedNamedRecord {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
        last_modified: row.get::<String, _>("LAST_MODIFIED"),
        ordered: row.get::<bool, _>("ORDERED"),
    }))
}

pub(super) async fn load_collection_books(
    pool: &SqlitePool,
    collection_id: &str,
) -> Result<Vec<PersistedBookFeedRecord>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT b.ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE, b.NAME AS FILE_NAME,
       COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
       COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
       COALESCE((SELECT GROUP_CONCAT(LABEL, char(30))
                 FROM (SELECT DISTINCT sms_inner.LABEL AS LABEL
                       FROM SERIES_METADATA_SHARING sms_inner
                       WHERE sms_inner.SERIES_ID = s.ID)), '') AS SHARING_LABELS,
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
GROUP BY b.ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME), b.NAME,
         COALESCE(m.MEDIA_TYPE, 'application/octet-stream'),
         COALESCE(sm.AGE_RATING, NULL),
         COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '')
ORDER BY cs.NUMBER ASC, COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0) ASC,
         b.ID ASC"#,
    )
    .bind(collection_id)
    .fetch_all(pool)
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

pub(super) async fn load_collection_series(
    pool: &SqlitePool,
    collection_id: &str,
    ordered: bool,
) -> Result<Vec<PersistedSeriesRecord>, sqlx::Error> {
    let query = if ordered {
        r#"SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE,
       COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) AS TITLE_SORT,
       COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
       COALESCE((SELECT GROUP_CONCAT(LABEL, char(30))
                 FROM (SELECT DISTINCT sms_inner.LABEL AS LABEL
                       FROM SERIES_METADATA_SHARING sms_inner
                       WHERE sms_inner.SERIES_ID = s.ID)), '') AS SHARING_LABELS,
       COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED
FROM COLLECTION_SERIES cs
JOIN SERIES s ON s.ID = cs.SERIES_ID
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
WHERE cs.COLLECTION_ID = ?
  AND s.DELETED_DATE IS NULL
GROUP BY s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME),
         COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME),
         COALESCE(sm.AGE_RATING, NULL),
         COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '')
ORDER BY cs.NUMBER ASC, s.ID ASC"#
    } else {
        r#"SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE,
       COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) AS TITLE_SORT,
       COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
       COALESCE((SELECT GROUP_CONCAT(LABEL, char(30))
                 FROM (SELECT DISTINCT sms_inner.LABEL AS LABEL
                       FROM SERIES_METADATA_SHARING sms_inner
                       WHERE sms_inner.SERIES_ID = s.ID)), '') AS SHARING_LABELS,
       COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED
FROM COLLECTION_SERIES cs
JOIN SERIES s ON s.ID = cs.SERIES_ID
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
WHERE cs.COLLECTION_ID = ?
  AND s.DELETED_DATE IS NULL
GROUP BY s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME),
         COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME),
         COALESCE(sm.AGE_RATING, NULL),
         COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '')"#
    };
    let rows = sqlx::query(query)
        .bind(collection_id)
        .fetch_all(pool)
        .await?;

    let mut series: Vec<(String, PersistedSeriesRecord)> = rows
        .into_iter()
        .map(|row| {
            let title_sort = row.get::<String, _>("TITLE_SORT");
            (
                title_sort,
                PersistedSeriesRecord {
                    id: row.get::<String, _>("ID"),
                    library_id: row.get::<String, _>("LIBRARY_ID"),
                    title: row.get::<String, _>("TITLE"),
                    summary: String::new(),
                    age_rating: parsed_age_rating(&row),
                    sharing_labels: parsed_sharing_labels(&row),
                    last_modified: row.get::<String, _>("LAST_MODIFIED"),
                },
            )
        })
        .collect();
    if !ordered {
        let collator = komga_domain::discovery::system_locale_collator();
        series.sort_by(|left, right| {
            collator
                .compare(&left.0, &right.0)
                .then_with(|| left.1.id.cmp(&right.1.id))
        });
    }

    Ok(series.into_iter().map(|(_, record)| record).collect())
}
