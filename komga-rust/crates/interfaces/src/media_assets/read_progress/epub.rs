use super::*;
use komga_application::media_assets::{EpubNavigationLoadError, load_book_epub_navigation};

pub(super) async fn load_epub_locator_for_page(
    app: &MediaAssetsState,
    book_id: &str,
    page: u64,
) -> Result<Option<Value>, String> {
    match load_book_epub_navigation(app.reader.as_ref(), app.content.as_ref(), book_id).await {
        Ok(navigation) => Ok(navigation.locator_for_page(page)),
        Err(EpubNavigationLoadError::MissingExtension) => Ok(None),
        Err(EpubNavigationLoadError::Internal(error)) => Err(error),
    }
}
