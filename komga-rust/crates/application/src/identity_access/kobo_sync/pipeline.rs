use std::collections::HashMap;

use crate::identity_access::user_id;

use super::lifecycle::KoboSyncLifecycle;
use super::{
    KoboLibrarySyncRequest, KoboLibrarySyncResponse, KoboProxyHeader, KoboStoreSyncMergeResult,
    KoboSyncBookSnapshot, KoboSyncBookState, KoboSyncEvent, KoboSyncPage, KoboSyncPageRequest,
    KoboSyncPointBook,
};

pub struct KoboLibrarySyncService<'a> {
    state: &'a dyn KoboSyncStatePort,
    store: &'a dyn KoboStoreSyncPort,
}

#[async_trait::async_trait]
pub trait KoboSyncStatePort: Send + Sync {
    async fn load_sync_page(&self, request: KoboSyncPageRequest) -> Result<KoboSyncPage, String>;

    async fn load_sync_book_states(
        &self,
        books: &[KoboSyncPointBook],
        user_id: &str,
    ) -> Result<Vec<KoboSyncBookState>, String>;

    async fn remove_sync_point(&self, sync_point_id: &str) -> Result<(), String>;
}

#[async_trait::async_trait]
pub trait KoboStoreSyncPort: Send + Sync {
    async fn sync_store_library(
        &self,
        forwarded_headers: &[KoboProxyHeader],
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
            merged_events.extend(store_response.events.into_iter().map(KoboSyncEvent::Raw));
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
    ) -> Result<Vec<KoboSyncEvent>, String> {
        let user_id_value = user_id(&request.user);
        let book_states = self.load_sync_book_state_map(page, user_id_value).await?;
        let mut events = Vec::new();

        for book in &page.books_added {
            let state = required_book_state(&book_states, &book.book_id)?;
            let snapshot = required_book_snapshot(state)?.clone();
            events.push(KoboSyncEvent::NewEntitlement {
                book: snapshot,
                progress: state.progress.clone(),
            });
        }

        for book in &page.books_changed {
            let state = required_book_state(&book_states, &book.book_id)?;
            let snapshot = required_book_snapshot(state)?.clone();
            events.push(KoboSyncEvent::NewEntitlement {
                book: snapshot.clone(),
                progress: state.progress.clone(),
            });
            events.push(KoboSyncEvent::ChangedProductMetadata {
                book: snapshot.clone(),
            });
            if let Some(progress) = state.progress.clone() {
                events.push(KoboSyncEvent::ChangedReadingState {
                    book: snapshot,
                    progress,
                });
            }
        }

        for book in &page.books_removed {
            let snapshot =
                removed_book_snapshot(&book.book_id, &book.created, &book.file_last_modified);
            events.push(KoboSyncEvent::ChangedEntitlementRemoved { book: snapshot });
        }

        for book in &page.books_read_progress_changed {
            let state = required_book_state(&book_states, &book.book_id)?;
            if let Some(progress) = state.progress.clone() {
                let snapshot = required_book_snapshot(state)?.clone();
                events.push(KoboSyncEvent::ChangedReadingState {
                    book: snapshot,
                    progress,
                });
            }
        }

        for readlist in &page.readlists_added {
            events.push(KoboSyncEvent::NewTag {
                readlist: readlist.clone(),
            });
        }
        for readlist in &page.readlists_changed {
            events.push(KoboSyncEvent::ChangedTag {
                readlist: readlist.clone(),
            });
        }
        for readlist in &page.readlists_removed {
            events.push(KoboSyncEvent::DeletedTag {
                readlist: readlist.clone(),
            });
        }

        Ok(events)
    }

    async fn load_sync_book_state_map(
        &self,
        page: &KoboSyncPage,
        user_id: &str,
    ) -> Result<HashMap<String, KoboSyncBookState>, String> {
        let books = page
            .books_added
            .iter()
            .chain(&page.books_changed)
            .chain(&page.books_read_progress_changed)
            .cloned()
            .collect::<Vec<_>>();
        let states = self.state.load_sync_book_states(&books, user_id).await?;
        Ok(states
            .into_iter()
            .map(|state| (state.book_id.clone(), state))
            .collect())
    }
}

fn required_book_state<'a>(
    states: &'a HashMap<String, KoboSyncBookState>,
    book_id: &str,
) -> Result<&'a KoboSyncBookState, String> {
    states
        .get(book_id)
        .ok_or_else(|| format!("kobo sync book state not found for {book_id}"))
}

fn required_book_snapshot(state: &KoboSyncBookState) -> Result<&KoboSyncBookSnapshot, String> {
    state
        .book
        .as_ref()
        .ok_or_else(|| format!("kobo sync book snapshot not found for {}", state.book_id))
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
