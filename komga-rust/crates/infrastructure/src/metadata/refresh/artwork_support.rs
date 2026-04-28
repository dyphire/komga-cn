use std::io::Cursor;
use std::path::Path;

use komga_application::media_assets::BookMediaRecord;
use pdfium_render::prelude::*;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use tokio::fs;

use crate::load_pdfium;
use crate::resolve_rooted_path;

pub(super) async fn book_thumbnail_housekeeping(
    tx: &mut Transaction<'_, Sqlite>,
    book_id: &str,
    library_root: &Path,
) -> Result<(), String> {
    let rows = sqlx::query(
        r#"
        SELECT ID, URL, THUMBNAIL, SELECTED
        FROM THUMBNAIL_BOOK
        WHERE BOOK_ID = ?
        ORDER BY LAST_MODIFIED_DATE DESC, ID ASC
        "#,
    )
    .bind(book_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| format!("failed to load thumbnails for '{book_id}' housekeeping: {error}"))?;

    let mut retained_ids = Vec::new();
    let mut selected_ids = Vec::new();
    for row in rows {
        let thumbnail_id = row.get::<String, _>("ID");
        let thumbnail_url = row.get::<Option<String>, _>("URL");
        let thumbnail_blob = row.get::<Option<Vec<u8>>, _>("THUMBNAIL");
        let selected = row.get::<bool, _>("SELECTED");

        let exists = thumbnail_blob
            .as_ref()
            .is_some_and(|thumbnail| !thumbnail.is_empty())
            || thumbnail_url.as_deref().is_some_and(|url| {
                let resolved = resolve_rooted_path(library_root, url);
                resolved.exists()
            });

        if !exists {
            sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE ID = ?")
                .bind(&thumbnail_id)
                .execute(&mut **tx)
                .await
                .map_err(|error| {
                    format!(
                        "failed to delete invalid thumbnail '{thumbnail_id}' for '{book_id}': {error}"
                    )
                })?;
            continue;
        }

        if selected {
            selected_ids.push(thumbnail_id.clone());
        }
        retained_ids.push(thumbnail_id);
    }

    let Some(target_selected_id) = (if selected_ids.len() > 1 {
        selected_ids.into_iter().next()
    } else if selected_ids.is_empty() {
        retained_ids.into_iter().next()
    } else {
        None
    }) else {
        return Ok(());
    };

    sqlx::query(
        "UPDATE THUMBNAIL_BOOK SET SELECTED = CASE WHEN ID = ? THEN 1 ELSE 0 END WHERE BOOK_ID = ?",
    )
    .bind(target_selected_id)
    .bind(book_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| format!("failed to normalize selected thumbnail for '{book_id}': {error}"))?;

    Ok(())
}

pub(super) fn render_generated_thumbnail_from_image_bytes(
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

pub(super) fn render_pdf_thumbnail(
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MarkSelectedPreference {
    IfNoneOrGenerated,
    No,
}

pub(super) async fn load_book_local_artwork_urls(
    library_root: &Path,
    book_url: &str,
) -> Result<Vec<String>, String> {
    let book_path = resolve_rooted_path(library_root, book_url);
    let Some(book_dir) = book_path.parent() else {
        return Ok(Vec::new());
    };
    let Some(book_base_name) = book_path.file_stem().and_then(|value| value.to_str()) else {
        return Ok(Vec::new());
    };

    let mut artwork_urls = Vec::new();
    let mut entries = fs::read_dir(book_dir).await.map_err(|error| {
        format!(
            "failed to scan local artwork directory '{}' for '{}': {error}",
            book_dir.display(),
            book_url,
        )
    })?;
    while let Some(entry) = entries.next_entry().await.map_err(|error| {
        format!(
            "failed to read local artwork entry in '{}' for '{}': {error}",
            book_dir.display(),
            book_url,
        )
    })? {
        let file_type = entry.file_type().await.map_err(|error| {
            format!(
                "failed to inspect local artwork entry in '{}' for '{}': {error}",
                book_dir.display(),
                book_url,
            )
        })?;
        let path = entry.path();
        if !file_type.is_file() || !supported_book_local_artwork_path(path.as_path()) {
            continue;
        }
        let Some(candidate_stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if !book_local_artwork_name_matches(candidate_stem, book_base_name) {
            continue;
        }

        let relative_url = path
            .strip_prefix(library_root)
            .map_err(|error| {
                format!(
                    "failed to relativize local artwork '{}' against library root '{}': {error}",
                    path.display(),
                    library_root.display(),
                )
            })?
            .to_string_lossy()
            .replace('\\', "/");
        artwork_urls.push(relative_url);
    }

    Ok(artwork_urls)
}

pub(super) async fn load_series_local_artwork_urls(
    library_root: &Path,
    series_url: &str,
) -> Result<Vec<String>, String> {
    let series_path = resolve_rooted_path(library_root, series_url);
    let mut artwork_urls = Vec::new();
    let mut entries = fs::read_dir(&series_path).await.map_err(|error| {
        format!(
            "failed to scan series local artwork directory '{}' for '{}': {error}",
            series_path.display(),
            series_url,
        )
    })?;
    while let Some(entry) = entries.next_entry().await.map_err(|error| {
        format!(
            "failed to read series local artwork entry in '{}' for '{}': {error}",
            series_path.display(),
            series_url,
        )
    })? {
        let file_type = entry.file_type().await.map_err(|error| {
            format!(
                "failed to inspect series local artwork entry in '{}' for '{}': {error}",
                series_path.display(),
                series_url,
            )
        })?;
        let path = entry.path();
        if !file_type.is_file() || !supported_series_local_artwork_path(path.as_path()) {
            continue;
        }
        let Some(candidate_stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if !series_local_artwork_name_matches(candidate_stem) {
            continue;
        }

        let relative_url = path
            .strip_prefix(library_root)
            .map_err(|error| {
                format!(
                    "failed to relativize series local artwork '{}' against library root '{}': {error}",
                    path.display(),
                    library_root.display(),
                )
            })?
            .to_string_lossy()
            .replace('\\', "/");
        artwork_urls.push(relative_url);
    }

    Ok(artwork_urls)
}

fn supported_series_local_artwork_path(path: &Path) -> bool {
    supported_book_local_artwork_path(path)
}

fn series_local_artwork_name_matches(candidate_stem: &str) -> bool {
    matches!(
        candidate_stem.to_ascii_lowercase().as_str(),
        "cover" | "default" | "folder" | "poster" | "series"
    )
}

fn supported_book_local_artwork_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref(),
        Some("png") | Some("jpeg") | Some("jpg") | Some("tbn") | Some("webp") | Some("gif")
    )
}

fn book_local_artwork_name_matches(candidate_stem: &str, book_base_name: &str) -> bool {
    let candidate_stem = candidate_stem.to_ascii_lowercase();
    let book_base_name = book_base_name.to_ascii_lowercase();
    if candidate_stem == book_base_name {
        return true;
    }

    candidate_stem
        .strip_prefix(&format!("{book_base_name}-"))
        .is_some_and(|suffix| !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit()))
}

pub(super) async fn import_book_local_artwork_thumbnail(
    pool: &SqlitePool,
    book_id: &str,
    library_root: &Path,
    artwork_url: &str,
    selected_preference: MarkSelectedPreference,
) -> Result<bool, String> {
    let artwork_path = library_root.join(artwork_url);
    let metadata = tokio::fs::metadata(&artwork_path).await.map_err(|error| {
        format!(
            "failed to read local artwork '{}' for book '{}': {error}",
            artwork_path.display(),
            book_id,
        )
    })?;
    let thumbnail_id = format!("thumbnail-book-sidecar:{book_id}:{artwork_url}");
    let selected = should_select_book_local_artwork(pool, book_id, selected_preference).await?;

    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND TYPE = 'SIDECAR' AND URL = ?")
        .bind(book_id)
        .bind(artwork_url)
        .execute(pool)
        .await
        .map_err(|error| {
            format!(
                "failed to remove duplicated sidecar thumbnail '{}' for '{}': {error}",
                artwork_url, book_id,
            )
        })?;

    sqlx::query(
        r#"
        INSERT INTO THUMBNAIL_BOOK
            (ID, URL, SELECTED, TYPE, BOOK_ID, MEDIA_TYPE, FILE_SIZE, LAST_MODIFIED_DATE)
        VALUES (?, ?, ?, 'SIDECAR', ?, ?, ?, CURRENT_TIMESTAMP)
        "#,
    )
    .bind(&thumbnail_id)
    .bind(artwork_url)
    .bind(selected)
    .bind(book_id)
    .bind(media_type_from_sidecar_path(artwork_path.as_path()))
    .bind(metadata.len() as i64)
    .execute(pool)
    .await
    .map_err(|error| {
        format!(
            "failed to insert local artwork '{}' for book '{}': {error}",
            artwork_url, book_id,
        )
    })?;

    if selected {
        sqlx::query(
            "UPDATE THUMBNAIL_BOOK SET SELECTED = CASE WHEN ID = ? THEN 1 ELSE 0 END WHERE BOOK_ID = ?",
        )
        .bind(&thumbnail_id)
        .bind(book_id)
        .execute(pool)
        .await
        .map_err(|error| {
            format!(
                "failed to mark local artwork '{}' as selected for '{}': {error}",
                artwork_url, book_id,
            )
        })?;
    }

    Ok(selected)
}

pub(super) async fn import_series_local_artwork_thumbnail(
    pool: &SqlitePool,
    series_id: &str,
    library_root: &Path,
    artwork_url: &str,
    selected_preference: MarkSelectedPreference,
) -> Result<bool, String> {
    let artwork_path = library_root.join(artwork_url);
    let metadata = tokio::fs::metadata(&artwork_path).await.map_err(|error| {
        format!(
            "failed to read series local artwork '{}' for '{}': {error}",
            artwork_path.display(),
            series_id,
        )
    })?;
    let thumbnail_id = format!("thumbnail-series-sidecar:{series_id}:{artwork_url}");
    let selected = should_select_series_local_artwork(pool, series_id, selected_preference).await?;

    sqlx::query(
        "DELETE FROM THUMBNAIL_SERIES WHERE SERIES_ID = ? AND TYPE = 'SIDECAR' AND URL = ?",
    )
    .bind(series_id)
    .bind(artwork_url)
    .execute(pool)
    .await
    .map_err(|error| {
        format!(
            "failed to remove duplicated series sidecar thumbnail '{}' for '{}': {error}",
            artwork_url, series_id,
        )
    })?;

    sqlx::query(
        r#"
        INSERT INTO THUMBNAIL_SERIES
            (ID, URL, SELECTED, TYPE, SERIES_ID, MEDIA_TYPE, FILE_SIZE, LAST_MODIFIED_DATE)
        VALUES (?, ?, ?, 'SIDECAR', ?, ?, ?, CURRENT_TIMESTAMP)
        "#,
    )
    .bind(&thumbnail_id)
    .bind(artwork_url)
    .bind(selected)
    .bind(series_id)
    .bind(media_type_from_sidecar_path(artwork_path.as_path()))
    .bind(metadata.len() as i64)
    .execute(pool)
    .await
    .map_err(|error| {
        format!(
            "failed to insert series local artwork '{}' for '{}': {error}",
            artwork_url, series_id,
        )
    })?;

    if selected {
        sqlx::query(
            "UPDATE THUMBNAIL_SERIES SET SELECTED = CASE WHEN ID = ? THEN 1 ELSE 0 END WHERE SERIES_ID = ?",
        )
        .bind(&thumbnail_id)
        .bind(series_id)
        .execute(pool)
        .await
        .map_err(|error| {
            format!(
                "failed to mark series local artwork '{}' as selected for '{}': {error}",
                artwork_url, series_id,
            )
        })?;
    }

    Ok(selected)
}

async fn should_select_book_local_artwork(
    pool: &SqlitePool,
    book_id: &str,
    selected_preference: MarkSelectedPreference,
) -> Result<bool, String> {
    if selected_preference == MarkSelectedPreference::No {
        return Ok(false);
    }

    let selected_row = sqlx::query(
        "SELECT TYPE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND SELECTED = 1 ORDER BY ID ASC LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to load selected thumbnail for '{}': {error}", book_id))?;

    Ok(match selected_row {
        None => true,
        Some(row) => row.get::<String, _>("TYPE") == "GENERATED",
    })
}

async fn should_select_series_local_artwork(
    pool: &SqlitePool,
    series_id: &str,
    selected_preference: MarkSelectedPreference,
) -> Result<bool, String> {
    if selected_preference == MarkSelectedPreference::No {
        return Ok(false);
    }

    let selected_row = sqlx::query(
        "SELECT TYPE FROM THUMBNAIL_SERIES WHERE SERIES_ID = ? AND SELECTED = 1 ORDER BY ID ASC LIMIT 1",
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        format!(
            "failed to load selected series thumbnail for '{}': {error}",
            series_id
        )
    })?;

    Ok(match selected_row {
        None => true,
        Some(row) => row.get::<String, _>("TYPE") == "GENERATED",
    })
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
        Some("tbn") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("avif") => "image/avif",
        _ => "application/octet-stream",
    }
}
