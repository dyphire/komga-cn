use std::collections::HashMap;

use async_trait::async_trait;

use super::{
    PersistedBookIdResolverPort, PersistedSeriesIdResolverPort, resolve_persisted_book_id,
    resolve_persisted_series_id,
};

#[derive(Default)]
struct RecordingBookIdResolver {
    existing: HashMap<String, bool>,
    sorted_ids: HashMap<usize, String>,
}

#[async_trait]
impl PersistedBookIdResolverPort for RecordingBookIdResolver {
    async fn persisted_book_resource_exists(&self, book_id: &str) -> Result<bool, String> {
        Ok(self.existing.get(book_id).copied().unwrap_or(false))
    }

    async fn load_book_id_by_sorted_position(
        &self,
        index: usize,
    ) -> Result<Option<String>, String> {
        Ok(self.sorted_ids.get(&index).cloned())
    }
}

#[derive(Default)]
struct RecordingSeriesIdResolver {
    existing: HashMap<String, bool>,
    sorted_ids: HashMap<usize, String>,
}

#[async_trait]
impl PersistedSeriesIdResolverPort for RecordingSeriesIdResolver {
    async fn persisted_series_resource_exists(&self, series_id: &str) -> Result<bool, String> {
        Ok(self.existing.get(series_id).copied().unwrap_or(false))
    }

    async fn load_series_id_by_sorted_position(
        &self,
        index: usize,
    ) -> Result<Option<String>, String> {
        Ok(self.sorted_ids.get(&index).cloned())
    }
}

#[tokio::test]
async fn persisted_book_id_resolution_preserves_existing_id_and_maps_position_fallback() {
    let resolver = RecordingBookIdResolver {
        existing: HashMap::from([("book-2".to_string(), true)]),
        sorted_ids: HashMap::from([(3, "real-book-3".to_string())]),
    };

    assert_eq!(
        resolve_persisted_book_id(&resolver, "book-2").await,
        "book-2"
    );
    assert_eq!(
        resolve_persisted_book_id(&resolver, "book-3").await,
        "real-book-3"
    );
    assert_eq!(
        resolve_persisted_book_id(&resolver, "book-0").await,
        "book-0"
    );
    assert_eq!(
        resolve_persisted_book_id(&resolver, "custom-book").await,
        "custom-book"
    );
}

#[tokio::test]
async fn persisted_series_id_resolution_preserves_existing_id_and_maps_position_fallback() {
    let resolver = RecordingSeriesIdResolver {
        existing: HashMap::from([("series-2".to_string(), true)]),
        sorted_ids: HashMap::from([(3, "real-series-3".to_string())]),
    };

    assert_eq!(
        resolve_persisted_series_id(&resolver, "series-2").await,
        "series-2"
    );
    assert_eq!(
        resolve_persisted_series_id(&resolver, "series-3").await,
        "real-series-3"
    );
    assert_eq!(
        resolve_persisted_series_id(&resolver, "series-0").await,
        "series-0"
    );
    assert_eq!(
        resolve_persisted_series_id(&resolver, "custom-series").await,
        "custom-series"
    );
}
