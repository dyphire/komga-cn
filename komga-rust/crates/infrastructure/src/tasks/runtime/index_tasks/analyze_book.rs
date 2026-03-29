use super::*;

pub(in crate::task_queue) fn analyze_book(
    runtime: &RuntimeConfig,
    book_id: &str,
) -> Result<(), TaskExecutionError> {
    let book_id = book_id.to_string();
    let runtime = runtime.task_runtime_context();
    let Some(input) = analyze_book_input(runtime.database_file.as_path(), &book_id)
        .map_err(TaskExecutionError::runtime)?
    else {
        return Ok(());
    };

    let file_path = PathBuf::from(&input.root).join(&input.url);
    let analysis = analyze_book_media_file(&file_path, &input.url).map_err(|error| {
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
                file_size: page.file_size,
            })
            .collect(),
    };

    persist_book_analysis(
        runtime.database_file.as_path(),
        runtime.lucene_data_directory.as_path(),
        &book_id,
        &persisted,
    )
    .map_err(TaskExecutionError::runtime)?;

    Ok(())
}
