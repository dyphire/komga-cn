use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use super::db_queries::{
    load_persisted_book_media, load_persisted_book_pages, public_page_number_to_persisted,
};
use super::page_content::resolve_book_page_bytes;

pub(crate) async fn persist_book_page_hashes_from_media_content(
    pool: &SqlitePool,
    book_id: &str,
) -> Result<(), String> {
    let media = load_persisted_book_media(pool, book_id)
        .await?
        .ok_or_else(|| "book media missing for page hash task".to_string())?;
    let pages = load_persisted_book_pages(pool, book_id).await?;

    let mut hashes = Vec::<(i64, String)>::new();
    for page in pages {
        let Some(bytes) = resolve_book_page_bytes(&media, &page, page.number).await? else {
            continue;
        };
        let Some(persisted_page_number) = public_page_number_to_persisted(page.number) else {
            continue;
        };
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let hash = hasher
            .finalize()
            .iter()
            .map(|value| format!("{value:02x}"))
            .collect::<String>();
        hashes.push((persisted_page_number, hash));
    }

    if hashes.is_empty() {
        return Ok(());
    }

    for (number, hash) in hashes {
        sqlx::query("UPDATE MEDIA_PAGE SET FILE_HASH = ? WHERE BOOK_ID = ? AND NUMBER = ?")
            .bind(hash)
            .bind(book_id)
            .bind(number)
            .execute(pool)
            .await
            .map_err(|error| format!("persist media-page hash: {error}"))?;
    }

    Ok(())
}
