use std::collections::HashSet;
use std::path::Path;

use sqlx::Row;

use super::run_database_query;

pub fn aggregate_series_metadata(database_file: &Path, series_id: &str) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let series_id = series_id.to_string();

    run_database_query(database_file, move |pool| {
        let series_id = series_id.clone();
        Box::pin(async move {
            let mut tx = pool.begin().await.map_err(|error| {
                format!(
                    "failed to start series metadata aggregation transaction for '{series_id}': {error}"
                )
            })?;

            let row = sqlx::query(
                r#"
                SELECT ID
                FROM SERIES
                WHERE ID = ?
                LIMIT 1
                "#,
            )
            .bind(&series_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| {
                format!("failed to load series for aggregation '{series_id}': {error}")
            })?;

            let Some(row) = row else {
                return Ok(());
            };

            let _series_id = row.get::<String, _>("ID");
            let aggregate = load_series_book_metadata_aggregate(&mut tx, &series_id).await?;

            sqlx::query(
                r#"
                INSERT INTO BOOK_METADATA_AGGREGATION (
                    SERIES_ID,
                    RELEASE_DATE,
                    SUMMARY,
                    SUMMARY_NUMBER,
                    LAST_MODIFIED_DATE
                )
                VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP)
                ON CONFLICT(SERIES_ID) DO UPDATE SET
                    RELEASE_DATE = excluded.RELEASE_DATE,
                    SUMMARY = excluded.SUMMARY,
                    SUMMARY_NUMBER = excluded.SUMMARY_NUMBER,
                    LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                "#,
            )
            .bind(&series_id)
            .bind(aggregate.release_date.as_deref())
            .bind(&aggregate.summary)
            .bind(&aggregate.summary_number)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                format!("failed to upsert BOOK_METADATA_AGGREGATION for '{series_id}': {error}")
            })?;

            sqlx::query("DELETE FROM BOOK_METADATA_AGGREGATION_AUTHOR WHERE SERIES_ID = ?")
                .bind(&series_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    format!(
                        "failed to clear BOOK_METADATA_AGGREGATION_AUTHOR for '{series_id}': {error}"
                    )
                })?;

            for author in aggregate.authors {
                sqlx::query(
                    r#"
                    INSERT INTO BOOK_METADATA_AGGREGATION_AUTHOR (SERIES_ID, NAME, ROLE)
                    VALUES (?, ?, ?)
                    "#,
                )
                .bind(&series_id)
                .bind(author.name)
                .bind(author.role)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    format!(
                        "failed to populate BOOK_METADATA_AGGREGATION_AUTHOR for '{series_id}': {error}"
                    )
                })?;
            }

            sqlx::query("DELETE FROM BOOK_METADATA_AGGREGATION_TAG WHERE SERIES_ID = ?")
                .bind(&series_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    format!(
                        "failed to clear BOOK_METADATA_AGGREGATION_TAG for '{series_id}': {error}"
                    )
                })?;

            for tag in aggregate.tags {
                sqlx::query(
                    r#"
                    INSERT INTO BOOK_METADATA_AGGREGATION_TAG (SERIES_ID, TAG)
                    VALUES (?, ?)
                    "#,
                )
                .bind(&series_id)
                .bind(tag)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    format!(
                        "failed to populate BOOK_METADATA_AGGREGATION_TAG for '{series_id}': {error}"
                    )
                })?;
            }

            sqlx::query(
                r#"
                UPDATE SERIES
                SET BOOK_COUNT = (
                        SELECT COUNT(*)
                        FROM BOOK
                        WHERE BOOK.SERIES_ID = SERIES.ID
                          AND BOOK.DELETED_DATE IS NULL
                    ),
                    LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE ID = ?
                "#,
            )
            .bind(&series_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                format!("failed to aggregate SERIES counters for '{series_id}': {error}")
            })?;

            tx.commit().await.map_err(|error| {
                format!(
                    "failed to commit series metadata aggregation transaction for '{series_id}': {error}"
                )
            })?;

            Ok(())
        })
    })
}

#[derive(Default)]
struct SeriesBookMetadataAggregate {
    authors: Vec<AggregatedAuthor>,
    tags: Vec<String>,
    release_date: Option<String>,
    summary: String,
    summary_number: String,
}

struct AggregatedAuthor {
    name: String,
    role: String,
}

async fn load_series_book_metadata_aggregate(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    series_id: &str,
) -> Result<SeriesBookMetadataAggregate, String> {
    let metadata_rows = sqlx::query(
        r#"
        SELECT COALESCE(bm.NUMBER, '') AS NUMBER,
               bm.NUMBER_SORT AS NUMBER_SORT,
               COALESCE(bm.SUMMARY, '') AS SUMMARY,
               bm.RELEASE_DATE AS RELEASE_DATE
        FROM BOOK b
        JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
        WHERE b.SERIES_ID = ?
          AND b.DELETED_DATE IS NULL
        ORDER BY bm.NUMBER_SORT ASC, b.ID ASC
        "#,
    )
    .bind(series_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| format!("failed to load book metadata rows for '{series_id}': {error}"))?;

    let mut summary = String::new();
    let mut summary_number = String::new();
    let mut release_date: Option<String> = None;

    for row in metadata_rows {
        let row_summary = row.get::<String, _>("SUMMARY");
        if summary.is_empty() && !row_summary.trim().is_empty() {
            summary = row_summary;
            summary_number = row.get::<String, _>("NUMBER");
        }

        if let Some(row_release_date) = row.get::<Option<String>, _>("RELEASE_DATE")
            && release_date
                .as_ref()
                .is_none_or(|current| row_release_date < *current)
        {
            release_date = Some(row_release_date);
        }
    }

    let author_rows = sqlx::query(
        r#"
        SELECT bmaa.NAME AS NAME,
               bmaa.ROLE AS ROLE
        FROM BOOK b
        JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
        JOIN BOOK_METADATA_AUTHOR bmaa ON bmaa.BOOK_ID = b.ID
        WHERE b.SERIES_ID = ?
          AND b.DELETED_DATE IS NULL
        ORDER BY bm.NUMBER_SORT ASC, b.ID ASC, bmaa.ROWID ASC
        "#,
    )
    .bind(series_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| format!("failed to load aggregated authors for '{series_id}': {error}"))?;

    let mut authors = Vec::new();
    let mut seen_authors = HashSet::new();
    for row in author_rows {
        let name = row.get::<String, _>("NAME");
        let role = row.get::<String, _>("ROLE");
        let dedupe_key = format!("{role}__{name}");
        if seen_authors.insert(dedupe_key) {
            authors.push(AggregatedAuthor { name, role });
        }
    }

    let tag_rows = sqlx::query(
        r#"
        SELECT bmt.TAG AS TAG
        FROM BOOK b
        JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
        JOIN BOOK_METADATA_TAG bmt ON bmt.BOOK_ID = b.ID
        WHERE b.SERIES_ID = ?
          AND b.DELETED_DATE IS NULL
        ORDER BY bm.NUMBER_SORT ASC, b.ID ASC, bmt.ROWID ASC
        "#,
    )
    .bind(series_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| format!("failed to load aggregated tags for '{series_id}': {error}"))?;

    let mut tags = Vec::new();
    let mut seen_tags = HashSet::new();
    for row in tag_rows {
        let tag = row.get::<String, _>("TAG");
        if seen_tags.insert(tag.clone()) {
            tags.push(tag);
        }
    }

    Ok(SeriesBookMetadataAggregate {
        authors,
        tags,
        release_date,
        summary,
        summary_number,
    })
}
