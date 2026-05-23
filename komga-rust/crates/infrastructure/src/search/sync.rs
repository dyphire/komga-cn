use std::path::Path;

use sqlx::SqlitePool;

use super::documents;
use super::index_lifecycle::{
    SearchEntityType, SearchError, SearchEvent, SearchIndexLifecycle, prepare_for_rebuild,
};
use super::runtime_tasks;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SearchEventAttempt {
    Applied,
    RebuildRequired,
}

async fn recover_search_index(pool: &SqlitePool, index_dir: &Path) -> Result<(), String> {
    prepare_for_rebuild(index_dir)
        .map_err(|error| format!("failed to prepare search index rebuild: {error}"))?;

    runtime_tasks::rebuild_index_from_database(pool, index_dir).await
}

async fn try_apply_search_event(
    index_dir: &Path,
    event: SearchEvent,
) -> Result<SearchEventAttempt, String> {
    match SearchIndexLifecycle::bootstrap(index_dir) {
        Ok(index) => {
            index
                .apply_event(event)
                .map_err(|error| format!("failed to apply search event: {error}"))?;
            index
                .shutdown()
                .map_err(|error| format!("failed to finalize search event writer: {error}"))?;
            Ok(SearchEventAttempt::Applied)
        }
        Err(SearchError::CorruptedIndexRequiresExplicitRebuild(_, _)) => {
            Ok(SearchEventAttempt::RebuildRequired)
        }
        Err(error) => Err(format!("failed to bootstrap search index: {error}")),
    }
}

async fn apply_search_event(
    pool: &SqlitePool,
    index_dir: &Path,
    event: SearchEvent,
) -> Result<(), String> {
    match try_apply_search_event(index_dir, event.clone()).await? {
        SearchEventAttempt::Applied => Ok(()),
        SearchEventAttempt::RebuildRequired => {
            recover_search_index(pool, index_dir).await?;

            match try_apply_search_event(index_dir, event).await? {
                SearchEventAttempt::Applied => Ok(()),
                SearchEventAttempt::RebuildRequired => Err(format!(
                    "failed to bootstrap search index after rebuild: corruption persisted at '{}'",
                    index_dir.display()
                )),
            }
        }
    }
}

pub async fn sync_entity_upsert_from_database(
    pool: &SqlitePool,
    index_dir: &Path,
    entity_type: SearchEntityType,
    entity_id: &str,
) -> Result<bool, String> {
    let document = match entity_type {
        SearchEntityType::Book => {
            documents::load_book_search_document(pool.clone(), entity_id).await?
        }
        SearchEntityType::Series => {
            documents::load_series_search_document(pool.clone(), entity_id).await?
        }
        SearchEntityType::Collection => {
            documents::load_collection_search_document(pool.clone(), entity_id).await?
        }
        SearchEntityType::ReadList => {
            documents::load_readlist_search_document(pool.clone(), entity_id).await?
        }
    };

    let Some(document) = document else {
        return Ok(false);
    };

    apply_search_event(pool, index_dir, SearchEvent::Upsert(document)).await?;
    Ok(true)
}

pub async fn sync_series_and_oneshot_books_after_metadata_update(
    pool: &SqlitePool,
    index_dir: &Path,
    series_id: &str,
) -> Result<(), String> {
    let series_document = documents::load_series_search_document(pool.clone(), series_id).await?;
    let oneshot_documents =
        documents::load_oneshot_book_search_documents(pool.clone(), series_id).await?;

    if let Some(document) = series_document {
        apply_search_event(pool, index_dir, SearchEvent::Upsert(document)).await?;
    }

    for document in oneshot_documents {
        apply_search_event(pool, index_dir, SearchEvent::Upsert(document)).await?;
    }

    Ok(())
}

pub async fn sync_entity_delete_from_index(
    pool: &SqlitePool,
    index_dir: &Path,
    entity_type: SearchEntityType,
    entity_id: &str,
) -> Result<(), String> {
    apply_search_event(
        pool,
        index_dir,
        SearchEvent::Delete {
            entity_type,
            id: entity_id.to_string(),
        },
    )
    .await
}
