use std::path::PathBuf;

use sqlx::SqlitePool;

use crate::persistence::sqlite::{connect_test_pool, schema};

pub(crate) struct BootstrappedBookFixture {
    pub(crate) db_path: PathBuf,
    pub(crate) pool: SqlitePool,
}

pub(crate) struct MediaPageFixture<'a> {
    pub(crate) book_id: &'a str,
    pub(crate) page_number: i64,
    pub(crate) file_name: &'a str,
    pub(crate) media_type: &'a str,
    pub(crate) width: i64,
    pub(crate) height: i64,
    pub(crate) file_size: Option<i64>,
}

impl BootstrappedBookFixture {
    pub(crate) async fn open(case_id: &str) -> Self {
        let db_path = temp_db_path(case_id);
        let pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open");
        schema::bootstrap_pool(&pool)
            .await
            .expect("temporary sqlite db should bootstrap main schema");
        Self { db_path, pool }
    }

    pub(crate) async fn close(self) {
        self.pool.close().await;
        let _ = std::fs::remove_file(self.db_path);
    }

    pub(crate) async fn insert_library_series(&self) {
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
            .bind("library-1")
            .bind("Library 1")
            .bind("/library")
            .execute(&self.pool)
            .await
            .expect("library row should be inserted");
        sqlx::query(
            r#"
            INSERT INTO SERIES (
                ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID
            )
            VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?)
            "#,
        )
        .bind("series-1")
        .bind(0_i64)
        .bind("Series 1")
        .bind("series")
        .bind("library-1")
        .execute(&self.pool)
        .await
        .expect("series row should be inserted");
    }

    pub(crate) async fn insert_series_metadata(&self) {
        sqlx::query(
            r#"
            INSERT INTO SERIES_METADATA (
                SERIES_ID, STATUS, TITLE, TITLE_SORT
            )
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind("series-1")
        .bind("ONGOING")
        .bind("Series 1")
        .bind("Series 1")
        .execute(&self.pool)
        .await
        .expect("series metadata row should be inserted");
    }

    pub(crate) async fn insert_book(&self, book_id: &str) {
        sqlx::query(
            r#"
            INSERT INTO BOOK (
                ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID
            )
            VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(book_id)
        .bind(0_i64)
        .bind(format!("{book_id}.cbz"))
        .bind(format!("series/{book_id}.cbz"))
        .bind("series-1")
        .bind(0_i64)
        .bind(1_i64)
        .bind("library-1")
        .execute(&self.pool)
        .await
        .expect("book row should be inserted");
    }

    pub(crate) async fn insert_book_metadata(&self, book_id: &str) {
        sqlx::query(
            r#"
            INSERT INTO BOOK_METADATA (
                BOOK_ID, TITLE, NUMBER, NUMBER_SORT
            )
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(book_id)
        .bind(format!("{book_id} title"))
        .bind("1")
        .bind(1.0_f64)
        .execute(&self.pool)
        .await
        .expect("book metadata row should be inserted");
    }

    pub(crate) async fn insert_media(&self, book_id: &str, media_type: Option<&str>) {
        self.insert_media_with_page_count(book_id, media_type, "READY", 1)
            .await;
    }

    pub(crate) async fn insert_media_with_page_count(
        &self,
        book_id: &str,
        media_type: Option<&str>,
        status: &str,
        page_count: i64,
    ) {
        sqlx::query(
            r#"
            INSERT INTO MEDIA (
                BOOK_ID, MEDIA_TYPE, STATUS, PAGE_COUNT
            )
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(book_id)
        .bind(media_type)
        .bind(status)
        .bind(page_count)
        .execute(&self.pool)
        .await
        .expect("media row should be inserted");
    }

    pub(crate) async fn insert_media_page(
        &self,
        book_id: &str,
        page_number: i64,
        file_name: &str,
        media_type: &str,
        file_size: Option<i64>,
    ) {
        self.insert_media_page_with_dimensions(MediaPageFixture {
            book_id,
            page_number,
            file_name,
            media_type,
            width: 640,
            height: 480,
            file_size,
        })
        .await;
    }

    pub(crate) async fn insert_media_page_with_dimensions(&self, page: MediaPageFixture<'_>) {
        sqlx::query(
            r#"
            INSERT INTO MEDIA_PAGE (
                BOOK_ID, NUMBER, FILE_NAME, MEDIA_TYPE, width, height, FILE_SIZE
            )
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(page.book_id)
        .bind(page.page_number)
        .bind(page.file_name)
        .bind(page.media_type)
        .bind(page.width)
        .bind(page.height)
        .bind(page.file_size)
        .execute(&self.pool)
        .await
        .expect("media page row should be inserted");
    }
}

fn temp_db_path(case_id: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "komga-rust-discovery-book-{case_id}-{nanos}.sqlite"
    ))
}
