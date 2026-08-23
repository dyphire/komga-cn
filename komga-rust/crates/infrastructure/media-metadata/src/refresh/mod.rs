use std::collections::BTreeSet;

use komga_application::runtime_sse::RuntimeSseEventSink;
use sqlx::{Row, SqlitePool};

use crate::{load_comicinfo_bytes_for_media, parse_comicinfo_xml};
use komga_infrastructure_base::resolve_stored_path;
use komga_infrastructure_media_core::content::epub_resources::load_epub_package_document;

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
mod sources;
mod support;

pub use artwork_refresh::generate_book_thumbnail;
pub use artwork_refresh::{refresh_book_local_artwork, refresh_series_local_artwork};
use epub::{extract_epub_book_patch, extract_epub_series_patch};
pub use series_aggregation::aggregate_series_metadata;
use series_metadata::{
    apply_mylar_series_import, apply_oneshot_series_metadata_import,
    apply_series_metadata_from_book_imports,
};
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

struct SeriesMetadataRefreshContext {
    library_id: Option<String>,
    should_emit_series_changed: bool,
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
) -> anyhow::Result<TransientMetadataProviderInference> {
    let book_patch = extract_epub_book_patch(package_document)?;
    let series_patch = extract_epub_series_patch(package_document)?;
    let mut series_titles = Vec::new();
    push_transient_series_title(&mut series_titles, series_patch.title);

    Ok(TransientMetadataProviderInference {
        series_titles,
        number: book_patch.number_sort,
    })
}

pub fn infer_transient_comicinfo_provider_metadata(
    xml: &[u8],
) -> anyhow::Result<TransientMetadataProviderInference> {
    let document = parse_comicinfo_xml(xml)?;
    let book_patch = extract_comicinfo_book_patch(&document);
    let append_volume_patch = extract_comicinfo_series_patch(&document, true);
    let plain_patch = extract_comicinfo_series_patch(&document, false);
    let mut series_titles = Vec::new();
    push_transient_series_title(&mut series_titles, append_volume_patch.title);
    push_transient_series_title(&mut series_titles, plain_patch.title);

    Ok(TransientMetadataProviderInference {
        series_titles,
        number: book_patch.number_sort,
    })
}

pub async fn refresh_book_metadata(
    pool: &SqlitePool,
    runtime_events: &dyn RuntimeSseEventSink,
    book_id: &str,
    capabilities: &BTreeSet<String>,
) -> anyhow::Result<RefreshBookMetadataOutcome> {
    let book_id = book_id.to_string();
    let book_id_for_events = book_id.clone();
    let outcome = {
        let mut changed_readlist_ids = BTreeSet::new();
        let mut should_emit_book_changed = false;
        let book_row = sqlx::query(
            r#"
            SELECT b.URL AS BOOK_URL,
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
            anyhow::anyhow!(error).context(format!(
                "failed to resolve book path for metadata refresh '{book_id}': "
            ))
        })?;

        if let Some(book_row) = &book_row {
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
            let should_read_comicinfo = (import_comicinfo_book || import_comicinfo_readlist)
                && comicinfo_provider_matches_capabilities(capabilities);
            if should_read_comicinfo
                && let Some(media) = load_book_media_for_refresh(pool, &book_id).await?
                && let Some(xml) = match load_cached_comicinfo_bytes(pool, &book_id).await? {
                    CacheLookup::Found(Some(xml)) => Some(xml),
                    CacheLookup::Found(None) => None,
                    CacheLookup::NotFound => load_comicinfo_bytes_for_media(&media)?,
                }
            {
                let document = parse_comicinfo_xml(&xml).map_err(|error| {
                    anyhow::anyhow!(error).context(format!(
                        "failed to parse ComicInfo.xml from '{}': ",
                        media.file_path.display()
                    ))
                })?;
                if import_comicinfo_book {
                    let patch = extract_comicinfo_book_patch(&document);
                    apply_book_metadata_import_patch(pool, &book_id, patch).await?;
                }

                if import_comicinfo_readlist {
                    for readlist in extract_comicinfo_readlists(&document) {
                        if let Some(readlist_id) = readlist::upsert_comicinfo_readlist(
                            pool,
                            runtime_events,
                            &book_id,
                            readlist,
                        )
                        .await?
                        {
                            changed_readlist_ids.insert(readlist_id);
                        }
                    }
                }
            }

            if import_epub_book
                && epub_provider_matches_capabilities(capabilities)
                && let Some(media) = load_book_media_for_refresh(pool, &book_id).await?
                && let Some(package_document) = match load_cached_epub_package_document(pool, &book_id).await? {
                    CacheLookup::Found(Some(doc)) => Some(doc),
                    CacheLookup::Found(None) => None,
                    CacheLookup::NotFound => load_epub_package_document(&media).await?,
                }
            {
                let patch = extract_epub_book_patch(&package_document)?;
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
        .map_err(|error| {
            anyhow::anyhow!(error)
                .context(format!("failed to refresh BOOK_METADATA for '{book_id}'"))
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
        .map_err(|error| {
            anyhow::anyhow!(error).context(format!(
                "failed to resolve book SSE context for '{book_id}': "
            ))
        })?;
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
        events::emit_book_changed(runtime_events, &book_id_for_events, series_id, library_id);
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
) -> anyhow::Result<()> {
    let Some(mut metadata) = load_book_metadata_for_refresh(pool, book_id).await? else {
        return Ok(());
    };

    if patch::apply_patch_to_metadata(&mut metadata, patch) {
        let persisted = persist_book_metadata_for_refresh(pool, book_id, &metadata).await?;
        if !persisted {
            return Err(anyhow::anyhow!(format!(
                "book metadata row disappeared before metadata refresh for '{book_id}'"
            )));
        }
    }

    Ok(())
}

use queries::{
    load_book_media_for_refresh, load_book_metadata_for_refresh, load_book_page_row_for_refresh,
    load_cached_comicinfo_bytes, load_cached_epub_package_document, CacheLookup,
    persist_book_metadata_for_refresh,
};

pub async fn refresh_series_metadata(
    pool: &SqlitePool,
    runtime_events: &dyn RuntimeSseEventSink,
    series_id: &str,
) -> anyhow::Result<()> {
    let series_id = series_id.to_string();
    let series_id_for_events = series_id.clone();

    let refresh_context = {
        let mut should_emit_series_changed = false;
        let series_row = sqlx::query(
                r#"
                SELECT s.URL AS SERIES_URL,
                       l.ROOT AS LIBRARY_ROOT,
                       s.ONESHOT AS ONESHOT,
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
        .map_err(|error| { anyhow::anyhow!(error).context( format!("failed to resolve series path for metadata refresh '{series_id}'"))
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
                runtime_events,
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
        .map_err(|error| {
            anyhow::anyhow!(error).context(format!(
                "failed to refresh SERIES_METADATA for '{series_id}': "
            ))
        })?;

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
            anyhow::anyhow!(error).context(format!(
                "failed to resolve LIBRARY_ID for refreshed series '{series_id}': "
            ))
        })
        .map(|row| SeriesMetadataRefreshContext {
            library_id: row.and_then(|row| row.get::<Option<String>, _>("LIBRARY_ID")),
            should_emit_series_changed,
        })
    }?;

    if refresh_context.should_emit_series_changed
        && let Some(library_id) = refresh_context.library_id.as_deref()
    {
        events::emit_series_changed(runtime_events, &series_id_for_events, library_id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;

    use komga_application::runtime_sse::RuntimeSseEventStore;

    use super::refresh_book_metadata;
    use komga_infrastructure_test_support::BootstrappedBookFixture;

    async fn set_library_import_flag(pool: &sqlx::SqlitePool, column: &str, value: i64) {
        let query = match column {
            "IMPORT_COMICINFO_BOOK" => "UPDATE LIBRARY SET IMPORT_COMICINFO_BOOK = ? WHERE ID = ?",
            "IMPORT_COMICINFO_READLIST" => {
                "UPDATE LIBRARY SET IMPORT_COMICINFO_READLIST = ? WHERE ID = ?"
            }
            "IMPORT_EPUB_BOOK" => "UPDATE LIBRARY SET IMPORT_EPUB_BOOK = ? WHERE ID = ?",
            "IMPORT_BARCODE_ISBN" => "UPDATE LIBRARY SET IMPORT_BARCODE_ISBN = ? WHERE ID = ?",
            "IMPORT_COMICINFO_SERIES" => {
                "UPDATE LIBRARY SET IMPORT_COMICINFO_SERIES = ? WHERE ID = ?"
            }
            "IMPORT_COMICINFO_COLLECTION" => {
                "UPDATE LIBRARY SET IMPORT_COMICINFO_COLLECTION = ? WHERE ID = ?"
            }
            "IMPORT_COMICINFO_SERIES_APPEND_VOLUME" => {
                "UPDATE LIBRARY SET IMPORT_COMICINFO_SERIES_APPEND_VOLUME = ? WHERE ID = ?"
            }
            "IMPORT_EPUB_SERIES" => "UPDATE LIBRARY SET IMPORT_EPUB_SERIES = ? WHERE ID = ?",
            "IMPORT_MYLAR_SERIES" => "UPDATE LIBRARY SET IMPORT_MYLAR_SERIES = ? WHERE ID = ?",
            _ => panic!("unsupported import flag column: {column}"),
        };
        sqlx::query(query)
            .bind(value)
            .bind("library-1")
            .execute(pool)
            .await
            .expect("library import flag should be updated");
    }

    #[tokio::test]
    async fn refresh_book_metadata_ignores_external_comicinfo_sidecar() {
        let fixture =
            BootstrappedBookFixture::open("refresh-book-ignores-external-comicinfo").await;
        let library_root =
            std::env::temp_dir().join(format!("komga-external-comicinfo-{}", std::process::id()));
        let book_path = library_root.join("series/book-1.cbz");
        fs::create_dir_all(book_path.parent().expect("book fixture should have parent"))
            .expect("book fixture parent should be created");
        zip::ZipWriter::new(fs::File::create(&book_path).expect("book archive should be created"))
            .finish()
            .expect("empty book archive should be written");
        fixture.insert_library_series().await;
        fixture.insert_book("book-1").await;
        fixture.insert_book_metadata("book-1").await;
        fixture
            .insert_media_with_page_count("book-1", Some("application/zip"), "ERROR", 0)
            .await;
        sqlx::query("UPDATE LIBRARY SET ROOT = ? WHERE ID = ?")
            .bind(library_root.to_string_lossy().to_string())
            .bind("library-1")
            .execute(&fixture.pool)
            .await
            .expect("library root should point at the readable book archive");
        set_library_import_flag(&fixture.pool, "IMPORT_COMICINFO_BOOK", 1).await;
        sqlx::query(
            "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
        )
        .bind("series/book-1.xml")
        .bind("series/book-1.cbz")
        .bind(1_i64)
        .bind("library-1")
        .execute(&fixture.pool)
        .await
        .expect("missing ComicInfo sidecar row should be inserted");
        let runtime_events = RuntimeSseEventStore::default();
        let capabilities = BTreeSet::from(["TITLE".to_string()]);

        refresh_book_metadata(&fixture.pool, &runtime_events, "book-1", &capabilities)
            .await
            .expect("external ComicInfo sidecars should not be read by book refresh");
        fixture.close().await;
        let _ = fs::remove_dir_all(library_root);
    }

    #[tokio::test]
    async fn refresh_book_metadata_propagates_corrupt_epub_package_error() {
        let fixture = BootstrappedBookFixture::open("refresh-book-corrupt-epub").await;
        let library_root =
            std::env::temp_dir().join(format!("komga-corrupt-book-epub-{}", std::process::id()));
        let book_path = library_root.join("series/book-1.epub");
        fs::create_dir_all(book_path.parent().expect("book fixture should have parent"))
            .expect("corrupt EPUB parent should be created");
        fs::write(&book_path, b"not a zip archive").expect("corrupt EPUB should be written");
        fixture.insert_library_series().await;
        fixture.insert_book("book-1").await;
        fixture.insert_book_metadata("book-1").await;
        fixture
            .insert_media("book-1", Some("application/epub+zip"))
            .await;
        sqlx::query("UPDATE LIBRARY SET ROOT = ?, IMPORT_EPUB_BOOK = ? WHERE ID = ?")
            .bind(library_root.to_string_lossy().to_string())
            .bind(1_i64)
            .bind("library-1")
            .execute(&fixture.pool)
            .await
            .expect("library root and EPUB flag should be updated");
        sqlx::query(
            "UPDATE LIBRARY SET IMPORT_COMICINFO_BOOK = 0, IMPORT_COMICINFO_READLIST = 0 WHERE ID = ?",
        )
            .bind("library-1")
            .execute(&fixture.pool)
            .await
            .expect("ComicInfo import should be disabled for the EPUB error test");
        sqlx::query("UPDATE BOOK SET NAME = ?, URL = ? WHERE ID = ?")
            .bind("book-1.epub")
            .bind("series/book-1.epub")
            .bind("book-1")
            .execute(&fixture.pool)
            .await
            .expect("book should point at corrupt EPUB fixture");
        let runtime_events = RuntimeSseEventStore::default();
        let capabilities = BTreeSet::from(["TITLE".to_string()]);

        let error = refresh_book_metadata(&fixture.pool, &runtime_events, "book-1", &capabilities)
            .await
            .expect_err("corrupt EPUB package should fail metadata refresh");

        assert!(
            error.to_string().contains("EPUB package document"),
            "{error}"
        );
        let _ = fs::remove_dir_all(library_root);
        fixture.close().await;
    }
}
