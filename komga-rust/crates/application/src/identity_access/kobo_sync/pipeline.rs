use async_trait::async_trait;
use serde_json::Value;

use crate::identity_access::{KoboMetadataRecord, PersistedReadProgressRecord, user_id};

use super::lifecycle::KoboSyncLifecycle;
use super::{
    KoboLibrarySyncRequest, KoboLibrarySyncResponse, KoboStoreSyncMergeResult,
    KoboSyncBookSnapshot, KoboSyncPage, KoboSyncPageRequest, KoboSyncReadProgressSnapshot,
    build_kobo_changed_entitlement_removed, build_kobo_changed_product_metadata,
    build_kobo_changed_reading_state, build_kobo_changed_tag, build_kobo_deleted_tag,
    build_kobo_new_entitlement, build_kobo_new_tag,
};

pub struct KoboLibrarySyncService<'a> {
    state: &'a dyn KoboSyncStatePort,
    store: &'a dyn KoboStoreSyncPort,
}

#[async_trait]
pub trait KoboSyncStatePort: Send + Sync {
    async fn load_sync_page(&self, request: KoboSyncPageRequest) -> Result<KoboSyncPage, String>;

    async fn load_kobo_metadata_record(
        &self,
        book_id: &str,
    ) -> Result<Option<KoboMetadataRecord>, String>;

    async fn load_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<Option<PersistedReadProgressRecord>, String>;

    async fn remove_sync_point(&self, sync_point_id: &str) -> Result<(), String>;
}

#[async_trait]
pub trait KoboStoreSyncPort: Send + Sync {
    async fn sync_store_library(
        &self,
        forwarded_headers: &[(String, String)],
        query: Option<&str>,
        raw_sync_token: &str,
    ) -> Result<KoboStoreSyncMergeResult, String>;
}

impl<'a> KoboLibrarySyncService<'a> {
    pub fn new(state: &'a dyn KoboSyncStatePort, store: &'a dyn KoboStoreSyncPort) -> Self {
        Self { state, store }
    }

    pub async fn sync_library(
        &self,
        request: KoboLibrarySyncRequest,
    ) -> Result<KoboLibrarySyncResponse, String> {
        let lifecycle = KoboSyncLifecycle::from_sync_token(request.sync_token.as_deref());
        let sync_page = self
            .state
            .load_sync_page(lifecycle.page_request(&request))
            .await?;
        let response_events = self.build_sync_events_page(&request, &sync_page).await?;

        let mut merged_events = response_events;
        let mut merged_should_continue = sync_page.should_continue;
        let mut merged_raw_kobo_sync_token = lifecycle.raw_kobo_sync_token();

        if !sync_page.should_continue
            && request.store_sync_enabled
            && let Some(raw_store_sync_token) =
                lifecycle.store_sync_token(&merged_raw_kobo_sync_token)
            && let Ok(store_response) = self
                .store
                .sync_store_library(
                    &request.forwarded_headers,
                    request.query.as_deref(),
                    raw_store_sync_token,
                )
                .await
        {
            merged_events.extend(store_response.events);
            merged_should_continue = store_response.should_continue;
            if let Some(raw_store_sync_token) = store_response.raw_sync_token
                && !raw_store_sync_token.trim().is_empty()
            {
                merged_raw_kobo_sync_token = Some(raw_store_sync_token);
            }
        }

        if let Some(sync_point_id) =
            lifecycle.sync_point_to_remove(&sync_page, merged_should_continue)
        {
            self.state.remove_sync_point(sync_point_id).await?;
        }

        let sync_token_payload = lifecycle.outgoing_sync_token_payload(
            &sync_page,
            merged_raw_kobo_sync_token,
            merged_should_continue,
        );

        Ok(KoboLibrarySyncResponse {
            events: merged_events,
            sync_token_payload,
            should_continue: merged_should_continue,
        })
    }

    async fn build_sync_events_page(
        &self,
        request: &KoboLibrarySyncRequest,
        page: &KoboSyncPage,
    ) -> Result<Vec<Value>, String> {
        let user_id_value = user_id(&request.user);
        let mut events = Vec::new();

        for book in &page.books_added {
            let metadata = self
                .state
                .load_kobo_metadata_record(&book.book_id)
                .await?
                .ok_or_else(|| "kobo metadata record not found".to_string())?;
            let progress = self
                .state
                .load_read_progress(&book.book_id, user_id_value)
                .await?
                .as_ref()
                .map(progress_snapshot);
            let snapshot = sync_book_snapshot_from_metadata(
                &book.book_id,
                &book.created,
                &book.file_last_modified,
                &metadata,
            );
            events.push(build_kobo_new_entitlement(
                &snapshot,
                progress.as_ref(),
                request.base_url.as_str(),
                request.auth_token.as_str(),
            ));
        }

        for book in &page.books_changed {
            let metadata = self
                .state
                .load_kobo_metadata_record(&book.book_id)
                .await?
                .ok_or_else(|| "kobo metadata record not found".to_string())?;
            let progress = self
                .state
                .load_read_progress(&book.book_id, user_id_value)
                .await?
                .as_ref()
                .map(progress_snapshot);
            let snapshot = sync_book_snapshot_from_metadata(
                &book.book_id,
                &book.created,
                &book.file_last_modified,
                &metadata,
            );
            events.push(build_kobo_new_entitlement(
                &snapshot,
                progress.as_ref(),
                request.base_url.as_str(),
                request.auth_token.as_str(),
            ));
            events.push(build_kobo_changed_product_metadata(
                &snapshot,
                request.base_url.as_str(),
                request.auth_token.as_str(),
            ));
            if let Some(progress) = progress.as_ref() {
                events.push(build_kobo_changed_reading_state(&snapshot, progress));
            }
        }

        for book in &page.books_removed {
            let snapshot =
                removed_book_snapshot(&book.book_id, &book.created, &book.file_last_modified);
            events.push(build_kobo_changed_entitlement_removed(
                &snapshot,
                request.base_url.as_str(),
                request.auth_token.as_str(),
            ));
        }

        for book in &page.books_read_progress_changed {
            if let Some(progress) = self
                .state
                .load_read_progress(&book.book_id, user_id_value)
                .await?
            {
                let metadata = self
                    .state
                    .load_kobo_metadata_record(&book.book_id)
                    .await?
                    .ok_or_else(|| "kobo metadata record not found".to_string())?;
                let snapshot = sync_book_snapshot_from_metadata(
                    &book.book_id,
                    &book.created,
                    &book.file_last_modified,
                    &metadata,
                );
                let progress = progress_snapshot(&progress);
                events.push(build_kobo_changed_reading_state(&snapshot, &progress));
            }
        }

        for readlist in &page.readlists_added {
            events.push(build_kobo_new_tag(readlist));
        }
        for readlist in &page.readlists_changed {
            events.push(build_kobo_changed_tag(readlist));
        }
        for readlist in &page.readlists_removed {
            events.push(build_kobo_deleted_tag(readlist));
        }

        Ok(events)
    }
}

fn sync_book_snapshot_from_metadata(
    book_id: &str,
    created: &str,
    file_last_modified: &str,
    metadata: &KoboMetadataRecord,
) -> KoboSyncBookSnapshot {
    KoboSyncBookSnapshot {
        id: book_id.to_string(),
        title: metadata.title.clone(),
        summary: metadata.summary.clone(),
        release_date: metadata.release_date.clone(),
        language: metadata.language.clone(),
        file_size: metadata.file_size,
        page_count: 1,
        created: metadata
            .created_date
            .clone()
            .unwrap_or_else(|| created.to_string()),
        last_modified: file_last_modified.to_string(),
        contributor_names: metadata.contributor_names.clone(),
        isbn: metadata.isbn.clone(),
        publisher_name: metadata.publisher_name.clone(),
        cover_image_id: metadata.cover_image_id.clone(),
        series_id: metadata.series_id.clone(),
        series_name: metadata.series_name.clone(),
        series_number: metadata.series_number.clone(),
        series_number_float: metadata.series_number_float,
        oneshot: metadata.oneshot,
    }
}

fn removed_book_snapshot(
    book_id: &str,
    created: &str,
    file_last_modified: &str,
) -> KoboSyncBookSnapshot {
    KoboSyncBookSnapshot {
        id: book_id.to_string(),
        title: book_id.to_string(),
        summary: String::new(),
        release_date: None,
        language: "en".to_string(),
        file_size: 0,
        page_count: 1,
        created: created.to_string(),
        last_modified: file_last_modified.to_string(),
        contributor_names: Vec::new(),
        isbn: None,
        publisher_name: None,
        cover_image_id: Some(book_id.to_string()),
        series_id: None,
        series_name: None,
        series_number: None,
        series_number_float: None,
        oneshot: true,
    }
}

fn progress_snapshot(record: &PersistedReadProgressRecord) -> KoboSyncReadProgressSnapshot {
    KoboSyncReadProgressSnapshot {
        page: record.page,
        completed: record.completed,
        created: record.created.clone(),
        last_modified: record.last_modified.clone(),
        locator: record.locator.clone(),
    }
}
