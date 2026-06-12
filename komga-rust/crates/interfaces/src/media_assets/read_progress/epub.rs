use komga_application::media_assets::{EpubNavigationLoadError, load_book_epub_navigation};
use serde_json::Value;

use crate::state::MediaAssetsState;

pub(super) async fn load_epub_locator_for_page(
    app: &MediaAssetsState,
    book_id: &str,
    page: u64,
) -> Result<Option<Value>, String> {
    match load_book_epub_navigation(
        app.epub_navigation_reader.as_ref(),
        app.epub_navigation_content.as_ref(),
        book_id,
    )
    .await
    {
        Ok(navigation) => Ok(navigation.locator_for_page(page)),
        Err(EpubNavigationLoadError::MissingExtension) => Ok(None),
        Err(EpubNavigationLoadError::Internal(error)) => Err(error),
    }
}
