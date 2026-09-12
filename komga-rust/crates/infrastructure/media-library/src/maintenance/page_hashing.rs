use anyhow::Context;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use komga_infrastructure_media_core::content::page_rendering::resolve_book_page_bytes;
use komga_infrastructure_media_core::content::persistence::{
    load_persisted_book_media, load_persisted_book_pages, public_page_number_to_persisted,
};

pub(crate) async fn persist_book_page_hashes_from_media_content(
    pool: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<()> {
    let media = load_persisted_book_media(pool, book_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("book media missing for page hash task"))?;
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

    let mut tx = pool.begin().await.context("begin media-page hash transaction")?;

    sqlx::query("DROP TABLE IF EXISTS _page_hash_updates")
        .execute(&mut *tx)
        .await
        .context("drop stale page-hash temp table")?;
    sqlx::query(
        "CREATE TEMP TABLE _page_hash_updates (NUMBER INTEGER PRIMARY KEY, FILE_HASH TEXT NOT NULL)",
    )
    .execute(&mut *tx)
    .await
    .context("create page-hash temp table")?;

    let mut insert = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
        "INSERT INTO _page_hash_updates (NUMBER, FILE_HASH) ",
    );
    insert
        .push_values(&hashes, |mut binder, (number, hash)| {
            binder.push_bind(number).push_bind(hash);
        })
        .build()
        .execute(&mut *tx)
        .await
        .context("bulk insert page-hash temp values")?;

    sqlx::query(
        "UPDATE MEDIA_PAGE
            SET FILE_HASH = (
                SELECT u.FILE_HASH FROM _page_hash_updates u WHERE u.NUMBER = MEDIA_PAGE.NUMBER
            )
          WHERE BOOK_ID = ?
            AND EXISTS (
                SELECT 1 FROM _page_hash_updates u WHERE u.NUMBER = MEDIA_PAGE.NUMBER
            )",
    )
    .bind(book_id)
    .execute(&mut *tx)
    .await
    .context("apply bulk media-page hashes")?;

    tx.commit().await.context("commit media-page hash transaction")?;

    Ok(())
}
