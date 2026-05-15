use super::super::media_helpers::media_updates::adjust_analyzed_book_read_progress;
use super::*;
use crate::resolve_library_item_path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::task_queue) struct AnalyzeBookOutcome {
    pub(in crate::task_queue) series_id: String,
    pub(in crate::task_queue) media_status: String,
}

pub(in crate::task_queue) async fn analyze_book(
    runtime: &JobRuntime<'_>,
    book_id: &str,
) -> Result<AnalyzeBookOutcome, TaskExecutionError> {
    let book_id = book_id.to_string();
    if !runtime.database().owns_main_database() {
        return Ok(AnalyzeBookOutcome {
            series_id: String::new(),
            media_status: String::new(),
        });
    }

    let Some(input) = analyze_book_input(runtime.database().read_pool(), &book_id)
        .await
        .map_err(TaskExecutionError::runtime)?
    else {
        return Ok(AnalyzeBookOutcome {
            series_id: String::new(),
            media_status: String::new(),
        });
    };

    let file_path = resolve_library_item_path(&input.root, &input.url);
    let analysis =
        analyze_book_media_file(&file_path, input.analyze_dimensions).map_err(|error| {
            TaskExecutionError::runtime(format!(
                "failed to analyze media file for '{book_id}' ('{}'): {error}",
                file_path.display(),
            ))
        })?;

    let persisted = AnalyzedBookMedia {
        status: analysis.status,
        media_type: analysis.media_type,
        pages: analysis
            .pages
            .into_iter()
            .map(|page| AnalyzedBookPage {
                file_name: page.file_name,
                media_type: page.media_type,
                width: page.width,
                height: page.height,
                file_size: page.file_size,
            })
            .collect(),
    };
    let current_page_count = persisted.pages.len() as i64;

    persist_book_analysis(
        runtime.database().write_pool(),
        runtime.database().main_db().database_file(),
        runtime.search().lucene_data_directory(),
        &book_id,
        &persisted,
        runtime.search().owns_search_index(),
    )
    .await
    .map_err(TaskExecutionError::runtime)?;

    adjust_analyzed_book_read_progress(
        runtime.database().write_pool(),
        &book_id,
        &input.series_id,
        &input.previous_media_status,
        input.previous_page_count,
        current_page_count,
    )
    .await
    .map_err(TaskExecutionError::runtime)?;

    Ok(AnalyzeBookOutcome {
        series_id: input.series_id,
        media_status: persisted.status,
    })
}
