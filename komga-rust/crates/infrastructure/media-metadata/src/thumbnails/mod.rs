use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use komga_application::runtime_sse::{RuntimeSseEvent, RuntimeSseEventSink};

use komga_infrastructure_base::resolve_optional_library_item_path;

mod books;
mod collections;
mod readlists;
mod series;

static GENERATED_THUMBNAIL_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

pub use books::{
    delete_book_thumbnail, insert_book_thumbnail, load_book_thumbnail_by_id,
    load_persisted_book_thumbnails, load_selected_book_thumbnail, select_book_thumbnail,
};
pub use collections::{
    delete_collection_thumbnail, insert_collection_thumbnail, load_collection_thumbnail_by_id,
    load_persisted_collection_thumbnails, persisted_collection_exists, select_collection_thumbnail,
};
pub use readlists::{
    delete_readlist_thumbnail, insert_readlist_thumbnail, load_persisted_readlist_name,
    load_persisted_readlist_thumbnails, load_readlist_thumbnail_by_id, persisted_readlist_exists,
    select_readlist_thumbnail,
};
pub use series::{
    delete_series_thumbnail, insert_series_thumbnail, load_persisted_series_thumbnails,
    load_selected_series_thumbnail, load_series_thumbnail_by_id, select_series_thumbnail,
};

fn load_thumbnail_bytes_or_sidecar(
    thumbnail: Option<Vec<u8>>,
    url: Option<String>,
    library_root: Option<String>,
    context: &str,
) -> anyhow::Result<Option<Vec<u8>>> {
    if let Some(thumbnail) = thumbnail {
        return Ok(Some(thumbnail));
    }

    let Some(url) = url else {
        return Ok(None);
    };

    let path =
        resolve_optional_library_item_path(library_root.as_deref(), &url).ok_or_else(|| {
            anyhow::anyhow!(format!(
                "persisted thumbnail sidecar URL requires a library root for {context}: {url}"
            ))
        })?;
    let bytes = std::fs::read(&path).map_err(|error| {
        anyhow::anyhow!(error).context(format!(
            "read thumbnail sidecar {} for {context}: ",
            path.display()
        ))
    })?;
    Ok(Some(bytes))
}

fn generated_thumbnail_id(prefix: &str) -> String {
    // Upload endpoints can create multiple thumbnails within the same clock tick, so the
    // identifier must stay unique even when timestamps collide under fast test/runtime paths.
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = GENERATED_THUMBNAIL_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{timestamp:032x}{counter:016x}")
}

pub fn emit_thumbnail_book_event(
    runtime_events: &dyn RuntimeSseEventSink,
    book_id: &str,
    series_id: &str,
    selected: bool,
    created: bool,
) {
    let event = if created {
        RuntimeSseEvent::ThumbnailBookAdded {
            book_id: book_id.to_string(),
            series_id: series_id.to_string(),
            selected,
        }
    } else {
        RuntimeSseEvent::ThumbnailBookDeleted {
            book_id: book_id.to_string(),
            series_id: series_id.to_string(),
            selected,
        }
    };
    runtime_events.register(event);
}

pub fn emit_thumbnail_series_event(
    runtime_events: &dyn RuntimeSseEventSink,
    series_id: &str,
    selected: bool,
    created: bool,
) {
    let event = if created {
        RuntimeSseEvent::ThumbnailSeriesAdded {
            series_id: series_id.to_string(),
            selected,
        }
    } else {
        RuntimeSseEvent::ThumbnailSeriesDeleted {
            series_id: series_id.to_string(),
            selected,
        }
    };
    runtime_events.register(event);
}

pub fn emit_thumbnail_readlist_event(
    runtime_events: &dyn RuntimeSseEventSink,
    readlist_id: &str,
    selected: bool,
    created: bool,
) {
    let event = if created {
        RuntimeSseEvent::ThumbnailReadListAdded {
            readlist_id: readlist_id.to_string(),
            selected,
        }
    } else {
        RuntimeSseEvent::ThumbnailReadListDeleted {
            readlist_id: readlist_id.to_string(),
            selected,
        }
    };
    runtime_events.register(event);
}

pub fn emit_thumbnail_collection_event(
    runtime_events: &dyn RuntimeSseEventSink,
    collection_id: &str,
    selected: bool,
    created: bool,
) {
    let event = if created {
        RuntimeSseEvent::ThumbnailCollectionAdded {
            collection_id: collection_id.to_string(),
            selected,
        }
    } else {
        RuntimeSseEvent::ThumbnailCollectionDeleted {
            collection_id: collection_id.to_string(),
            selected,
        }
    };
    runtime_events.register(event);
}

#[cfg(test)]
mod tests {
    use komga_infrastructure_test_support::BootstrappedBookFixture;

    use super::{load_selected_book_thumbnail, load_selected_series_thumbnail};

    #[tokio::test]
    async fn selected_thumbnail_loaders_read_relative_sidecar_files() {
        let fixture = BootstrappedBookFixture::open("thumbnail-selected-sidecar").await;
        fixture.insert_library_series().await;
        fixture.insert_book("book-1").await;

        let library_root = fixture
            .db_path
            .parent()
            .expect("fixture database should have a parent")
            .join("thumbnail-sidecar-root");
        std::fs::create_dir_all(&library_root).expect("sidecar root should be created");
        let book_bytes = b"book sidecar bytes";
        let series_bytes = b"series sidecar bytes";
        std::fs::write(library_root.join("book-cover.png"), book_bytes)
            .expect("book sidecar should be written");
        std::fs::write(library_root.join("series-cover.png"), series_bytes)
            .expect("series sidecar should be written");

        sqlx::query("UPDATE LIBRARY SET ROOT = ? WHERE ID = ?")
            .bind(library_root.to_string_lossy().as_ref())
            .bind("library-1")
            .execute(&fixture.pool)
            .await
            .expect("library root should be updated");

        sqlx::query(
            r#"
            INSERT INTO THUMBNAIL_BOOK (
                ID, BOOK_ID, URL, TYPE, SELECTED, MEDIA_TYPE, FILE_SIZE
            )
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("thumbnail-book-sidecar")
        .bind("book-1")
        .bind("book-cover.png")
        .bind("SIDECAR")
        .bind(true)
        .bind("image/png")
        .bind(i64::try_from(book_bytes.len()).expect("book bytes length should fit i64"))
        .execute(&fixture.pool)
        .await
        .expect("selected book sidecar row should be inserted");

        sqlx::query(
            r#"
            INSERT INTO THUMBNAIL_SERIES (
                ID, SERIES_ID, URL, TYPE, SELECTED, MEDIA_TYPE, FILE_SIZE
            )
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("thumbnail-series-sidecar")
        .bind("series-1")
        .bind("series-cover.png")
        .bind("SIDECAR")
        .bind(true)
        .bind("image/png")
        .bind(i64::try_from(series_bytes.len()).expect("series bytes length should fit i64"))
        .execute(&fixture.pool)
        .await
        .expect("selected series sidecar row should be inserted");

        let book = load_selected_book_thumbnail(&fixture.pool, "book-1")
            .await
            .expect("selected book sidecar should load")
            .expect("selected book sidecar should exist");
        assert_eq!(book.thumbnail, book_bytes);

        let series = load_selected_series_thumbnail(&fixture.pool, "series-1")
            .await
            .expect("selected series sidecar should load")
            .expect("selected series sidecar should exist");
        assert_eq!(series.thumbnail, series_bytes);

        fixture.close().await;
    }
}
