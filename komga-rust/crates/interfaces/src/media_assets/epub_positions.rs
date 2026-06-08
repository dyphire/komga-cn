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

    let positions = app
        .content
        .decode_epub_positions_extension(&blob)?
        .positions;
    if positions.is_empty() {
        Ok(None)
    } else {
        Ok(Some(positions))
    }
}
