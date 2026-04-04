use super::*;

pub(super) async fn load_persisted_epub_positions(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<Vec<Value>>, String> {
    let Some((extension_class, blob)) =
        load_persisted_epub_extension_blob(database_file, book_id).await?
    else {
        return Ok(None);
    };
    if !extension_class.is_empty()
        && !extension_class
            .to_ascii_lowercase()
            .contains("mediaextensionepub")
    {
        return Ok(None);
    }

    let positions = decode_epub_positions_blob(&blob)?;
    if positions.is_empty() {
        Ok(None)
    } else {
        Ok(Some(positions))
    }
}

pub(super) fn decode_epub_positions_blob(blob: &[u8]) -> Result<Vec<Value>, String> {
    decode_epub_positions(blob)
}
