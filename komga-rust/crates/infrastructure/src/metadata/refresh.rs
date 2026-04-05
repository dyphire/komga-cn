#![allow(clippy::type_complexity)]

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use komga_application::media_assets::{
    BookMediaRecord, BookPageRecord, book_media_is_epub, book_media_is_pdf,
    book_media_is_single_image, content_type_from_filename,
};
use pdfium_render::prelude::*;
use sqlx::{Row, SqlitePool};

use crate::filesystem::{load_archive_page_row, load_epub_cover_bytes, resolve_book_page_bytes};
use crate::load_pdfium;
use crate::sqlite::connect_pool;

fn thumbnail_max_edge_from_setting(value: Option<&str>) -> u32 {
    match value.unwrap_or("DEFAULT") {
        "MEDIUM" => 600,
        "LARGE" => 900,
        "XLARGE" => 1200,
        _ => 300,
    }
}

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
                r#"
                SELECT b.URL AS BOOK_URL, l.ROOT AS LIBRARY_ROOT
                FROM BOOK b
                JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
                WHERE b.ID = ?
                LIMIT 1
                "#,
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to resolve book path for metadata refresh '{book_id}': {error}")
            })?;

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
                                r#"
                                UPDATE BOOK_METADATA
                                SET TITLE = CASE WHEN TITLE_LOCK = 0 AND ? <> '' THEN ? ELSE TITLE END,
                                    SUMMARY = CASE WHEN SUMMARY_LOCK = 0 AND ? <> '' THEN ? ELSE SUMMARY END,
                                    LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                                WHERE BOOK_ID = ?
                                "#,
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
                r#"
                UPDATE BOOK_METADATA
                SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE BOOK_ID = ?
                "#,
            )
            .bind(&book_id)
            .execute(&pool)
            .await
            .map_err(|error| format!("failed to refresh BOOK_METADATA for '{book_id}': {error}"))?;

            sqlx::query(
                r#"
                UPDATE BOOK
                SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE ID = ?
                "#,
            )
            .bind(&book_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                format!("failed to refresh BOOK row timestamp for '{book_id}': {error}")
            })?;

            let series_id = sqlx::query(
                r#"
                SELECT SERIES_ID
                FROM BOOK
                WHERE ID = ?
                LIMIT 1
                "#,
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
                r#"
                SELECT s.URL AS SERIES_URL, l.ROOT AS LIBRARY_ROOT
                FROM SERIES s
                JOIN LIBRARY l ON l.ID = s.LIBRARY_ID
                WHERE s.ID = ?
                LIMIT 1
                "#,
            )
            .bind(&series_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to resolve series path for metadata refresh '{series_id}': {error}")
            })?;

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
                                r#"
                                UPDATE SERIES_METADATA
                                SET TITLE = CASE WHEN TITLE_LOCK = 0 AND ? <> '' THEN ? ELSE TITLE END,
                                    TITLE_SORT = CASE WHEN TITLE_SORT_LOCK = 0 AND ? <> '' THEN ? ELSE TITLE_SORT END,
                                    SUMMARY = CASE WHEN SUMMARY_LOCK = 0 AND ? <> '' THEN ? ELSE SUMMARY END,
                                    LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                                WHERE SERIES_ID = ?
                                "#,
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
                r#"
                UPDATE SERIES_METADATA
                SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE SERIES_ID = ?
                "#,
            )
            .bind(&series_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                format!("failed to refresh SERIES_METADATA for '{series_id}': {error}")
            })?;

            sqlx::query(
                r#"
                UPDATE SERIES
                SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE ID = ?
                "#,
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
                r#"
                SELECT NAME
                FROM SERIES
                WHERE ID = ?
                LIMIT 1
                "#,
            )
            .bind(&series_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to load series for aggregation '{series_id}': {error}")
            })?;

            let Some(row) = row else {
                return Ok(());
            };

            let series_name = row.get::<String, _>("NAME");

            sqlx::query(
                r#"
                UPDATE SERIES_METADATA
                SET TITLE = CASE WHEN TITLE_LOCK = 0 THEN ? ELSE TITLE END,
                    TITLE_SORT = CASE WHEN TITLE_SORT_LOCK = 0 THEN ? ELSE TITLE_SORT END,
                    LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE SERIES_ID = ?
                "#,
            )
            .bind(&series_name)
            .bind(&series_name)
            .bind(&series_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                format!("failed to aggregate SERIES_METADATA for '{series_id}': {error}")
            })?;

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
            .execute(&pool)
            .await
            .map_err(|error| {
                format!("failed to aggregate SERIES counters for '{series_id}': {error}")
            })?;

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
                r#"
                SELECT b.URL AS BOOK_URL, l.ROOT AS LIBRARY_ROOT
                FROM BOOK b
                JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
                WHERE b.ID = ?
                LIMIT 1
                "#,
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to resolve book path for artwork refresh '{book_id}': {error}")
            })?;

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
                            r#"
                            INSERT OR REPLACE INTO THUMBNAIL_BOOK
                                (ID, URL, SELECTED, TYPE, BOOK_ID, MEDIA_TYPE, FILE_SIZE, LAST_MODIFIED_DATE)
                            VALUES (?, ?, 1, 'SIDECAR', ?, ?, ?, CURRENT_TIMESTAMP)
                            "#,
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
                r#"
                UPDATE THUMBNAIL_BOOK
                SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE BOOK_ID = ?
                "#,
            )
            .bind(&book_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                format!("failed to refresh THUMBNAIL_BOOK rows for '{book_id}': {error}")
            })?;

            sqlx::query(
                r#"
                UPDATE BOOK
                SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE ID = ?
                "#,
            )
            .bind(&book_id)
            .execute(&pool)
            .await
            .map_err(|error| format!("failed to refresh BOOK row while updating local artwork for '{book_id}': {error}"))?;

            Ok(())
        })
    })
}

pub fn generate_book_thumbnail(database_file: &Path, book_id: &str) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let book_id = book_id.to_string();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        Box::pin(async move {
            let media_row = sqlx::query(
                r#"
                SELECT b.LIBRARY_ID AS LIBRARY_ID,
                       b.NAME AS FILE_NAME,
                       b.URL AS BOOK_URL,
                       l.ROOT AS LIBRARY_ROOT,
                       COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
                       COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT
                FROM BOOK b
                JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
                LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
                WHERE b.ID = ?
                  AND b.DELETED_DATE IS NULL
                LIMIT 1
                "#,
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!(
                    "failed to resolve book media for thumbnail generation '{book_id}': {error}"
                )
            })?;

            let Some(media_row) = media_row else {
                return Ok(());
            };

            let media = BookMediaRecord {
                library_id: media_row.get::<String, _>("LIBRARY_ID"),
                media_type: media_row.get::<String, _>("MEDIA_TYPE"),
                file_path: PathBuf::from(media_row.get::<String, _>("LIBRARY_ROOT"))
                    .join(media_row.get::<String, _>("BOOK_URL")),
                file_name: media_row.get::<String, _>("FILE_NAME"),
                page_count: media_row.get::<i64, _>("PAGE_COUNT").max(0) as u64,
            };

            let thumbnail_size_setting = sqlx::query(
                r#"
                SELECT VALUE
                FROM SERVER_SETTINGS
                WHERE KEY = 'THUMBNAIL_SIZE'
                LIMIT 1
                "#,
            )
            .fetch_optional(&pool)
            .await
            .map_err(|error| format!("failed to load thumbnail size setting: {error}"))?
            .and_then(|row| row.get::<Option<String>, _>("VALUE"));
            let configured_max_edge =
                thumbnail_max_edge_from_setting(thumbnail_size_setting.as_deref());

            let epub_cover = if book_media_is_epub(&media) {
                load_epub_cover_bytes(&media)
            } else {
                None
            };

            let persisted_page_row = sqlx::query(
                r#"
                SELECT NUMBER,
                       FILE_NAME,
                       MEDIA_TYPE,
                       WIDTH,
                       HEIGHT,
                       COALESCE(FILE_SIZE, 0) AS FILE_SIZE
                FROM MEDIA_PAGE
                WHERE BOOK_ID = ? AND NUMBER = 1
                LIMIT 1
                "#,
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to resolve page row for thumbnail generation '{book_id}': {error}")
            })?;

            let (thumbnail_bytes, thumbnail_media_type, width, height) =
                if book_media_is_pdf(&media) {
                    let Some(rendered) = render_pdf_thumbnail(&media, configured_max_edge)? else {
                        return Ok(());
                    };
                    rendered
                } else if let Some((bytes, _media_type)) = epub_cover {
                    render_generated_thumbnail_from_image_bytes(
                        &book_id,
                        &bytes,
                        configured_max_edge,
                    )?
                } else {
                    let page_row = if let Some(row) = persisted_page_row {
                        Some(BookPageRecord {
                            number: row.get::<i64, _>("NUMBER") as u64,
                            file_name: row.get::<String, _>("FILE_NAME"),
                            media_type: row.get::<String, _>("MEDIA_TYPE"),
                            width: row.get::<Option<i64>, _>("width"),
                            height: row.get::<Option<i64>, _>("height"),
                            file_size: row.get::<i64, _>("FILE_SIZE"),
                        })
                    } else if book_media_is_single_image(&media) {
                        Some(BookPageRecord {
                            number: 1,
                            file_name: media.file_name.clone(),
                            media_type: content_type_from_filename(
                                &media.file_name,
                                &media.media_type,
                            ),
                            width: None,
                            height: None,
                            file_size: fs::metadata(&media.file_path)
                                .ok()
                                .map(|metadata| metadata.len() as i64)
                                .unwrap_or(0),
                        })
                    } else {
                        load_archive_page_row(&media, 1)
                    };

                    let Some(page_row) = page_row else {
                        return Ok(());
                    };
                    let Some(thumbnail_bytes) = resolve_book_page_bytes(&media, &page_row, 1)
                    else {
                        return Ok(());
                    };
                    let thumbnail_media_type = if page_row.media_type.is_empty() {
                        content_type_from_filename(&page_row.file_name, &media.media_type)
                    } else {
                        page_row.media_type.clone()
                    };
                    if !thumbnail_media_type
                        .to_ascii_lowercase()
                        .starts_with("image/")
                    {
                        return Ok(());
                    }
                    render_generated_thumbnail_from_image_bytes(
                        &book_id,
                        &thumbnail_bytes,
                        configured_max_edge,
                    )?
                };

            let selected_thumbnail_type = sqlx::query(
                r#"
                SELECT TYPE
                FROM THUMBNAIL_BOOK
                WHERE BOOK_ID = ? AND SELECTED = 1
                ORDER BY LAST_MODIFIED_DATE DESC, ID ASC
                LIMIT 1
                "#,
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to query selected thumbnail for '{book_id}': {error}")
            })?
            .map(|row| row.get::<String, _>("TYPE"));
            let should_select = selected_thumbnail_type
                .as_deref()
                .is_none_or(|thumbnail_type| thumbnail_type == "GENERATED");

            let mut tx = pool
                .begin()
                .await
                .map_err(|error| format!("begin generate thumbnail tx: {error}"))?;

            sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND TYPE = 'GENERATED'")
                .bind(&book_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    format!("failed to delete prior generated thumbnails for '{book_id}': {error}")
                })?;

            if should_select {
                sqlx::query("UPDATE THUMBNAIL_BOOK SET SELECTED = 0 WHERE BOOK_ID = ?")
                    .bind(&book_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| {
                        format!("failed to clear selected thumbnails for '{book_id}': {error}")
                    })?;
            }

            let thumbnail_id = format!("thumbnail-book-generated:{book_id}");
            sqlx::query(
                r#"
                INSERT INTO THUMBNAIL_BOOK
                    (ID, SELECTED, THUMBNAIL, TYPE, BOOK_ID, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT, LAST_MODIFIED_DATE)
                VALUES (?, ?, ?, 'GENERATED', ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
                "#,
            )
            .bind(&thumbnail_id)
            .bind(should_select)
            .bind(&thumbnail_bytes)
            .bind(&book_id)
            .bind(&thumbnail_media_type)
            .bind(thumbnail_bytes.len() as i64)
            .bind(width)
            .bind(height)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("failed to insert generated thumbnail for '{book_id}': {error}"))?;

            tx.commit()
                .await
                .map_err(|error| format!("commit generate thumbnail tx: {error}"))?;

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
                r#"
                SELECT s.URL AS SERIES_URL, l.ROOT AS LIBRARY_ROOT
                FROM SERIES s
                JOIN LIBRARY l ON l.ID = s.LIBRARY_ID
                WHERE s.ID = ?
                LIMIT 1
                "#,
            )
            .bind(&series_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to resolve series path for artwork refresh '{series_id}': {error}")
            })?;

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
                            r#"
                            INSERT OR REPLACE INTO THUMBNAIL_SERIES
                                (ID, URL, SELECTED, TYPE, SERIES_ID, MEDIA_TYPE, FILE_SIZE, LAST_MODIFIED_DATE)
                            VALUES (?, ?, 1, 'SIDECAR', ?, ?, ?, CURRENT_TIMESTAMP)
                            "#,
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
                r#"
                UPDATE THUMBNAIL_SERIES
                SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE SERIES_ID = ?
                "#,
            )
            .bind(&series_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                format!("failed to refresh THUMBNAIL_SERIES rows for '{series_id}': {error}")
            })?;

            sqlx::query(
                r#"
                UPDATE SERIES
                SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE ID = ?
                "#,
            )
            .bind(&series_id)
            .execute(&pool)
            .await
            .map_err(|error| format!("failed to refresh SERIES row while updating local artwork for '{series_id}': {error}"))?;

            Ok(())
        })
    })
}

fn render_generated_thumbnail_from_image_bytes(
    book_id: &str,
    thumbnail_bytes: &[u8],
    configured_max_edge: u32,
) -> Result<(Vec<u8>, String, i64, i64), String> {
    let image = image::load_from_memory(thumbnail_bytes).map_err(|error| {
        format!("failed to decode generated thumbnail source for '{book_id}': {error}")
    })?;
    let source_max_edge = image.width().max(image.height()).max(1);
    let effective_max_edge = configured_max_edge.min(source_max_edge);
    let resized = image.thumbnail(effective_max_edge, effective_max_edge);
    let width = i64::from(resized.width());
    let height = i64::from(resized.height());
    let mut output = Cursor::new(Vec::new());
    resized
        .write_to(&mut output, image::ImageFormat::Jpeg)
        .map_err(|error| {
            format!("failed to encode generated thumbnail for '{book_id}': {error}")
        })?;
    Ok((output.into_inner(), "image/jpeg".to_string(), width, height))
}

fn render_pdf_thumbnail(
    media: &BookMediaRecord,
    configured_max_edge: u32,
) -> Result<Option<(Vec<u8>, String, i64, i64)>, String> {
    let pdfium = load_pdfium()?;
    let document = pdfium
        .load_pdf_from_file(&media.file_path, None)
        .map_err(|error| {
            format!(
                "failed to load PDF for thumbnail generation '{}': {error}",
                media.file_path.display()
            )
        })?;
    let page = match document.pages().first() {
        Ok(page) => page,
        Err(_) => return Ok(None),
    };

    let rendered = page
        .render_with_config(
            &PdfRenderConfig::new()
                .set_target_width(i32::try_from(configured_max_edge).unwrap_or(i32::MAX))
                .set_maximum_height(i32::try_from(configured_max_edge).unwrap_or(i32::MAX)),
        )
        .map_err(|error| {
            format!(
                "failed to render PDF page for thumbnail generation '{}': {error}",
                media.file_path.display()
            )
        })?
        .as_image()
        .map_err(|error| {
            format!(
                "failed to convert PDF render to image '{}': {error}",
                media.file_path.display()
            )
        })?
        .into_rgb8();

    let width = i64::from(rendered.width());
    let height = i64::from(rendered.height());
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(rendered)
        .write_to(&mut output, image::ImageFormat::Jpeg)
        .map_err(|error| {
            format!(
                "failed to encode PDF thumbnail for '{}': {error}",
                media.file_path.display()
            )
        })?;

    Ok(Some((
        output.into_inner(),
        "image/jpeg".to_string(),
        width,
        height,
    )))
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
        r#"
        SELECT URL
        FROM SIDECAR
        WHERE PARENT_URL = ?
          AND LOWER(URL) LIKE '%.xml'
        ORDER BY LAST_MODIFIED_TIME DESC
        LIMIT 1
        "#
    } else {
        r#"
        SELECT URL
        FROM SIDECAR
        WHERE PARENT_URL = ?
          AND (
                LOWER(URL) LIKE '%.jpg'
             OR LOWER(URL) LIKE '%.jpeg'
             OR LOWER(URL) LIKE '%.png'
             OR LOWER(URL) LIKE '%.webp'
             OR LOWER(URL) LIKE '%.gif'
             OR LOWER(URL) LIKE '%.avif'
          )
        ORDER BY LAST_MODIFIED_TIME DESC
        LIMIT 1
        "#
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
