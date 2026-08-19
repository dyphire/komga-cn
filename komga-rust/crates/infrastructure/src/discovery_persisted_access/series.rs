use anyhow::Context;
use std::collections::HashMap;

use komga_application::discovery::{BookMetadataLinkReadModel, SeriesReadModel, SeriesReadingDirection};
use komga_domain::discovery::SeriesStatus;
use sqlx::sqlite::SqliteRow;
use sqlx::{Error, QueryBuilder, Row, Sqlite, SqlitePool};

use super::common;
use super::models::{SeriesSummary, SeriesSummaryLink};

pub(super) async fn load_persisted_series_summaries(
    pool: &SqlitePool,
) -> anyhow::Result<Vec<SeriesSummary>> {
    let rows = fetch_persisted_series_summary_rows(pool, None)
        .await
        .context("query persisted series summaries")?;

    Ok(rows.into_iter().map(map_series_summary).collect())
}

pub(super) async fn load_persisted_series_summaries_by_ids(
    pool: &SqlitePool,
    ids: &[String],
) -> anyhow::Result<Vec<SeriesSummary>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = fetch_persisted_series_summary_rows(pool, Some(ids))
        .await
        .context("query persisted series summaries by ids")?;

    let mut rows_by_id: HashMap<String, SeriesSummary> = rows
        .into_iter()
        .map(map_series_summary)
        .map(|row| (row.id.clone(), row))
        .collect();

    Ok(ids.iter().filter_map(|id| rows_by_id.remove(id)).collect())
}

async fn fetch_persisted_series_summary_rows(
    pool: &SqlitePool,
    ids: Option<&[String]>,
) -> Result<Vec<SqliteRow>, Error> {
    let mut query = QueryBuilder::<Sqlite>::new(
        r#"SELECT s.ID,
                  s.LIBRARY_ID,
                  s.CREATED_DATE,
                  s.LAST_MODIFIED_DATE,
                  CAST(s.FILE_LAST_MODIFIED AS TEXT) AS FILE_LAST_MODIFIED,
                  s.BOOK_COUNT,
                  s.DELETED_DATE,
                  s.ONESHOT AS ONESHOT,
                  COALESCE(sm.TITLE, s.NAME) AS TITLE,
                  COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) AS TITLE_SORT,
                  COALESCE(sm.STATUS, 'ONGOING') AS STATUS,
                  COALESCE(sm.SUMMARY, '') AS SUMMARY,
                  COALESCE(sm.READING_DIRECTION, '') AS READING_DIRECTION,
                  COALESCE(sm.PUBLISHER, '') AS PUBLISHER,
                  sm.AGE_RATING AS AGE_RATING,
                  sm.TOTAL_BOOK_COUNT AS TOTAL_BOOK_COUNT,
                  COALESCE(sm.LANGUAGE, '') AS LANGUAGE,
                  COALESCE(sm.CREATED_DATE, s.CREATED_DATE) AS METADATA_CREATED,
                  COALESCE(sm.LAST_MODIFIED_DATE, s.LAST_MODIFIED_DATE) AS METADATA_LAST_MODIFIED,
                  COALESCE(bma.RELEASE_DATE, NULL) AS BOOKS_METADATA_RELEASE_DATE,
                  COALESCE(bma.SUMMARY, '') AS BOOKS_METADATA_SUMMARY,
                   COALESCE(bma.SUMMARY_NUMBER, '') AS BOOKS_METADATA_SUMMARY_NUMBER,
                   COALESCE(bma.CREATED_DATE, s.CREATED_DATE) AS BOOKS_METADATA_CREATED,
                   COALESCE(bma.LAST_MODIFIED_DATE, s.LAST_MODIFIED_DATE) AS BOOKS_METADATA_LAST_MODIFIED,
                   s.NAME AS NAME,
                   COALESCE((SELECT GROUP_CONCAT(LABEL, char(30))
                             FROM (SELECT DISTINCT sms.LABEL AS LABEL
                                   FROM SERIES_METADATA_SHARING sms
                                   WHERE sms.SERIES_ID = s.ID)), '') AS LABELS,
                  COALESCE((SELECT GROUP_CONCAT(GENRE, char(30))
                            FROM (SELECT DISTINCT smg.GENRE AS GENRE
                                  FROM SERIES_METADATA_GENRE smg
                                  WHERE smg.SERIES_ID = s.ID)), '') AS GENRES,
                  COALESCE((SELECT GROUP_CONCAT(TAG, char(30))
                            FROM (SELECT DISTINCT smt.TAG AS TAG
                                  FROM SERIES_METADATA_TAG smt
                                  WHERE smt.SERIES_ID = s.ID)), '') AS TAGS,
                  COALESCE(
                    (SELECT GROUP_CONCAT(ALTERNATE_TITLE, char(30))
                     FROM (SELECT DISTINCT CASE
                             WHEN smat.LABEL IS NULL OR smat.LABEL = '' THEN smat.TITLE
                             ELSE smat.LABEL || '::' || smat.TITLE
                           END AS ALTERNATE_TITLE
                           FROM SERIES_METADATA_ALTERNATE_TITLE smat
                           WHERE smat.SERIES_ID = s.ID)),
                    ''
                  ) AS ALTERNATE_TITLES,
                  COALESCE(
                    (SELECT GROUP_CONCAT(AUTHOR, char(30))
                     FROM (SELECT DISTINCT CASE
                             WHEN bmaa.ROLE IS NULL OR bmaa.ROLE = '' THEN bmaa.NAME
                             ELSE bmaa.NAME || '::' || bmaa.ROLE
                           END AS AUTHOR
                           FROM BOOK_METADATA_AGGREGATION_AUTHOR bmaa
                           WHERE bmaa.SERIES_ID = s.ID)),
                    ''
                  ) AS BOOKS_METADATA_AUTHORS,
                   COALESCE((SELECT GROUP_CONCAT(TAG, char(30))
                             FROM (SELECT DISTINCT bmat.TAG AS TAG
                                   FROM BOOK_METADATA_AGGREGATION_TAG bmat
                                   WHERE bmat.SERIES_ID = s.ID)), '') AS BOOKS_METADATA_TAGS,
                   sm.STATUS_LOCK AS STATUS_LOCK,
                   sm.TITLE_LOCK AS TITLE_LOCK,
                   sm.TITLE_SORT_LOCK AS TITLE_SORT_LOCK,
                   sm.SUMMARY_LOCK AS SUMMARY_LOCK,
                   sm.READING_DIRECTION_LOCK AS READING_DIRECTION_LOCK,
                   sm.PUBLISHER_LOCK AS PUBLISHER_LOCK,
                   sm.AGE_RATING_LOCK AS AGE_RATING_LOCK,
                   sm.LANGUAGE_LOCK AS LANGUAGE_LOCK,
                   sm.GENRES_LOCK AS GENRES_LOCK,
                   sm.TAGS_LOCK AS TAGS_LOCK,
                   sm.TOTAL_BOOK_COUNT_LOCK AS TOTAL_BOOK_COUNT_LOCK,
                   sm.SHARING_LABELS_LOCK AS SHARING_LABELS_LOCK,
                   sm.LINKS_LOCK AS LINKS_LOCK,
                   sm.ALTERNATE_TITLES_LOCK AS ALTERNATE_TITLES_LOCK,
                   COALESCE(
                     (SELECT GROUP_CONCAT(LABEL || char(30) || URL, char(31))
                      FROM SERIES_METADATA_LINK sml
                      WHERE sml.SERIES_ID = s.ID),
                     ''
                   ) AS LINKS
            FROM SERIES s
            LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
            LEFT JOIN BOOK_METADATA_AGGREGATION bma ON bma.SERIES_ID = s.ID"#,
    );

    if let Some(ids) = ids.filter(|ids| !ids.is_empty()) {
        query.push(" WHERE s.ID IN (");
        let mut separated = query.separated(",");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");
    }

    query.build().fetch_all(pool).await
}

pub(crate) async fn load_persisted_series_read_models(
    pool: &SqlitePool,
) -> anyhow::Result<Vec<SeriesReadModel>> {
    load_persisted_series_summaries(pool)
        .await
        .map(|summaries| summaries.into_iter().map(series_read_model).collect())
}

pub(super) async fn load_persisted_series_count(pool: &SqlitePool) -> anyhow::Result<usize> {
    let row = sqlx::query("SELECT COUNT(*) AS COUNT FROM SERIES")
        .fetch_one(pool)
        .await
        .context("query persisted series count")?;
    Ok(row.get::<i64, _>("COUNT").max(0) as usize)
}

fn series_read_model(summary: SeriesSummary) -> SeriesReadModel {
    SeriesReadModel {
        id: summary.id,
        library_id: summary.library_id,
        name: summary.name,
        title: summary.title,
        title_sort: summary.title_sort,
        labels: summary.labels,
        created: summary.created,
        last_modified: summary.last_modified,
        file_last_modified: summary.file_last_modified,
        books_count: summary.books_count,
        books_read_count: summary.books_read_count,
        books_unread_count: summary.books_unread_count,
        books_in_progress_count: summary.books_in_progress_count,
        status: SeriesStatus::parse(&summary.status).unwrap_or(SeriesStatus::Ongoing),
        summary: summary.summary,
        reading_direction: SeriesReadingDirection::parse(&summary.reading_direction),
        publisher: summary.publisher,
        age_rating: summary.age_rating,
        language: summary.language,
        genres: summary.genres,
        tags: summary.tags,
        alternate_titles: summary.alternate_titles,
        metadata_created: summary.metadata_created,
        metadata_last_modified: summary.metadata_last_modified,
        books_metadata_authors: summary.books_metadata_authors,
        books_metadata_tags: summary.books_metadata_tags,
        books_metadata_release_date: summary.books_metadata_release_date,
        books_metadata_summary: summary.books_metadata_summary,
        books_metadata_summary_number: summary.books_metadata_summary_number,
        books_metadata_created: summary.books_metadata_created,
        books_metadata_last_modified: summary.books_metadata_last_modified,
        deleted: summary.deleted,
        oneshot: summary.oneshot,
        status_lock: summary.status_lock,
        title_lock: summary.title_lock,
        title_sort_lock: summary.title_sort_lock,
        summary_lock: summary.summary_lock,
        reading_direction_lock: summary.reading_direction_lock,
        publisher_lock: summary.publisher_lock,
        age_rating_lock: summary.age_rating_lock,
        language_lock: summary.language_lock,
        genres_lock: summary.genres_lock,
        tags_lock: summary.tags_lock,
        total_book_count_lock: summary.total_book_count_lock,
        sharing_labels_lock: summary.sharing_labels_lock,
        links_lock: summary.links_lock,
        alternate_titles_lock: summary.alternate_titles_lock,
        links: summary
            .links
            .into_iter()
            .map(|link| BookMetadataLinkReadModel {
                label: link.label,
                url: link.url,
            })
            .collect(),
    }
}

fn map_series_summary(row: SqliteRow) -> SeriesSummary {
    let links = row
        .get::<String, _>("LINKS")
        .split(char::from(31))
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| {
            let (label, url) = entry.split_once(char::from(30))?;
            SeriesSummaryLink {
                label: label.to_string(),
                url: url.to_string(),
            }
        })
        .collect();

    SeriesSummary {
        id: row.get::<String, _>("ID"),
        library_id: row.get::<String, _>("LIBRARY_ID"),
        name: row.get::<String, _>("NAME"),
        title: row.get::<String, _>("TITLE"),
        title_sort: row.get::<String, _>("TITLE_SORT"),
        labels: common::parse_group_concat_values(&row.get::<String, _>("LABELS")),
        created: row.get::<String, _>("CREATED_DATE"),
        last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
        file_last_modified: row.get::<String, _>("FILE_LAST_MODIFIED"),
        books_count: row.get::<i64, _>("BOOK_COUNT").max(0) as u64,
        books_read_count: 0,
        books_unread_count: row.get::<i64, _>("BOOK_COUNT").max(0) as u64,
        books_in_progress_count: 0,
        status: row.get::<String, _>("STATUS"),
        summary: row.get::<String, _>("SUMMARY"),
        reading_direction: row.get::<String, _>("READING_DIRECTION"),
        publisher: row.get::<String, _>("PUBLISHER"),
        age_rating: row
            .get::<Option<i64>, _>("AGE_RATING")
            .map(common::clamp_kotlin_int_u32),
        language: row.get::<String, _>("LANGUAGE"),
        genres: common::parse_group_concat_values(&row.get::<String, _>("GENRES")),
        tags: common::parse_group_concat_values(&row.get::<String, _>("TAGS")),
        alternate_titles: common::parse_group_concat_values(
            &row.get::<String, _>("ALTERNATE_TITLES"),
        ),
        metadata_created: row.get::<String, _>("METADATA_CREATED"),
        metadata_last_modified: row.get::<String, _>("METADATA_LAST_MODIFIED"),
        books_metadata_authors: common::parse_group_concat_values(
            &row.get::<String, _>("BOOKS_METADATA_AUTHORS"),
        ),
        books_metadata_tags: common::parse_group_concat_values(
            &row.get::<String, _>("BOOKS_METADATA_TAGS"),
        ),
        books_metadata_release_date: row.get::<Option<String>, _>("BOOKS_METADATA_RELEASE_DATE"),
        books_metadata_summary: row.get::<String, _>("BOOKS_METADATA_SUMMARY"),
        books_metadata_summary_number: row.get::<String, _>("BOOKS_METADATA_SUMMARY_NUMBER"),
        books_metadata_created: row.get::<String, _>("BOOKS_METADATA_CREATED"),
        books_metadata_last_modified: row.get::<String, _>("BOOKS_METADATA_LAST_MODIFIED"),
        deleted: row.get::<Option<String>, _>("DELETED_DATE").is_some(),
        oneshot: row.get::<i64, _>("ONESHOT") != 0,
        status_lock: row.get::<i64, _>("STATUS_LOCK") != 0,
        title_lock: row.get::<i64, _>("TITLE_LOCK") != 0,
        title_sort_lock: row.get::<i64, _>("TITLE_SORT_LOCK") != 0,
        summary_lock: row.get::<i64, _>("SUMMARY_LOCK") != 0,
        reading_direction_lock: row.get::<i64, _>("READING_DIRECTION_LOCK") != 0,
        publisher_lock: row.get::<i64, _>("PUBLISHER_LOCK") != 0,
        age_rating_lock: row.get::<i64, _>("AGE_RATING_LOCK") != 0,
        language_lock: row.get::<i64, _>("LANGUAGE_LOCK") != 0,
        genres_lock: row.get::<i64, _>("GENRES_LOCK") != 0,
        tags_lock: row.get::<i64, _>("TAGS_LOCK") != 0,
        total_book_count_lock: row.get::<i64, _>("TOTAL_BOOK_COUNT_LOCK") != 0,
        sharing_labels_lock: row.get::<i64, _>("SHARING_LABELS_LOCK") != 0,
        links_lock: row.get::<i64, _>("LINKS_LOCK") != 0,
        alternate_titles_lock: row.get::<i64, _>("ALTERNATE_TITLES_LOCK") != 0,
        links,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::BootstrappedBookFixture;

    #[tokio::test]
    async fn load_persisted_series_summaries_preserves_commas_in_metadata_values() {
        let fixture = BootstrappedBookFixture::open("series-summary-comma-values").await;
        fixture.insert_library_series().await;
        fixture.insert_series_metadata().await;

        sqlx::query("INSERT INTO SERIES_METADATA_SHARING (SERIES_ID, LABEL) VALUES (?, ?)")
            .bind("series-1")
            .bind("Kids, Family")
            .execute(&fixture.pool)
            .await
            .expect("sharing label should be inserted");
        sqlx::query("INSERT INTO SERIES_METADATA_GENRE (SERIES_ID, GENRE) VALUES (?, ?)")
            .bind("series-1")
            .bind("Sci, Fi")
            .execute(&fixture.pool)
            .await
            .expect("genre should be inserted");
        sqlx::query("INSERT INTO SERIES_METADATA_TAG (SERIES_ID, TAG) VALUES (?, ?)")
            .bind("series-1")
            .bind("Slice, Life")
            .execute(&fixture.pool)
            .await
            .expect("tag should be inserted");

        let summaries = load_persisted_series_summaries(&fixture.pool)
            .await
            .expect("series summary should load");
        let summary = summaries
            .first()
            .expect("series summary should include seeded series");

        assert_eq!(summary.labels, vec!["Kids, Family"]);
        assert_eq!(summary.genres, vec!["Sci, Fi"]);
        assert_eq!(summary.tags, vec!["Slice, Life"]);
        fixture.close().await;
    }
}
