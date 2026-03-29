use std::fs;
use std::path::{Path, PathBuf};

use sqlx::{Row, SqlitePool};

use crate::sqlite::connect_pool;

pub fn refresh_book_metadata(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<String>, String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        Box::pin(async move {
            let book_row = sqlx::query(
                "SELECT b.URL AS BOOK_URL, l.ROOT AS LIBRARY_ROOT\n                 FROM BOOK b\n                 JOIN LIBRARY l ON l.ID = b.LIBRARY_ID\n                 WHERE b.ID = ?\n                 LIMIT 1",
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| format!("failed to resolve book path for metadata refresh '{book_id}': {error}"))?;

            if let Some(book_row) = &book_row {
                let book_url = book_row.get::<String, _>("BOOK_URL");
                let library_root = book_row.get::<String, _>("LIBRARY_ROOT");
                if let Some(sidecar_url) =
                    load_sidecar_url_for_parent(&pool, &book_url, true).await?
                {
                    let sidecar_path = PathBuf::from(library_root).join(sidecar_url);
                    if let Ok(xml) = fs::read_to_string(&sidecar_path) {
                        let title = extract_xml_tag(&xml, "Title").unwrap_or_default();
                        let summary = extract_xml_tag(&xml, "Summary").unwrap_or_default();
                        if !title.is_empty() || !summary.is_empty() {
                            sqlx::query(
                                "UPDATE BOOK_METADATA\n                                 SET TITLE = CASE WHEN TITLE_LOCK = 0 AND ? <> '' THEN ? ELSE TITLE END,\n                                     SUMMARY = CASE WHEN SUMMARY_LOCK = 0 AND ? <> '' THEN ? ELSE SUMMARY END,\n                                     LAST_MODIFIED_DATE = CURRENT_TIMESTAMP\n                                 WHERE BOOK_ID = ?",
                            )
                            .bind(&title)
                            .bind(&title)
                            .bind(&summary)
                            .bind(&summary)
                            .bind(&book_id)
                            .execute(&pool)
                            .await
                            .map_err(|error| format!("failed to apply sidecar metadata for '{book_id}': {error}"))?;
                        }
                    }
                }
            }

            sqlx::query(
                "UPDATE BOOK_METADATA\n                 SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP\n                 WHERE BOOK_ID = ?",
            )
            .bind(&book_id)
            .execute(&pool)
            .await
            .map_err(|error| format!("failed to refresh BOOK_METADATA for '{book_id}': {error}"))?;

            sqlx::query(
                "UPDATE BOOK\n                 SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP\n                 WHERE ID = ?",
            )
            .bind(&book_id)
            .execute(&pool)
            .await
            .map_err(|error| format!("failed to refresh BOOK row timestamp for '{book_id}': {error}"))?;

            let series_id = sqlx::query(
                "SELECT SERIES_ID\n                 FROM BOOK\n                 WHERE ID = ?\n                 LIMIT 1",
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| format!("failed to resolve SERIES_ID for '{book_id}': {error}"))?
            .and_then(|row| row.get::<Option<String>, _>("SERIES_ID"));

            Ok(series_id)
        })
    })
}

pub fn refresh_series_metadata(database_file: &Path, series_id: &str) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let series_id = series_id.to_string();

    run_database_query(database_file, move |pool| {
        let series_id = series_id.clone();
        Box::pin(async move {
            let series_row = sqlx::query(
                "SELECT s.URL AS SERIES_URL, l.ROOT AS LIBRARY_ROOT\n                 FROM SERIES s\n                 JOIN LIBRARY l ON l.ID = s.LIBRARY_ID\n                 WHERE s.ID = ?\n                 LIMIT 1",
            )
            .bind(&series_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| format!("failed to resolve series path for metadata refresh '{series_id}': {error}"))?;

            if let Some(series_row) = &series_row {
                let series_url = series_row.get::<String, _>("SERIES_URL");
                let library_root = series_row.get::<String, _>("LIBRARY_ROOT");
                if let Some(sidecar_url) =
                    load_sidecar_url_for_parent(&pool, &series_url, true).await?
                {
                    let sidecar_path = PathBuf::from(library_root).join(sidecar_url);
                    if let Ok(xml) = fs::read_to_string(&sidecar_path) {
                        let title = extract_xml_tag(&xml, "Title").unwrap_or_default();
                        let summary = extract_xml_tag(&xml, "Summary").unwrap_or_default();
                        if !title.is_empty() || !summary.is_empty() {
                            sqlx::query(
                                "UPDATE SERIES_METADATA\n                                 SET TITLE = CASE WHEN TITLE_LOCK = 0 AND ? <> '' THEN ? ELSE TITLE END,\n                                     TITLE_SORT = CASE WHEN TITLE_SORT_LOCK = 0 AND ? <> '' THEN ? ELSE TITLE_SORT END,\n                                     SUMMARY = CASE WHEN SUMMARY_LOCK = 0 AND ? <> '' THEN ? ELSE SUMMARY END,\n                                     LAST_MODIFIED_DATE = CURRENT_TIMESTAMP\n                                 WHERE SERIES_ID = ?",
                            )
                            .bind(&title)
                            .bind(&title)
                            .bind(&title)
                            .bind(&title)
                            .bind(&summary)
                            .bind(&summary)
                            .bind(&series_id)
                            .execute(&pool)
                            .await
                            .map_err(|error| format!("failed to apply series sidecar metadata for '{series_id}': {error}"))?;
                        }
                    }
                }
            }

            sqlx::query(
                "UPDATE SERIES_METADATA\n                 SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP\n                 WHERE SERIES_ID = ?",
            )
            .bind(&series_id)
            .execute(&pool)
            .await
            .map_err(|error| format!("failed to refresh SERIES_METADATA for '{series_id}': {error}"))?;

            sqlx::query(
                "UPDATE SERIES\n                 SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP\n                 WHERE ID = ?",
            )
            .bind(&series_id)
            .execute(&pool)
            .await
            .map_err(|error| format!("failed to refresh SERIES row for '{series_id}': {error}"))?;

            Ok(())
        })
    })
}

pub fn aggregate_series_metadata(database_file: &Path, series_id: &str) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let series_id = series_id.to_string();

    run_database_query(database_file, move |pool| {
        let series_id = series_id.clone();
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT NAME\n                 FROM SERIES\n                 WHERE ID = ?\n                 LIMIT 1",
            )
            .bind(&series_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| format!("failed to load series for aggregation '{series_id}': {error}"))?;

            let Some(row) = row else {
                return Ok(());
            };

            let series_name = row.get::<String, _>("NAME");

            sqlx::query(
                "UPDATE SERIES_METADATA\n                 SET TITLE = CASE WHEN TITLE_LOCK = 0 THEN ? ELSE TITLE END,\n                     TITLE_SORT = CASE WHEN TITLE_SORT_LOCK = 0 THEN ? ELSE TITLE_SORT END,\n                     LAST_MODIFIED_DATE = CURRENT_TIMESTAMP\n                 WHERE SERIES_ID = ?",
            )
            .bind(&series_name)
            .bind(&series_name)
            .bind(&series_id)
            .execute(&pool)
            .await
            .map_err(|error| format!("failed to aggregate SERIES_METADATA for '{series_id}': {error}"))?;

            sqlx::query(
                "UPDATE SERIES\n                 SET BOOK_COUNT = (SELECT COUNT(*)\n                                   FROM BOOK\n                                   WHERE BOOK.SERIES_ID = SERIES.ID\n                                     AND BOOK.DELETED_DATE IS NULL),\n                     LAST_MODIFIED_DATE = CURRENT_TIMESTAMP\n                 WHERE ID = ?",
            )
            .bind(&series_id)
            .execute(&pool)
            .await
            .map_err(|error| format!("failed to aggregate SERIES counters for '{series_id}': {error}"))?;

            Ok(())
        })
    })
}

pub fn refresh_book_local_artwork(database_file: &Path, book_id: &str) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        Box::pin(async move {
            let book_row = sqlx::query(
                "SELECT b.URL AS BOOK_URL, l.ROOT AS LIBRARY_ROOT\n                 FROM BOOK b\n                 JOIN LIBRARY l ON l.ID = b.LIBRARY_ID\n                 WHERE b.ID = ?\n                 LIMIT 1",
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| format!("failed to resolve book path for artwork refresh '{book_id}': {error}"))?;

            if let Some(book_row) = &book_row {
                let book_url = book_row.get::<String, _>("BOOK_URL");
                let library_root = book_row.get::<String, _>("LIBRARY_ROOT");
                if let Some(sidecar_url) =
                    load_sidecar_url_for_parent(&pool, &book_url, false).await?
                {
                    let sidecar_path = PathBuf::from(library_root).join(&sidecar_url);
                    if let Ok(meta) = fs::metadata(&sidecar_path) {
                        let media_type = media_type_from_sidecar_path(sidecar_path.as_path());
                        let thumbnail_id = format!("thumbnail-book-sidecar:{book_id}");
                        sqlx::query(
                            "INSERT OR REPLACE INTO THUMBNAIL_BOOK\n                             (ID, URL, SELECTED, TYPE, BOOK_ID, MEDIA_TYPE, FILE_SIZE, LAST_MODIFIED_DATE)\n                             VALUES (?, ?, 1, 'SIDECAR', ?, ?, ?, CURRENT_TIMESTAMP)",
                        )
                        .bind(&thumbnail_id)
                        .bind(sidecar_url)
                        .bind(&book_id)
                        .bind(media_type)
                        .bind(meta.len() as i64)
                        .execute(&pool)
                        .await
                        .map_err(|error| format!("failed to upsert sidecar thumbnail for book '{book_id}': {error}"))?;
                    }
                }
            }

            sqlx::query(
                "UPDATE THUMBNAIL_BOOK\n                 SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP\n                 WHERE BOOK_ID = ?",
            )
            .bind(&book_id)
            .execute(&pool)
            .await
            .map_err(|error| format!("failed to refresh THUMBNAIL_BOOK rows for '{book_id}': {error}"))?;

            sqlx::query(
                "UPDATE BOOK\n                 SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP\n                 WHERE ID = ?",
            )
            .bind(&book_id)
            .execute(&pool)
            .await
            .map_err(|error| format!("failed to refresh BOOK row while updating local artwork for '{book_id}': {error}"))?;

            Ok(())
        })
    })
}

pub fn refresh_series_local_artwork(database_file: &Path, series_id: &str) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let series_id = series_id.to_string();

    run_database_query(database_file, move |pool| {
        let series_id = series_id.clone();
        Box::pin(async move {
            let series_row = sqlx::query(
                "SELECT s.URL AS SERIES_URL, l.ROOT AS LIBRARY_ROOT\n                 FROM SERIES s\n                 JOIN LIBRARY l ON l.ID = s.LIBRARY_ID\n                 WHERE s.ID = ?\n                 LIMIT 1",
            )
            .bind(&series_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| format!("failed to resolve series path for artwork refresh '{series_id}': {error}"))?;

            if let Some(series_row) = &series_row {
                let series_url = series_row.get::<String, _>("SERIES_URL");
                let library_root = series_row.get::<String, _>("LIBRARY_ROOT");
                if let Some(sidecar_url) =
                    load_sidecar_url_for_parent(&pool, &series_url, false).await?
                {
                    let sidecar_path = PathBuf::from(library_root).join(&sidecar_url);
                    if let Ok(meta) = fs::metadata(&sidecar_path) {
                        let media_type = media_type_from_sidecar_path(sidecar_path.as_path());
                        let thumbnail_id = format!("thumbnail-series-sidecar:{series_id}");
                        sqlx::query(
                            "INSERT OR REPLACE INTO THUMBNAIL_SERIES\n                             (ID, URL, SELECTED, TYPE, SERIES_ID, MEDIA_TYPE, FILE_SIZE, LAST_MODIFIED_DATE)\n                             VALUES (?, ?, 1, 'SIDECAR', ?, ?, ?, CURRENT_TIMESTAMP)",
                        )
                        .bind(&thumbnail_id)
                        .bind(sidecar_url)
                        .bind(&series_id)
                        .bind(media_type)
                        .bind(meta.len() as i64)
                        .execute(&pool)
                        .await
                        .map_err(|error| format!("failed to upsert sidecar thumbnail for series '{series_id}': {error}"))?;
                    }
                }
            }

            sqlx::query(
                "UPDATE THUMBNAIL_SERIES\n                 SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP\n                 WHERE SERIES_ID = ?",
            )
            .bind(&series_id)
            .execute(&pool)
            .await
            .map_err(|error| format!("failed to refresh THUMBNAIL_SERIES rows for '{series_id}': {error}"))?;

            sqlx::query(
                "UPDATE SERIES\n                 SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP\n                 WHERE ID = ?",
            )
            .bind(&series_id)
            .execute(&pool)
            .await
            .map_err(|error| format!("failed to refresh SERIES row while updating local artwork for '{series_id}': {error}"))?;

            Ok(())
        })
    })
}

type BoxFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, String>> + Send>>;

fn run_database_query<T>(
    database_file: PathBuf,
    operation: impl FnOnce(SqlitePool) -> BoxFuture<T> + Send + 'static,
) -> Result<T, String>
where
    T: Send + 'static,
{
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("failed to build metadata runtime: {error}"))?;

        runtime.block_on(async move {
            let pool = connect_pool(&database_file, 1)
                .await
                .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
            operation(pool).await
        })
    })
    .join()
    .map_err(|_| "metadata worker thread panicked".to_string())?
}

async fn load_sidecar_url_for_parent(
    pool: &SqlitePool,
    parent_url: &str,
    metadata_only: bool,
) -> Result<Option<String>, String> {
    let sql = if metadata_only {
        "SELECT URL\n         FROM SIDECAR\n         WHERE PARENT_URL = ?\n           AND LOWER(URL) LIKE '%.xml'\n         ORDER BY LAST_MODIFIED_TIME DESC\n         LIMIT 1"
    } else {
        "SELECT URL\n         FROM SIDECAR\n         WHERE PARENT_URL = ?\n           AND (LOWER(URL) LIKE '%.jpg' OR LOWER(URL) LIKE '%.jpeg' OR LOWER(URL) LIKE '%.png'\n                OR LOWER(URL) LIKE '%.webp' OR LOWER(URL) LIKE '%.gif' OR LOWER(URL) LIKE '%.avif')\n         ORDER BY LAST_MODIFIED_TIME DESC\n         LIMIT 1"
    };

    let row = sqlx::query(sql)
        .bind(parent_url)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("failed to load sidecar for '{parent_url}': {error}"))?;
    Ok(row.map(|row| row.get::<String, _>("URL")))
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    let value = xml[start..end].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn media_type_from_sidecar_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("avif") => "image/avif",
        _ => "application/octet-stream",
    }
}
