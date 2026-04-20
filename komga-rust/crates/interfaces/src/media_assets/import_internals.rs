use super::*;

pub(super) fn parse_books_import_payload(body: &Value) -> Result<BooksImportPayload, String> {
    let body = body
        .as_object()
        .ok_or_else(|| "books import payload must be a JSON object".to_string())?;

    let copy_mode = match body.get("copyMode").and_then(Value::as_str) {
        Some("MOVE") => ImportCopyMode::Move,
        Some("COPY") => ImportCopyMode::Copy,
        Some("HARDLINK") => ImportCopyMode::Hardlink,
        Some(_) => {
            return Err("copyMode must be one of MOVE, COPY, HARDLINK".to_string());
        }
        None => {
            return Err("copyMode is required".to_string());
        }
    };

    let books = match body.get("books") {
        Some(books) => books
            .as_array()
            .ok_or_else(|| "books must be an array".to_string())?
            .iter()
            .map(|entry| {
                let entry = entry
                    .as_object()
                    .ok_or_else(|| "books entries must be objects".to_string())?;

                let source_file = entry
                    .get("sourceFile")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "books[].sourceFile must be a string".to_string())?;
                let series_id = entry
                    .get("seriesId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "books[].seriesId must be a string".to_string())?;
                if source_file.trim().is_empty() || series_id.trim().is_empty() {
                    return Err(
                        "books[].sourceFile and books[].seriesId must not be blank".to_string()
                    );
                }

                let destination_name = entry
                    .get("destinationName")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);

                let upgrade_book_id = entry
                    .get("upgradeBookId")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);

                Ok(BooksImportEntry {
                    source_file: PathBuf::from(source_file),
                    series_id: series_id.to_string(),
                    destination_name,
                    upgrade_book_id,
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };

    Ok(BooksImportPayload { copy_mode, books })
}
