use komga_application::media_assets::{
    BookMetadata, BookMetadataAuthor, BookMetadataLink, BookMetadataPort,
};
use sqlx::{Row, SqlitePool};

#[derive(Clone, Debug)]
pub struct SqliteBookMetadataPort {
    read_pool: SqlitePool,
    write_pool: SqlitePool,
}

impl SqliteBookMetadataPort {
    pub fn new(read_pool: SqlitePool, write_pool: SqlitePool) -> Self {
        Self {
            read_pool,
            write_pool,
        }
    }
}

#[async_trait::async_trait]
impl BookMetadataPort for SqliteBookMetadataPort {
    async fn load_book_metadata(&self, book_id: &str) -> Result<Option<BookMetadata>, String> {
        load_book_metadata(&self.read_pool, book_id).await
    }

    async fn load_book_series_id(&self, book_id: &str) -> Result<Option<String>, String> {
        load_book_series_id(&self.read_pool, book_id).await
    }

    async fn load_book_library_id(&self, book_id: &str) -> Result<Option<String>, String> {
        load_book_library_id(&self.read_pool, book_id).await
    }

    async fn persist_book_metadata(
        &self,
        book_id: &str,
        metadata: &BookMetadata,
    ) -> Result<bool, String> {
        persist_book_metadata(&self.write_pool, book_id, metadata).await
    }
}

async fn load_book_metadata(
    pool: &SqlitePool,
    book_id: &str,
) -> Result<Option<BookMetadata>, String> {
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
    .map_err(|error| format!("query existing book metadata: {error}"))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let author_rows = sqlx::query(
        "SELECT NAME, ROLE FROM BOOK_METADATA_AUTHOR WHERE BOOK_ID = ? ORDER BY ROLE ASC, NAME ASC",
    )
    .bind(book_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query existing book metadata authors: {error}"))?;

    let tag_rows = sqlx::query(
        "SELECT TAG FROM BOOK_METADATA_TAG WHERE BOOK_ID = ? ORDER BY TAG COLLATE NOCASE ASC",
    )
    .bind(book_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query existing book metadata tags: {error}"))?;

    let link_rows = sqlx::query(
        "SELECT LABEL, URL FROM BOOK_METADATA_LINK WHERE BOOK_ID = ? ORDER BY LABEL COLLATE NOCASE ASC, URL ASC",
    )
    .bind(book_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query existing book metadata links: {error}"))?;

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

async fn load_book_series_id(pool: &SqlitePool, book_id: &str) -> Result<Option<String>, String> {
    let row = sqlx::query("SELECT SERIES_ID FROM BOOK WHERE ID = ? LIMIT 1")
        .bind(book_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("query book series id: {error}"))?;

    Ok(row.map(|row| row.get::<String, _>("SERIES_ID")))
}

async fn load_book_library_id(pool: &SqlitePool, book_id: &str) -> Result<Option<String>, String> {
    let row = sqlx::query("SELECT LIBRARY_ID FROM BOOK WHERE ID = ? LIMIT 1")
        .bind(book_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("query book library id: {error}"))?;

    Ok(row.map(|row| row.get::<String, _>("LIBRARY_ID")))
}

async fn persist_book_metadata(
    pool: &SqlitePool,
    book_id: &str,
    metadata: &BookMetadata,
) -> Result<bool, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin book metadata update tx: {error}"))?;

    let exists = sqlx::query("SELECT 1 AS FOUND FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1")
        .bind(book_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| format!("query book metadata existence for update: {error}"))?
        .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback book metadata update tx: {error}"))?;
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
    .map_err(|error| format!("update book metadata: {error}"))?;

    sqlx::query("UPDATE BOOK SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP WHERE ID = ?")
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("touch book last modified after metadata update: {error}"))?;

    sqlx::query("DELETE FROM BOOK_METADATA_AUTHOR WHERE BOOK_ID = ?")
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("delete existing book metadata authors: {error}"))?;
    for author in &metadata.authors {
        sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
            .bind(book_id)
            .bind(&author.name)
            .bind(&author.role)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("insert updated book metadata author: {error}"))?;
    }

    sqlx::query("DELETE FROM BOOK_METADATA_TAG WHERE BOOK_ID = ?")
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("delete existing book metadata tags: {error}"))?;
    for tag in &metadata.tags {
        sqlx::query("INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) VALUES (?, ?)")
            .bind(book_id)
            .bind(tag)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("insert updated book metadata tag: {error}"))?;
    }

    sqlx::query("DELETE FROM BOOK_METADATA_LINK WHERE BOOK_ID = ?")
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("delete existing book metadata links: {error}"))?;
    for link in &metadata.links {
        sqlx::query("INSERT INTO BOOK_METADATA_LINK (BOOK_ID, LABEL, URL) VALUES (?, ?, ?)")
            .bind(book_id)
            .bind(&link.label)
            .bind(&link.url)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("insert updated book metadata link: {error}"))?;
    }

    tx.commit()
        .await
        .map_err(|error| format!("commit book metadata update tx: {error}"))?;
    Ok(true)
}
