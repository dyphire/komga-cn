use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use crate::http::discovery::persisted::models::{
    PersistedAuthorEntry, PersistedAuthorsScope, PersistedBookBrowseEntry,
    PersistedBookPosterSummary, PersistedBookSummary, PersistedBookTagsScope,
    PersistedSeriesSummary,
};
use async_trait::async_trait;

#[async_trait]
pub trait PersistedDiscoveryService: Send + Sync {
    async fn load_persisted_author_names(
        &self,
        database_file: PathBuf,
        search: String,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_author_roles(
        &self,
        database_file: PathBuf,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_authors_by_scope(
        &self,
        database_file: PathBuf,
        scope: PersistedAuthorsScope,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<PersistedAuthorEntry>, String>;
    async fn load_book_poster_summaries(
        &self,
        database_file: PathBuf,
    ) -> Result<HashMap<String, Vec<PersistedBookPosterSummary>>, String>;
    async fn load_persisted_book_summaries(
        &self,
        database_file: PathBuf,
        user_id: Option<String>,
    ) -> Result<Vec<PersistedBookSummary>, String>;
    async fn load_persisted_book_summaries_by_ids(
        &self,
        database_file: PathBuf,
        user_id: Option<String>,
        ids: Vec<String>,
    ) -> Result<Vec<PersistedBookSummary>, String>;
    async fn load_persisted_book_count(&self, database_file: PathBuf) -> Result<usize, String>;
    async fn load_persisted_genres(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_tags(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_languages(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_publishers(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_age_ratings(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_sharing_labels(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_series_release_dates(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_series_tags(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_library_ids(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<String>, String>;
    async fn load_collection_memberships(
        &self,
        database_file: PathBuf,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, String>;
    async fn load_collection_ordering(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<HashMap<String, i64>, String>;
    async fn load_readlist_memberships(
        &self,
        database_file: PathBuf,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, String>;
    async fn load_persisted_ondeck_books(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<Vec<PersistedBookBrowseEntry>, String>;
    async fn load_persisted_duplicate_books(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedBookBrowseEntry>, String>;
    async fn load_persisted_book_tags(
        &self,
        database_file: PathBuf,
        scope: Option<PersistedBookTagsScope>,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, String>;
    async fn persisted_utc_date_minus_days(
        &self,
        database_file: PathBuf,
        days: i64,
    ) -> Result<Option<String>, String>;
    async fn load_series_read_progress_counts(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<HashMap<String, (i64, i64)>, String>;
    async fn load_series_read_dates(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<HashMap<String, String>, String>;
    async fn load_series_total_book_counts(
        &self,
        database_file: PathBuf,
    ) -> Result<HashMap<String, i64>, String>;
    async fn load_persisted_series_summaries(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedSeriesSummary>, String>;
    async fn load_persisted_series_summaries_by_ids(
        &self,
        database_file: PathBuf,
        ids: Vec<String>,
    ) -> Result<Vec<PersistedSeriesSummary>, String>;
    async fn load_persisted_series_count(&self, database_file: PathBuf) -> Result<usize, String>;
    async fn persisted_series_exist(&self, database_file: PathBuf) -> Result<bool, String>;
    async fn search_book_ids(
        &self,
        database_file: PathBuf,
        query: String,
        limit: usize,
    ) -> Result<Vec<String>, String>;
    async fn search_collection_ids(
        &self,
        database_file: PathBuf,
        query: String,
        limit: usize,
    ) -> Result<Vec<String>, String>;
    async fn search_readlist_scored_ids(
        &self,
        database_file: PathBuf,
        query: String,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, String>;
    async fn search_series_scored_ids(
        &self,
        database_file: PathBuf,
        query: String,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, String>;
}
