use anyhow::Context;
use komga_application::media_assets::{
    BookMediaRecord, BookMetadata, BookMetadataAuthor, BookMetadataLink, BookPageRecord,
};
use sqlx::{Row, SqlitePool};

use komga_infrastructure_base::resolve_library_item_path;
use komga_infrastructure_media_core::content::persistence::public_page_number_to_persisted;

fn persisted_page_number_to_public(number: i64) -> u64 {
    number as u64 + 1
}

pub(super) async fn load_book_media_for_refresh(
    pool: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Option<BookMediaRecord>> {
    let row = sqlx::query(
        r#"
        SELECT b.LIBRARY_ID AS LIBRARY_ID, b.NAME AS FILE_NAME, b.URL AS BOOK_URL,
               l.ROOT AS LIBRARY_ROOT,
               COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
               COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT
        FROM BOOK b
        JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
        LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
        WHERE b.ID = ?
        "#,
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .context("query persisted book media for refresh")?;

    Ok(row.map(|row| BookMediaRecord {
        library_id: row.get::<String, _>("LIBRARY_ID"),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        file_path: resolve_library_item_path(
            row.get::<String, _>("LIBRARY_ROOT").as_str(),
            row.get::<String, _>("BOOK_URL").as_str(),
        ),
        file_name: row.get::<String, _>("FILE_NAME"),
        page_count: row.get::<i64, _>("PAGE_COUNT").max(0) as u64,
    }))
}

pub(super) async fn load_book_page_row_for_refresh(
    pool: &SqlitePool,
    book_id: &str,
    page_number: u64,
) -> anyhow::Result<Option<BookPageRecord>> {
    let Some(persisted_page_number) = public_page_number_to_persisted(page_number) else {
        return Ok(None);
    };

    let row = sqlx::query(
        r#"
        SELECT NUMBER, FILE_NAME, MEDIA_TYPE, width, height,
               CASE WHEN FILE_SIZE IS NULL THEN -1 ELSE FILE_SIZE END AS FILE_SIZE
        FROM MEDIA_PAGE
        WHERE BOOK_ID = ? AND NUMBER = ?
        LIMIT 1
        "#,
    )
    .bind(book_id)
    .bind(persisted_page_number)
    .fetch_optional(pool)
    .await
    .context("query single persisted book page for refresh")?;

    Ok(row.map(|row| BookPageRecord {
        number: persisted_page_number_to_public(row.get::<i64, _>("NUMBER")),
        file_name: row.get::<String, _>("FILE_NAME"),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        width: row.get::<Option<i64>, _>("width"),
        height: row.get::<Option<i64>, _>("height"),
        file_size: row.get::<i64, _>("FILE_SIZE"),
    }))
}

pub(super) async fn load_book_metadata_for_refresh(
    pool: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Option<BookMetadata>> {
    let row = sqlx::query(
        r#"
        SELECT TITLE, TITLE_LOCK, SUMMARY, SUMMARY_LOCK, NUMBER, NUMBER_LOCK, NUMBER_SORT,
               NUMBER_SORT_LOCK, RELEASE_DATE, RELEASE_DATE_LOCK, AUTHORS_LOCK, TAGS_LOCK, ISBN,
               ISBN_LOCK, LINKS_LOCK
        FROM BOOK_METADATA
        WHERE BOOK_ID = ?
        LIMIT 1
        "#,
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .context("query existing book metadata for refresh")?;

    let Some(row) = row else {
        return Ok(None);
    };

    let author_rows = sqlx::query(
        "SELECT NAME, ROLE FROM BOOK_METADATA_AUTHOR WHERE BOOK_ID = ? ORDER BY ROLE ASC, NAME ASC",
    )
    .bind(book_id)
    .fetch_all(pool)
    .await
    .context("query existing book metadata authors for refresh: ")?;

    let tag_rows = sqlx::query(
        "SELECT TAG FROM BOOK_METADATA_TAG WHERE BOOK_ID = ? ORDER BY TAG COLLATE NOCASE ASC",
    )
    .bind(book_id)
    .fetch_all(pool)
    .await
    .context("query existing book metadata tags for refresh")?;

    let link_rows = sqlx::query(
        "SELECT LABEL, URL FROM BOOK_METADATA_LINK WHERE BOOK_ID = ? ORDER BY LABEL COLLATE NOCASE ASC, URL ASC",
    )
    .bind(book_id)
    .fetch_all(pool)
    .await
    .context("query existing book metadata links for refresh")?;

    Ok(Some(BookMetadata {
        title: row.get::<String, _>("TITLE"),
        title_lock: row.get::<i64, _>("TITLE_LOCK") != 0,
        summary: row.get::<String, _>("SUMMARY"),
        summary_lock: row.get::<i64, _>("SUMMARY_LOCK") != 0,
        number: row.get::<String, _>("NUMBER"),
        number_lock: row.get::<i64, _>("NUMBER_LOCK") != 0,
        number_sort: row.get::<f64, _>("NUMBER_SORT"),
        number_sort_lock: row.get::<i64, _>("NUMBER_SORT_LOCK") != 0,
        release_date: row.get::<Option<String>, _>("RELEASE_DATE"),
        release_date_lock: row.get::<i64, _>("RELEASE_DATE_LOCK") != 0,
        authors: author_rows
            .into_iter()
            .map(|entry| BookMetadataAuthor {
                name: entry.get::<String, _>("NAME"),
                role: entry.get::<String, _>("ROLE"),
            })
            .collect(),
        authors_lock: row.get::<i64, _>("AUTHORS_LOCK") != 0,
        tags: tag_rows
            .into_iter()
            .map(|entry| entry.get::<String, _>("TAG"))
            .collect(),
        tags_lock: row.get::<i64, _>("TAGS_LOCK") != 0,
        isbn: row.get::<String, _>("ISBN"),
        isbn_lock: row.get::<i64, _>("ISBN_LOCK") != 0,
        links: link_rows
            .into_iter()
            .map(|entry| BookMetadataLink {
                label: entry.get::<String, _>("LABEL"),
                url: entry.get::<String, _>("URL"),
            })
            .collect(),
        links_lock: row.get::<i64, _>("LINKS_LOCK") != 0,
    }))
}

pub(super) async fn persist_book_metadata_for_refresh(
    pool: &SqlitePool,
    book_id: &str,
    metadata: &BookMetadata,
) -> anyhow::Result<bool> {
    let mut tx = pool
        .begin()
        .await
        .context("begin book metadata refresh tx")?;

    let exists = sqlx::query("SELECT 1 AS FOUND FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1")
        .bind(book_id)
        .fetch_optional(&mut *tx)
        .await
        .context("query book metadata existence for refresh")?
        .is_some();
    if !exists {
        tx.rollback()
            .await
            .context("rollback book metadata refresh tx")?;
        return Ok(false);
    }

    sqlx::query(
        r#"
        UPDATE BOOK_METADATA
        SET TITLE = ?, TITLE_LOCK = ?, SUMMARY = ?, SUMMARY_LOCK = ?, NUMBER = ?,
            NUMBER_LOCK = ?, NUMBER_SORT = ?, NUMBER_SORT_LOCK = ?, RELEASE_DATE = ?,
            RELEASE_DATE_LOCK = ?, AUTHORS_LOCK = ?, TAGS_LOCK = ?, ISBN = ?, ISBN_LOCK = ?,
            LINKS_LOCK = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
        WHERE BOOK_ID = ?
        "#,
    )
    .bind(&metadata.title)
    .bind(metadata.title_lock)
    .bind(&metadata.summary)
    .bind(metadata.summary_lock)
    .bind(&metadata.number)
    .bind(metadata.number_lock)
    .bind(metadata.number_sort)
    .bind(metadata.number_sort_lock)
    .bind(metadata.release_date.as_deref())
    .bind(metadata.release_date_lock)
    .bind(metadata.authors_lock)
    .bind(metadata.tags_lock)
    .bind(&metadata.isbn)
    .bind(metadata.isbn_lock)
    .bind(metadata.links_lock)
    .bind(book_id)
    .execute(&mut *tx)
    .await
    .context("update book metadata for refresh")?;

    sqlx::query("DELETE FROM BOOK_METADATA_AUTHOR WHERE BOOK_ID = ?")
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .context("delete existing book metadata authors for refresh: ")?;
    for author in &metadata.authors {
        sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
            .bind(book_id)
            .bind(&author.name)
            .bind(&author.role)
            .execute(&mut *tx)
            .await
            .context("insert refreshed book metadata author")?;
    }

    sqlx::query("DELETE FROM BOOK_METADATA_TAG WHERE BOOK_ID = ?")
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .context("delete existing book metadata tags for refresh")?;
    for tag in &metadata.tags {
        sqlx::query("INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) VALUES (?, ?)")
            .bind(book_id)
            .bind(tag)
            .execute(&mut *tx)
            .await
            .context("insert refreshed book metadata tag")?;
    }

    sqlx::query("DELETE FROM BOOK_METADATA_LINK WHERE BOOK_ID = ?")
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .context("delete existing book metadata links for refresh")?;
    for link in &metadata.links {
        sqlx::query("INSERT INTO BOOK_METADATA_LINK (BOOK_ID, LABEL, URL) VALUES (?, ?, ?)")
            .bind(book_id)
            .bind(&link.label)
            .bind(&link.url)
            .execute(&mut *tx)
            .await
            .context("insert refreshed book metadata link")?;
    }

    tx.commit()
        .await
        .context("commit book metadata refresh tx")?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::load_book_page_row_for_refresh;
    use komga_infrastructure_test_support::BootstrappedBookFixture;

    #[tokio::test]
    async fn load_book_page_row_for_refresh_maps_public_page_number_to_persisted_row() {
        let fixture = BootstrappedBookFixture::open("refresh-page-row").await;
        fixture.insert_library_series().await;
        fixture.insert_book("book-1").await;
        fixture
            .insert_media_page("book-1", 0, "0001.jpg", "image/jpeg", Some(1234))
            .await;

        let page = load_book_page_row_for_refresh(&fixture.pool, "book-1", 1)
            .await
            .expect("load book page")
            .expect("public page one should map to persisted row zero");

        assert_eq!(1, page.number);
        assert_eq!("0001.jpg", page.file_name);
        assert_eq!(Some(640), page.width);
        assert_eq!(Some(480), page.height);
        assert_eq!(1234, page.file_size);

        fixture.close().await;
    }
}
