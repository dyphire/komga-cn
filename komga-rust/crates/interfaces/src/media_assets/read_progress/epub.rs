use super::*;

pub(super) async fn load_epub_locator_for_page(
    app: &MediaAssetsState,
    book_id: &str,
    page: u64,
) -> Result<Option<Value>, String> {
    match app.reader.epub_extension_blob(book_id).await {
        Ok(Some((_class, blob))) => Ok(app
            .content
            .decode_epub_positions_blob(&blob)
            .ok()
            .and_then(|positions| positions.get(page.saturating_sub(1) as usize).cloned())),
        Ok(None) => Ok(None),
        Err(error) => Err(error),
    }
}
