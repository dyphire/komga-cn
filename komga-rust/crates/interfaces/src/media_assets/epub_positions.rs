use super::*;
use komga_application::media_assets::{EpubNavigationLoadError, load_book_epub_navigation};

pub(super) async fn load_persisted_epub_positions(
    app: &MediaAssetsState,
    book_id: &str,
) -> Result<Option<Vec<Value>>, String> {
    let navigation =
        match load_book_epub_navigation(app.reader.as_ref(), app.content.as_ref(), book_id).await {
            Ok(navigation) => navigation,
            Err(EpubNavigationLoadError::MissingExtension) => return Ok(None),
            Err(EpubNavigationLoadError::Internal(error)) => return Err(error),
        };
    let positions = navigation.positions().to_vec();
    if positions.is_empty() {
        Ok(None)
    } else {
        Ok(Some(positions))
    }
}
