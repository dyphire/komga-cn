use super::*;

pub(super) async fn load_persisted_epub_positions(
    app: &MediaAssetsState,
    book_id: &str,
) -> Result<Option<Vec<Value>>, String> {
    let Some((extension_class, blob)) = app.reader.epub_extension_blob(book_id).await? else {
        return Ok(None);
    };
    if !extension_class.is_empty()
        && !extension_class
            .to_ascii_lowercase()
            .contains("mediaextensionepub")
    {
        return Ok(None);
    }

    let positions = decode_epub_positions_blob(app, &blob)?;
    if positions.is_empty() {
        Ok(None)
    } else {
        Ok(Some(positions))
    }
}

pub(super) fn decode_epub_positions_blob(
    app: &MediaAssetsState,
    blob: &[u8],
) -> Result<Vec<Value>, String> {
    app.content.decode_epub_positions_blob(blob)
}
