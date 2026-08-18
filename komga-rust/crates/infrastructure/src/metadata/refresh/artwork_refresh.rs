use anyhow::Context;
use komga_application::media_assets::{
    BookMediaRecord, BookPageRecord, book_media_is_epub, book_media_is_pdf,
    book_media_is_single_image, content_type_from_filename,
};
use komga_application::runtime_sse::RuntimeSseEventSink;
use komga_application::task_processing::ThumbnailRegenerationPolicy;
use komga_domain::media_assets::ThumbnailType;
use sqlx::{Row, SqlitePool};

use crate::filesystem::media_access::epub::load_epub_cover_bytes;
use crate::filesystem::media_access::page_content::{
    load_archive_page_rows, resolve_book_page_bytes,
};
use crate::metadata::thumbnails::{emit_thumbnail_book_event, emit_thumbnail_series_event};
use crate::parsing::parse_thumbnail_type;
use crate::{resolve_library_item_path, resolve_stored_path};

use super::artwork_support::{
    MarkSelectedPreference, book_thumbnail_housekeeping, import_book_local_artwork_thumbnail,
    import_series_local_artwork_thumbnail, is_suitable_cover_image, load_book_local_artwork_urls,
    load_series_local_artwork_urls, render_generated_thumbnail_from_image_bytes,
    render_pdf_thumbnail,
};

pub(crate) async fn refresh_book_local_artwork(
    pool: &SqlitePool,
    runtime_events: &dyn RuntimeSseEventSink,
    book_id: &str,
) -> anyhow::Result<()> {
    let book_id = book_id.to_string();

    let result: anyhow::Result<()> = 'result: {
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
            anyhow::anyhow!(error).context(format!(
                "failed to resolve book path for artwork refresh '{book_id}': "
            ))
        })?;

        if let Some(book_row) = &book_row {
            let series_id = sqlx::query("SELECT SERIES_ID FROM BOOK WHERE ID = ? LIMIT 1")
                .bind(&book_id)
                .fetch_optional(pool)
                .await
                .map_err(|error| {
                    anyhow::anyhow!(error).context(format!(
                        "failed to resolve book series for artwork refresh '{book_id}': "
                    ))
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
                anyhow::anyhow!(error).context(format!(
                    "failed to resolve import-local-artwork flag for '{book_id}': "
                ))
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
                emit_thumbnail_book_event(runtime_events, &book_id, &series_id, selected, true);
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
            anyhow::anyhow!(error).context(format!(
                "failed to refresh THUMBNAIL_BOOK rows for '{book_id}': "
            ))
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
            anyhow::anyhow!(error).context(format!(
                "failed to refresh BOOK row while updating local artwork for '{book_id}': "
            ))
        })?;

        Ok(())
    };
    result
}

pub async fn generate_book_thumbnail(
    pool: &SqlitePool,
    runtime_events: &dyn RuntimeSseEventSink,
    book_id: &str,
    policy: ThumbnailRegenerationPolicy,
) -> anyhow::Result<()> {
    let book_id = book_id.to_string();
    let result: anyhow::Result<()> = 'result: {
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
            anyhow::anyhow!(error).context(format!(
                "failed to resolve book media for thumbnail generation '{book_id}': "
            ))
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

        let configured_max_edge = policy.generated_thumbnail_max_edge;

        let epub_cover = if book_media_is_epub(&media) {
            load_epub_cover_bytes(&media).await?
        } else {
            None
        };

        let thumbnail = if book_media_is_pdf(&media) {
            let Some(rendered) = render_pdf_thumbnail(&media, configured_max_edge)? else {
                break 'result Ok(());
            };
            rendered
        } else if let Some(cover) = epub_cover {
            render_generated_thumbnail_from_image_bytes(
                &book_id,
                &cover.bytes,
                configured_max_edge,
            )?
        } else {
            let Some(page_row) = find_best_cover_page(pool, &book_id, &media).await? else {
                break 'result Ok(());
            };
            let Some(thumbnail_bytes) =
                resolve_book_page_bytes(&media, &page_row, page_row.number).await?
            else {
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
        .map_err(|error| {
            anyhow::anyhow!(error).context(format!(
                "failed to query selected thumbnail for '{book_id}': "
            ))
        })?
        .map(|row| parse_thumbnail_type(&row.get::<String, _>("TYPE")));
        let should_select = selected_thumbnail_type
            .is_none_or(|thumbnail_type| thumbnail_type == ThumbnailType::Generated);

        let mut tx = pool.begin().await.context("begin generate thumbnail tx")?;

        sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND TYPE = ?")
            .bind(&book_id)
            .bind(ThumbnailType::Generated.persisted_name())
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                anyhow::anyhow!(error).context(format!(
                    "failed to delete prior generated thumbnails for '{book_id}': "
                ))
            })?;

        if should_select {
            sqlx::query("UPDATE THUMBNAIL_BOOK SET SELECTED = 0 WHERE BOOK_ID = ?")
                .bind(&book_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    anyhow::anyhow!(error).context(format!(
                        "failed to clear selected thumbnails for '{book_id}': "
                    ))
                })?;
        }

        let thumbnail_id = format!("thumbnail-book-generated:{book_id}");
        sqlx::query(
            r#"
            INSERT INTO THUMBNAIL_BOOK
                (ID, SELECTED, THUMBNAIL, TYPE, BOOK_ID, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT, LAST_MODIFIED_DATE)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
            "#,
        )
        .bind(&thumbnail_id)
        .bind(should_select)
        .bind(&thumbnail.bytes)
        .bind(ThumbnailType::Generated.persisted_name())
        .bind(&book_id)
        .bind(&thumbnail.media_type)
        .bind(thumbnail.bytes.len() as i64)
        .bind(thumbnail.width)
        .bind(thumbnail.height)
        .execute(&mut *tx)
        .await
        .map_err(|error| { anyhow::anyhow!(error).context( format!("failed to insert generated thumbnail for '{book_id}'"))
        })?;

        if !should_select {
            book_thumbnail_housekeeping(&mut tx, &book_id, resolved_library_root.as_path()).await?;
        }

        tx.commit().await.context("commit generate thumbnail tx")?;
        emit_thumbnail_book_event(runtime_events, &book_id, &series_id, should_select, true);

        Ok(())
    };
    result
}

async fn find_best_cover_page(
    pool: &SqlitePool,
    book_id: &str,
    media: &BookMediaRecord,
) -> anyhow::Result<Option<BookPageRecord>> {
    let mut candidates = Vec::new();

    if book_media_is_single_image(media) {
        let file_size = tokio::fs::metadata(&media.file_path)
            .await
            .map_err(|error| {
                anyhow::anyhow!(error).context(format!(
                    "failed to inspect single-image media '{}' for cover selection: ",
                    media.file_path.display()
                ))
            })?
            .len() as i64;
        candidates.push(BookPageRecord {
            number: 1,
            file_name: media.file_name.clone(),
            media_type: content_type_from_filename(&media.file_name, &media.media_type),
            width: None,
            height: None,
            file_size,
        });
    } else {
        for page_number in 1..=3 {
            if let Some(row) =
                super::load_book_page_row_for_refresh(pool, book_id, page_number).await?
            {
                candidates.push(row);
            }
        }

        if candidates.is_empty()
            && let Some(rows) = load_archive_page_rows(media).await?
        {
            candidates.extend(rows.into_iter().take(3));
        }
    }

    for page in &candidates {
        if let Some(bytes) = resolve_book_page_bytes(media, page, page.number).await?
            && is_suitable_cover_image(&bytes)
        {
            return Ok(Some(page.clone()));
        }
    }

    Ok(candidates.into_iter().next())
}

pub(crate) async fn refresh_series_local_artwork(
    pool: &SqlitePool,
    runtime_events: &dyn RuntimeSseEventSink,
    series_id: &str,
) -> anyhow::Result<()> {
    let series_id = series_id.to_string();

    let result: anyhow::Result<()> = 'result: {
        let series_row = sqlx::query(
            r#"
            SELECT s.URL AS SERIES_URL,
                   l.ROOT AS LIBRARY_ROOT,
                   l.IMPORT_LOCAL_ARTWORK AS IMPORT_LOCAL_ARTWORK,
                   s.ONESHOT AS ONESHOT
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
            anyhow::anyhow!(error).context(format!(
                "failed to resolve series path for artwork refresh '{series_id}': "
            ))
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
                emit_thumbnail_series_event(runtime_events, &series_id, selected, true);
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
            anyhow::anyhow!(error).context(format!(
                "failed to refresh THUMBNAIL_SERIES rows for '{series_id}': "
            ))
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
            anyhow::anyhow!(error).context(format!(
                "failed to refresh SERIES row while updating local artwork for '{series_id}': "
            ))
        })?;

        Ok(())
    };
    result
}
#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use image::{ImageBuffer, Rgba};
    use komga_application::media_assets::BookMediaRecord;
    use komga_application::runtime_sse::RuntimeSseEventStore;
    use komga_application::task_processing::ThumbnailRegenerationPolicy;
    use sqlx::Row;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use super::{find_best_cover_page, generate_book_thumbnail};
    use crate::test_support::{BootstrappedBookFixture, MediaPageFixture};

    fn unique_temp_dir(case: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("komga-thumbnail-refresh-{case}-{nanos}"))
    }

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let image =
            ImageBuffer::<Rgba<u8>, Vec<u8>>::from_pixel(width, height, Rgba([80, 100, 120, 255]));
        let mut output = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut output, image::ImageFormat::Png)
            .expect("png fixture should encode");
        output.into_inner()
    }

    #[tokio::test]
    async fn generate_book_thumbnail_reads_public_page_one_from_persisted_row_zero() {
        let root = unique_temp_dir("persisted-row-zero");
        let series_dir = root.join("series");
        fs::create_dir_all(&series_dir).expect("series directory should be created");
        fs::write(series_dir.join("0001.png"), png_bytes(6, 4))
            .expect("page image fixture should be written");
        let fixture = BootstrappedBookFixture::open("thumbnail-row-zero").await;
        fixture.insert_library_series().await;
        fixture.insert_book("book-1").await;
        fixture
            .insert_media_with_page_count("book-1", Some("application/zip"), "READY", 1)
            .await;
        sqlx::query("UPDATE LIBRARY SET ROOT = ? WHERE ID = 'library-1'")
            .bind(root.to_string_lossy().as_ref())
            .execute(&fixture.pool)
            .await
            .expect("update library root");
        fixture
            .insert_media_page_with_dimensions(MediaPageFixture {
                book_id: "book-1",
                page_number: 0,
                file_name: "0001.png",
                media_type: "image/png",
                width: 6,
                height: 4,
                file_size: Some(128),
            })
            .await;
        let runtime_events = RuntimeSseEventStore::default();

        generate_book_thumbnail(
            &fixture.pool,
            &runtime_events,
            "book-1",
            ThumbnailRegenerationPolicy::default(),
        )
        .await
        .expect("generate thumbnail");

        let row = sqlx::query(
            "SELECT MEDIA_TYPE, WIDTH, HEIGHT FROM THUMBNAIL_BOOK WHERE BOOK_ID = 'book-1' AND TYPE = 'GENERATED'",
        )
        .fetch_optional(&fixture.pool)
        .await
        .expect("load generated thumbnail")
        .expect("generated thumbnail should exist");

        assert_eq!("image/jpeg", row.get::<String, _>("MEDIA_TYPE"));
        assert_eq!(6, row.get::<i64, _>("WIDTH"));
        assert_eq!(4, row.get::<i64, _>("HEIGHT"));

        fixture.close().await;
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn find_best_cover_page_skips_unreadable_candidates() {
        let root = unique_temp_dir("cover-candidate-decode-error");
        let series_dir = root.join("series");
        fs::create_dir_all(&series_dir).expect("series directory should be created");
        let archive_path = series_dir.join("book-1.zip");
        let file = fs::File::create(&archive_path).expect("cover candidate archive should open");
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        writer
            .start_file("1.png", options)
            .expect("invalid first page should be created");
        writer
            .write_all(b"not an image")
            .expect("invalid first page should be written");
        writer
            .start_file("2.png", options)
            .expect("valid second page should be created");
        writer
            .write_all(&png_bytes(6, 4))
            .expect("valid second page should be written");
        writer
            .finish()
            .expect("cover candidate archive should finish");

        let fixture = BootstrappedBookFixture::open("cover-candidate-decode-error").await;
        fixture.insert_library_series().await;
        fixture.insert_book("book-1").await;
        fixture
            .insert_media_with_page_count("book-1", Some("application/zip"), "READY", 2)
            .await;
        let media = BookMediaRecord {
            library_id: "library-1".to_string(),
            file_name: "book-1.zip".to_string(),
            file_path: archive_path,
            media_type: "application/zip".to_string(),
            page_count: 2,
        };

        let page = find_best_cover_page(&fixture.pool, "book-1", &media)
            .await
            .expect("cover candidates should be readable")
            .expect("a valid second page should be selected");

        assert_eq!(page.number, 2);
        assert_eq!(page.file_name, "2.png");

        fixture.close().await;
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn generate_book_thumbnail_propagates_single_image_metadata_errors() {
        let root = unique_temp_dir("single-image-metadata-error");
        fs::create_dir_all(&root).expect("thumbnail root should be created");
        fs::write(root.join("blocked"), b"not a directory")
            .expect("blocking file should be written");
        let fixture = BootstrappedBookFixture::open("thumbnail-single-image-metadata-error").await;
        fixture.insert_library_series().await;
        fixture.insert_book("book-1").await;
        fixture.insert_media("book-1", Some("image/png")).await;
        sqlx::query("UPDATE LIBRARY SET ROOT = ? WHERE ID = 'library-1'")
            .bind(root.to_string_lossy().as_ref())
            .execute(&fixture.pool)
            .await
            .expect("update library root");
        sqlx::query("UPDATE BOOK SET NAME = ?, URL = ? WHERE ID = 'book-1'")
            .bind("book.png")
            .bind("blocked/book.png")
            .execute(&fixture.pool)
            .await
            .expect("update book path");
        let runtime_events = RuntimeSseEventStore::default();

        let error = generate_book_thumbnail(
            &fixture.pool,
            &runtime_events,
            "book-1",
            ThumbnailRegenerationPolicy::default(),
        )
        .await
        .expect_err("single-image metadata errors should fail thumbnail generation");

        assert!(
            error
                .to_string()
                .contains("failed to inspect single-image media"),
            "unexpected thumbnail generation error: {error}"
        );
        let generated_count = sqlx::query(
            "SELECT COUNT(*) AS COUNT FROM THUMBNAIL_BOOK WHERE BOOK_ID = 'book-1' AND TYPE = 'GENERATED'",
        )
        .fetch_one(&fixture.pool)
        .await
        .expect("generated thumbnail count should be queryable")
        .get::<i64, _>("COUNT");
        assert_eq!(generated_count, 0);

        fixture.close().await;
        let _ = fs::remove_dir_all(root);
    }
}
