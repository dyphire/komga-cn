use komga_application::media_assets::{
    BookMediaRecord, BookPageRecord, book_media_is_epub, book_media_is_pdf,
    book_media_is_single_image, content_type_from_filename,
};
use sqlx::{Row, SqlitePool};

use crate::filesystem::media_access::epub::load_epub_cover_bytes;
use crate::filesystem::media_access::page_content::{
    load_archive_page_row, resolve_book_page_bytes,
};
use crate::metadata::thumbnails::{emit_thumbnail_book_event, emit_thumbnail_series_event};
use crate::{resolve_library_item_path, resolve_stored_path};

use super::artwork_support::{
    MarkSelectedPreference, book_thumbnail_housekeeping, import_book_local_artwork_thumbnail,
    import_series_local_artwork_thumbnail, load_book_local_artwork_urls,
    load_series_local_artwork_urls, render_generated_thumbnail_from_image_bytes,
    render_pdf_thumbnail,
};
use super::thumbnail_max_edge_from_setting;

pub async fn refresh_book_local_artwork(pool: &SqlitePool, book_id: &str) -> Result<(), String> {
    let book_id = book_id.to_string();

    let result: Result<(), String> = 'result: {
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
        .fetch_optional(pool)
        .await
        .map_err(|error| {
            format!("failed to resolve book path for artwork refresh '{book_id}': {error}")
        })?;

        if let Some(book_row) = &book_row {
            let series_id = sqlx::query("SELECT SERIES_ID FROM BOOK WHERE ID = ? LIMIT 1")
                .bind(&book_id)
                .fetch_optional(pool)
                .await
                .map_err(|error| {
                    format!(
                        "failed to resolve book series for artwork refresh '{book_id}': {error}"
                    )
                })?
                .map(|row| row.get::<String, _>("SERIES_ID"))
                .unwrap_or_default();
            let import_local_artwork = sqlx::query(
                r#"
                SELECT l.IMPORT_LOCAL_ARTWORK AS IMPORT_LOCAL_ARTWORK
                FROM BOOK b
                JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
                WHERE b.ID = ?
                LIMIT 1
                "#,
            )
            .bind(&book_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| {
                format!("failed to resolve import-local-artwork flag for '{book_id}': {error}")
            })?
            .map(|row| row.get::<bool, _>("IMPORT_LOCAL_ARTWORK"))
            .unwrap_or(false);
            if !import_local_artwork {
                break 'result Ok(());
            }

            let book_url = book_row.get::<String, _>("BOOK_URL");
            let library_root =
                resolve_stored_path(book_row.get::<String, _>("LIBRARY_ROOT").as_str());
            let artwork_urls = load_book_local_artwork_urls(&library_root, &book_url).await?;

            for (index, artwork_url) in artwork_urls.into_iter().enumerate() {
                let selected = if index == 0 {
                    MarkSelectedPreference::IfNoneOrGenerated
                } else {
                    MarkSelectedPreference::No
                };
                let selected = import_book_local_artwork_thumbnail(
                    pool,
                    &book_id,
                    &library_root,
                    &artwork_url,
                    selected,
                )
                .await?;
                emit_thumbnail_book_event(&book_id, &series_id, selected, true);
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
        .execute(pool)
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
        .execute(pool)
        .await
        .map_err(|error| {
            format!(
                "failed to refresh BOOK row while updating local artwork for '{book_id}': {error}"
            )
        })?;

        Ok(())
    };
    result
}

pub async fn generate_book_thumbnail(pool: &SqlitePool, book_id: &str) -> Result<(), String> {
    let book_id = book_id.to_string();
    let result: Result<(), String> = 'result: {
        let media_row = sqlx::query(
            r#"
            SELECT b.LIBRARY_ID AS LIBRARY_ID,
                   b.SERIES_ID AS SERIES_ID,
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
        .fetch_optional(pool)
        .await
        .map_err(|error| {
            format!("failed to resolve book media for thumbnail generation '{book_id}': {error}")
        })?;

        let Some(media_row) = media_row else {
            break 'result Ok(());
        };

        let library_root = media_row.get::<String, _>("LIBRARY_ROOT");
        let series_id = media_row.get::<String, _>("SERIES_ID");
        let resolved_library_root = resolve_stored_path(&library_root);
        let media = BookMediaRecord {
            library_id: media_row.get::<String, _>("LIBRARY_ID"),
            media_type: media_row.get::<String, _>("MEDIA_TYPE"),
            file_path: resolve_library_item_path(
                &library_root,
                media_row.get::<String, _>("BOOK_URL").as_str(),
            ),
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
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("failed to load thumbnail size setting: {error}"))?
        .and_then(|row| row.get::<Option<String>, _>("VALUE"));
        let configured_max_edge =
            thumbnail_max_edge_from_setting(thumbnail_size_setting.as_deref());

        let epub_cover = if book_media_is_epub(&media) {
            load_epub_cover_bytes(&media).await
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
        .fetch_optional(pool)
        .await
        .map_err(|error| {
            format!("failed to resolve page row for thumbnail generation '{book_id}': {error}")
        })?;

        let (thumbnail_bytes, thumbnail_media_type, width, height) = if book_media_is_pdf(&media) {
            let Some(rendered) = render_pdf_thumbnail(&media, configured_max_edge)? else {
                break 'result Ok(());
            };
            rendered
        } else if let Some((bytes, _media_type)) = epub_cover {
            render_generated_thumbnail_from_image_bytes(&book_id, &bytes, configured_max_edge)?
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
                    media_type: content_type_from_filename(&media.file_name, &media.media_type),
                    width: None,
                    height: None,
                    file_size: tokio::fs::metadata(&media.file_path)
                        .await
                        .ok()
                        .map(|metadata| metadata.len() as i64)
                        .unwrap_or(0),
                })
            } else {
                load_archive_page_row(&media, 1).await
            };

            let Some(page_row) = page_row else {
                break 'result Ok(());
            };
            let Some(thumbnail_bytes) = resolve_book_page_bytes(&media, &page_row, 1).await else {
                break 'result Ok(());
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
                break 'result Ok(());
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
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("failed to query selected thumbnail for '{book_id}': {error}"))?
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
        .map_err(|error| {
            format!("failed to insert generated thumbnail for '{book_id}': {error}")
        })?;

        if !should_select {
            book_thumbnail_housekeeping(&mut tx, &book_id, resolved_library_root.as_path()).await?;
        }

        tx.commit()
            .await
            .map_err(|error| format!("commit generate thumbnail tx: {error}"))?;
        emit_thumbnail_book_event(&book_id, &series_id, should_select, true);

        Ok(())
    };
    result
}

pub async fn refresh_series_local_artwork(
    pool: &SqlitePool,
    series_id: &str,
) -> Result<(), String> {
    let series_id = series_id.to_string();

    let result: Result<(), String> = 'result: {
        let series_row = sqlx::query(
            r#"
            SELECT s.URL AS SERIES_URL,
                   l.ROOT AS LIBRARY_ROOT,
                   l.IMPORT_LOCAL_ARTWORK AS IMPORT_LOCAL_ARTWORK,
                   COALESCE(s.ONESHOT, 0) AS ONESHOT
            FROM SERIES s
            JOIN LIBRARY l ON l.ID = s.LIBRARY_ID
            WHERE s.ID = ?
            LIMIT 1
            "#,
        )
        .bind(&series_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| {
            format!("failed to resolve series path for artwork refresh '{series_id}': {error}")
        })?;

        if let Some(series_row) = &series_row {
            let series_url = series_row.get::<String, _>("SERIES_URL");
            let import_local_artwork = series_row.get::<bool, _>("IMPORT_LOCAL_ARTWORK");
            if !import_local_artwork {
                break 'result Ok(());
            }

            let oneshot = series_row.get::<i64, _>("ONESHOT") != 0;
            if oneshot {
                break 'result Ok(());
            }

            let library_root =
                resolve_stored_path(series_row.get::<String, _>("LIBRARY_ROOT").as_str());
            let artwork_urls = load_series_local_artwork_urls(&library_root, &series_url).await?;

            for (index, artwork_url) in artwork_urls.into_iter().enumerate() {
                let selected = import_series_local_artwork_thumbnail(
                    pool,
                    &series_id,
                    &library_root,
                    &artwork_url,
                    if index == 0 {
                        MarkSelectedPreference::IfNoneOrGenerated
                    } else {
                        MarkSelectedPreference::No
                    },
                )
                .await?;
                emit_thumbnail_series_event(&series_id, selected, true);
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
        .execute(pool)
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
        .execute(pool)
        .await
        .map_err(|error| {
            format!(
                "failed to refresh SERIES row while updating local artwork for '{series_id}': {error}"
            )
        })?;

        Ok(())
    };
    result
}
