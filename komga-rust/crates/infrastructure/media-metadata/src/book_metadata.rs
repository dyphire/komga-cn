use anyhow::Context;
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
    async fn load_book_metadata(&self, book_id: &str) -> anyhow::Result<Option<BookMetadata>> {
        load_book_metadata(&self.read_pool, book_id).await
    }

    async fn load_book_series_id(&self, book_id: &str) -> anyhow::Result<Option<String>> {
        load_book_series_id(&self.read_pool, book_id).await
    }

    async fn load_book_library_id(&self, book_id: &str) -> anyhow::Result<Option<String>> {
        load_book_library_id(&self.read_pool, book_id).await
    }

    async fn persist_book_metadata(
        &self,
        book_id: &str,
        metadata: &BookMetadata,
    ) -> anyhow::Result<bool> {
        persist_book_metadata(&self.write_pool, book_id, metadata).await
    }
}

async fn load_book_metadata(
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
    .context("query existing book metadata")?;

    let Some(row) = row else {
        return Ok(None);
    };

    let author_rows = sqlx::query(
        "SELECT NAME, ROLE FROM BOOK_METADATA_AUTHOR WHERE BOOK_ID = ? ORDER BY rowid ASC",
    )
    .bind(book_id)
    .fetch_all(pool)
    .await
    .context("query existing book metadata authors")?;

    let tag_rows = sqlx::query(
        "SELECT DISTINCT TAG FROM BOOK_METADATA_TAG WHERE BOOK_ID = ? ORDER BY rowid ASC",
    )
    .bind(book_id)
    .fetch_all(pool)
    .await
    .context("query existing book metadata tags")?;

    let link_rows = sqlx::query(
        "SELECT LABEL, URL FROM BOOK_METADATA_LINK WHERE BOOK_ID = ? ORDER BY rowid ASC",
    )
    .bind(book_id)
    .fetch_all(pool)
    .await
    .context("query existing book metadata links")?;

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

async fn load_book_series_id(pool: &SqlitePool, book_id: &str) -> anyhow::Result<Option<String>> {
    let row = sqlx::query("SELECT SERIES_ID FROM BOOK WHERE ID = ? LIMIT 1")
        .bind(book_id)
        .fetch_optional(pool)
        .await
        .context("query book series id")?;

    Ok(row.map(|row| row.get::<String, _>("SERIES_ID")))
}

async fn load_book_library_id(pool: &SqlitePool, book_id: &str) -> anyhow::Result<Option<String>> {
    let row = sqlx::query("SELECT LIBRARY_ID FROM BOOK WHERE ID = ? LIMIT 1")
        .bind(book_id)
        .fetch_optional(pool)
        .await
        .context("query book library id")?;

    Ok(row.map(|row| row.get::<String, _>("LIBRARY_ID")))
}

async fn persist_book_metadata(
    pool: &SqlitePool,
    book_id: &str,
    metadata: &BookMetadata,
) -> anyhow::Result<bool> {
    let mut tx = pool
        .begin()
        .await
        .context("begin book metadata update tx")?;

    let exists = sqlx::query("SELECT 1 AS FOUND FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1")
        .bind(book_id)
        .fetch_optional(&mut *tx)
        .await
        .context("query book metadata existence for update")?
        .is_some();
    if !exists {
        tx.rollback()
            .await
            .context("rollback book metadata update tx")?;
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
    .context("update book metadata")?;

    sqlx::query("DELETE FROM BOOK_METADATA_AUTHOR WHERE BOOK_ID = ?")
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .context("delete existing book metadata authors")?;
    if !metadata.authors.is_empty() {
        let mut insert = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) ",
        );
        insert
            .push_values(&metadata.authors, |mut binder, author| {
                binder
                    .push_bind(book_id)
                    .push_bind(&author.name)
                    .push_bind(&author.role);
            })
            .build()
            .execute(&mut *tx)
            .await
            .context("bulk insert updated book metadata authors")?;
    }

    sqlx::query("DELETE FROM BOOK_METADATA_TAG WHERE BOOK_ID = ?")
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .context("delete existing book metadata tags")?;
    if !metadata.tags.is_empty() {
        let mut insert = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) ",
        );
        insert
            .push_values(&metadata.tags, |mut binder, tag| {
                binder.push_bind(book_id).push_bind(tag);
            })
            .build()
            .execute(&mut *tx)
            .await
            .context("bulk insert updated book metadata tags")?;
    }

    sqlx::query("DELETE FROM BOOK_METADATA_LINK WHERE BOOK_ID = ?")
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .context("delete existing book metadata links")?;
    if !metadata.links.is_empty() {
        let mut insert = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "INSERT INTO BOOK_METADATA_LINK (BOOK_ID, LABEL, URL) ",
        );
        insert
            .push_values(&metadata.links, |mut binder, link| {
                binder
                    .push_bind(book_id)
                    .push_bind(&link.label)
                    .push_bind(&link.url);
            })
            .build()
            .execute(&mut *tx)
            .await
            .context("bulk insert updated book metadata links")?;
    }

    tx.commit()
        .await
        .context("commit book metadata update tx")?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use komga_infrastructure_test_support::BootstrappedBookFixture;

    #[tokio::test]
    async fn load_book_metadata_preserves_author_tag_link_insertion_order() {
        let fixture = BootstrappedBookFixture::open("book-metadata-order").await;
        fixture.insert_library_series().await;
        fixture.insert_book("book-1").await;
        fixture.insert_book_metadata("book-1").await;

        for (name, role) in [("Zed", "artist"), ("Alpha", "writer"), ("Zed", "artist")] {
            sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
                .bind("book-1")
                .bind(name)
                .bind(role)
                .execute(&fixture.pool)
                .await
                .expect("book author should be inserted");
        }
        for tag in ["Omega", "Beta", "Omega"] {
            sqlx::query("INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) VALUES (?, ?)")
                .bind("book-1")
                .bind(tag)
                .execute(&fixture.pool)
                .await
                .expect("book metadata tag should be inserted");
        }
        for (label, url) in [("Zed", "https://z.example"), ("Alpha", "https://a.example")] {
            sqlx::query("INSERT INTO BOOK_METADATA_LINK (BOOK_ID, LABEL, URL) VALUES (?, ?, ?)")
                .bind("book-1")
                .bind(label)
                .bind(url)
                .execute(&fixture.pool)
                .await
                .expect("book metadata link should be inserted");
        }

        let metadata = load_book_metadata(&fixture.pool, "book-1")
            .await
            .expect("book metadata should load")
            .expect("book metadata should exist");

        assert_eq!(
            metadata.authors,
            vec![
                BookMetadataAuthor {
                    name: "Zed".to_string(),
                    role: "artist".to_string(),
                },
                BookMetadataAuthor {
                    name: "Alpha".to_string(),
                    role: "writer".to_string(),
                },
                BookMetadataAuthor {
                    name: "Zed".to_string(),
                    role: "artist".to_string(),
                },
            ]
        );
        assert_eq!(metadata.tags, vec!["Omega", "Beta"]);
        assert_eq!(
            metadata.links,
            vec![
                BookMetadataLink {
                    label: "Zed".to_string(),
                    url: "https://z.example".to_string(),
                },
                BookMetadataLink {
                    label: "Alpha".to_string(),
                    url: "https://a.example".to_string(),
                },
            ]
        );
        fixture.close().await;
    }
}
