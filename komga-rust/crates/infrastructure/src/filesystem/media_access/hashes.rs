use std::path::Path;

use sha2::{Digest, Sha256};

use crate::sqlite::connect_pool;

use super::{load_persisted_book_media, load_persisted_book_pages, resolve_book_page_bytes};

pub async fn persist_book_page_hashes_from_media_content(
    database_file: &Path,
    book_id: &str,
) -> Result<(), String> {
    let media = load_persisted_book_media(database_file, book_id)
        .await?
        .ok_or_else(|| "book media missing for page hash task".to_string())?;
    let pages = load_persisted_book_pages(database_file, book_id).await?;

    let mut hashes = Vec::<(i64, String)>::new();
    for page in pages {
        let Some(bytes) = resolve_book_page_bytes(&media, &page, page.number) else {
            continue;
        };
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let hash = hasher
            .finalize()
            .iter()
            .map(|value| format!("{value:02x}"))
            .collect::<String>();
        hashes.push((page.number as i64, hash));
    }

    if hashes.is_empty() {
        return Ok(());
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open media-page hash db: {error}"))?;
    for (number, hash) in hashes {
        sqlx::query("UPDATE MEDIA_PAGE SET FILE_HASH = ? WHERE BOOK_ID = ? AND NUMBER = ?")
            .bind(hash)
            .bind(book_id)
            .bind(number)
            .execute(&pool)
            .await
            .map_err(|error| format!("persist media-page hash: {error}"))?;
    }

    Ok(())
}
