use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

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
    delete_collection_thumbnail, insert_collection_thumbnail, load_persisted_collection_thumbnails,
    persisted_collection_exists, select_collection_thumbnail,
};
pub use readlists::{
    delete_readlist_thumbnail, insert_readlist_thumbnail, load_persisted_readlist_name,
    load_persisted_readlist_thumbnails, persisted_readlist_exists, select_readlist_thumbnail,
};
pub use series::{
    delete_series_thumbnail, insert_series_thumbnail, load_persisted_series_thumbnails,
    load_selected_series_thumbnail, load_series_thumbnail_by_id, select_series_thumbnail,
};

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
