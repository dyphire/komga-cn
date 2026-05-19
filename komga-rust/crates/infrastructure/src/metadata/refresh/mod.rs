use std::collections::BTreeSet;

use sqlx::{Row, SqlitePool};

use crate::filesystem::media_access::epub::load_epub_package_document;
use crate::{resolve_library_item_path, resolve_stored_path};

mod artwork_refresh;
mod artwork_support;
mod barcode;
mod epub;
mod events;
mod patch;
mod queries;
mod readlist;
mod series_aggregation;
mod series_metadata;
mod sidecar_query;
mod sources;
mod support;

pub use artwork_refresh::{
    generate_book_thumbnail, refresh_book_local_artwork, refresh_series_local_artwork,
};
use epub::{extract_epub_book_patch, extract_epub_series_patch};
pub use series_aggregation::aggregate_series_metadata;
use series_metadata::{
    apply_mylar_series_import, apply_oneshot_series_metadata_import,
    apply_series_metadata_from_book_imports,
};
use sidecar_query::load_sidecar_url_for_parent;
use sources::{
    extract_comicinfo_book_patch, extract_comicinfo_readlists, extract_comicinfo_series_patch,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RefreshBookMetadataOutcome {
    pub series_id: Option<String>,
    pub library_id: Option<String>,
    pub changed_readlist_ids: Vec<String>,
    pub book_changed: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TransientMetadataProviderInference {
    pub series_titles: Vec<String>,
    pub number: Option<f64>,
}

fn push_transient_series_title(series_titles: &mut Vec<String>, title: Option<String>) {
    let Some(title) = title.map(|value| value.trim().to_string()) else {
        return;
    };
    if title.is_empty() || series_titles.iter().any(|existing| existing == &title) {
        return;
    }
    series_titles.push(title);
}

pub fn infer_transient_epub_provider_metadata(
    package_document: &[u8],
) -> TransientMetadataProviderInference {
    let book_patch = extract_epub_book_patch(package_document);
    let series_patch = extract_epub_series_patch(package_document);
    let mut series_titles = Vec::new();
    push_transient_series_title(&mut series_titles, series_patch.title);

    TransientMetadataProviderInference {
        series_titles,
        number: book_patch.number_sort,
    }
}

pub fn infer_transient_comicinfo_provider_metadata(
    xml: &str,
) -> TransientMetadataProviderInference {
    let book_patch = extract_comicinfo_book_patch(xml);
    let append_volume_patch = extract_comicinfo_series_patch(xml, true);
    let plain_patch = extract_comicinfo_series_patch(xml, false);
    let mut series_titles = Vec::new();
    push_transient_series_title(&mut series_titles, append_volume_patch.title);
    push_transient_series_title(&mut series_titles, plain_patch.title);

    TransientMetadataProviderInference {
        series_titles,
        number: book_patch.number_sort,
    }
}



pub async fn refresh_book_metadata(
    pool: &SqlitePool,
    book_id: &str,
    capabilities: &BTreeSet<String>,
) -> Result<RefreshBookMetadataOutcome, String> {
    let book_id = book_id.to_string();
    let book_id_for_events = book_id.clone();
    let outcome = {
        let mut changed_readlist_ids = BTreeSet::new();
        let mut should_emit_book_changed = false;
        let book_row = sqlx::query(
            r#"
            SELECT b.URL AS BOOK_URL,
                   l.ROOT AS LIBRARY_ROOT,
                   l.IMPORT_COMICINFO_BOOK AS IMPORT_COMICINFO_BOOK,
                   l.IMPORT_COMICINFO_READLIST AS IMPORT_COMICINFO_READLIST,
                   l.IMPORT_EPUB_BOOK AS IMPORT_EPUB_BOOK,
                   l.IMPORT_BARCODE_ISBN AS IMPORT_BARCODE_ISBN
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
            format!("failed to resolve book path for metadata refresh '{book_id}': {error}")
        })?;

        if let Some(book_row) = &book_row {
            let book_url = book_row.get::<String, _>("BOOK_URL");
            let library_root = book_row.get::<String, _>("LIBRARY_ROOT");
            let import_comicinfo_book = book_row.get::<bool, _>("IMPORT_COMICINFO_BOOK");
            let import_comicinfo_readlist = book_row.get::<bool, _>("IMPORT_COMICINFO_READLIST");
            let import_epub_book = book_row.get::<bool, _>("IMPORT_EPUB_BOOK");
            let import_barcode_isbn = book_row.get::<bool, _>("IMPORT_BARCODE_ISBN");
            should_emit_book_changed |=
                import_comicinfo_book && comicinfo_provider_matches_capabilities(capabilities);
            should_emit_book_changed |=
                import_epub_book && epub_provider_matches_capabilities(capabilities);
            should_emit_book_changed |=
                import_barcode_isbn && barcode_provider_matches_capabilities(capabilities);
            if let Some(sidecar_url) = load_sidecar_url_for_parent(pool, &book_url, true).await? {
                let sidecar_path = resolve_library_item_path(&library_root, &sidecar_url);
                if comicinfo_provider_matches_capabilities(capabilities)
                    && let Ok(xml) = tokio::fs::read_to_string(&sidecar_path).await
                {
                    if import_comicinfo_book {
                        let patch = extract_comicinfo_book_patch(&xml);
                        apply_book_metadata_import_patch(pool, &book_id, patch).await?;
                    }

                    if import_comicinfo_readlist {
                        for readlist in extract_comicinfo_readlists(&xml) {
                            if let Some(readlist_id) =
                                readlist::upsert_comicinfo_readlist(pool, &book_id, readlist).await?
                            {
                                changed_readlist_ids.insert(readlist_id);
                            }
                        }
                    }
                }
            }

            if import_epub_book
                && epub_provider_matches_capabilities(capabilities)
                && let Some(media) = load_book_media_for_refresh(pool, &book_id).await?
                && let Some(package_document) = load_epub_package_document(&media).await
            {
                let patch = extract_epub_book_patch(&package_document);
                apply_book_metadata_import_patch(pool, &book_id, patch).await?;
            }

            if import_barcode_isbn && barcode_provider_matches_capabilities(capabilities) {
                barcode::refresh_barcode_isbn(pool, &book_id).await?;
            }
        }

        sqlx::query(
            r#"
            UPDATE BOOK_METADATA
            SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
            WHERE BOOK_ID = ?
            "#,
        )
        .bind(&book_id)
        .execute(pool)
        .await
        .map_err(|error| format!("failed to refresh BOOK_METADATA for '{book_id}': {error}"))?;

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
            format!("failed to refresh BOOK row timestamp for '{book_id}': {error}")
        })?;

        let book_context = sqlx::query(
            r#"
            SELECT SERIES_ID, LIBRARY_ID
            FROM BOOK
            WHERE ID = ?
            LIMIT 1
            "#,
        )
        .bind(&book_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("failed to resolve book SSE context for '{book_id}': {error}"))?;
        let series_id = book_context
            .as_ref()
            .and_then(|row| row.get::<Option<String>, _>("SERIES_ID"));
        let library_id = book_context
            .as_ref()
            .and_then(|row| row.get::<Option<String>, _>("LIBRARY_ID"));

        RefreshBookMetadataOutcome {
            series_id,
            library_id,
            changed_readlist_ids: changed_readlist_ids.into_iter().collect(),
            book_changed: should_emit_book_changed,
        }
    };

    if outcome.book_changed
        && let (Some(series_id), Some(library_id)) =
            (outcome.series_id.as_deref(), outcome.library_id.as_deref())
    {
        events::emit_book_changed(&book_id_for_events, series_id, library_id);
    }

    Ok(outcome)
}


use patch::{BookMetadataImportPatch, SeriesMetadataImportPatch};

fn comicinfo_provider_matches_capabilities(capabilities: &BTreeSet<String>) -> bool {
    capabilities.iter().any(|capability| {
        matches!(
            capability.as_str(),
            "TITLE"
                | "SUMMARY"
                | "NUMBER"
                | "NUMBER_SORT"
                | "RELEASE_DATE"
                | "AUTHORS"
                | "READ_LISTS"
                | "LINKS"
        )
    })
}

fn epub_provider_matches_capabilities(capabilities: &BTreeSet<String>) -> bool {
    capabilities.iter().any(|capability| {
        matches!(
            capability.as_str(),
            "TITLE" | "SUMMARY" | "RELEASE_DATE" | "AUTHORS" | "ISBN"
        )
    })
}

fn barcode_provider_matches_capabilities(capabilities: &BTreeSet<String>) -> bool {
    capabilities.contains("ISBN")
}


async fn apply_book_metadata_import_patch(
    pool: &SqlitePool,
    book_id: &str,
    patch: BookMetadataImportPatch,
) -> Result<(), String> {
    let Some(mut metadata) = load_book_metadata_for_refresh(pool, book_id).await? else {
        return Ok(());
    };

    if patch::apply_patch_to_metadata(&mut metadata, patch) {
        let persisted = persist_book_metadata_for_refresh(pool, book_id, &metadata).await?;
        if !persisted {
            return Err(format!(
                "book metadata row disappeared before metadata refresh for '{book_id}'"
            ));
        }
    }

    Ok(())
}

use queries::{
    load_book_media_for_refresh, load_book_metadata_for_refresh, load_book_page_row_for_refresh,
    persist_book_metadata_for_refresh,
};


pub async fn refresh_series_metadata(pool: &SqlitePool, series_id: &str) -> Result<(), String> {
    let series_id = series_id.to_string();
    let series_id_for_events = series_id.clone();

    let (library_id, should_emit_series_changed) = {
        let mut should_emit_series_changed = false;
        let series_row = sqlx::query(
                r#"
                SELECT s.URL AS SERIES_URL,
                       l.ROOT AS LIBRARY_ROOT,
                       COALESCE(s.ONESHOT, 0) AS ONESHOT,
                       l.IMPORT_COMICINFO_SERIES AS IMPORT_COMICINFO_SERIES,
                       l.IMPORT_COMICINFO_COLLECTION AS IMPORT_COMICINFO_COLLECTION,
                       l.IMPORT_COMICINFO_SERIES_APPEND_VOLUME AS IMPORT_COMICINFO_SERIES_APPEND_VOLUME,
                       l.IMPORT_EPUB_SERIES AS IMPORT_EPUB_SERIES,
                       l.IMPORT_MYLAR_SERIES AS IMPORT_MYLAR_SERIES
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
            format!("failed to resolve series path for metadata refresh '{series_id}': {error}")
        })?;

        if let Some(series_row) = &series_row {
            let series_url = series_row.get::<String, _>("SERIES_URL");
            let library_root = series_row.get::<String, _>("LIBRARY_ROOT");
            let resolved_library_root = resolve_stored_path(&library_root);
            let oneshot = series_row.get::<i64, _>("ONESHOT") != 0;
            let import_comicinfo_series = series_row.get::<bool, _>("IMPORT_COMICINFO_SERIES");
            let import_comicinfo_collection =
                series_row.get::<bool, _>("IMPORT_COMICINFO_COLLECTION");
            let import_comicinfo_series_append_volume =
                series_row.get::<bool, _>("IMPORT_COMICINFO_SERIES_APPEND_VOLUME");
            let import_epub_series = series_row.get::<bool, _>("IMPORT_EPUB_SERIES");
            let import_mylar_series = series_row.get::<bool, _>("IMPORT_MYLAR_SERIES");
            should_emit_series_changed =
                import_comicinfo_series || import_epub_series || import_mylar_series || oneshot;

            apply_series_metadata_from_book_imports(
                pool,
                &series_id,
                resolved_library_root.as_path(),
                import_comicinfo_series,
                import_comicinfo_collection,
                import_comicinfo_series_append_volume,
                import_epub_series,
            )
            .await?;

            apply_mylar_series_import(
                pool,
                &series_id,
                resolved_library_root.as_path(),
                &series_url,
                import_mylar_series,
                oneshot,
            )
            .await?;

            if oneshot {
                apply_oneshot_series_metadata_import(pool, &series_id).await?;
            }
        }

        sqlx::query(
            r#"
                UPDATE SERIES_METADATA
                SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE SERIES_ID = ?
                "#,
        )
        .bind(&series_id)
        .execute(pool)
        .await
        .map_err(|error| format!("failed to refresh SERIES_METADATA for '{series_id}': {error}"))?;

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
        .map_err(|error| format!("failed to refresh SERIES row for '{series_id}': {error}"))?;

        sqlx::query(
            r#"
                SELECT LIBRARY_ID
                FROM SERIES
                WHERE ID = ?
                LIMIT 1
                "#,
        )
        .bind(&series_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| {
            format!("failed to resolve LIBRARY_ID for refreshed series '{series_id}': {error}")
        })
        .map(|row| {
            (
                row.and_then(|row| row.get::<Option<String>, _>("LIBRARY_ID")),
                should_emit_series_changed,
            )
        })
    }?;

    if should_emit_series_changed && let Some(library_id) = library_id.as_deref() {
        events::emit_series_changed(&series_id_for_events, library_id);
    }
    Ok(())
}
