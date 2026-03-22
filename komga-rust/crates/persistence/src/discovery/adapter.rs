use std::cell::RefCell;

use komga_domain::discovery::{BookDetailReadModel, DiscoveryError, DiscoveryQueryContext};
use rusqlite::{Connection, params};

use super::queries::get_readlist_book_sibling;
use super::rows::{BookRow, CollectionRow, LibraryRow, ReadListRow, ReadProgressRow, SeriesRow};

pub struct SqliteDiscoveryAdapter {
    pub(super) connection: RefCell<Connection>,
}

impl Default for SqliteDiscoveryAdapter {
    fn default() -> Self {
        let connection =
            Connection::open_in_memory().expect("sqlite in-memory open should succeed");
        bootstrap_schema(&connection).expect("sqlite schema bootstrap should succeed");
        Self {
            connection: RefCell::new(connection),
        }
    }
}

impl SqliteDiscoveryAdapter {
    pub fn insert_library(&mut self, row: LibraryRow) {
        let connection = self.connection.borrow_mut();
        connection
            .execute(
                "INSERT INTO libraries (id, name, root) VALUES (?1, ?2, ?3)",
                params![row.id, row.name, row.root],
            )
            .expect("library insert should succeed");
    }

    pub fn insert_series(&mut self, row: SeriesRow) {
        let connection = self.connection.borrow_mut();
        connection
            .execute(
                "INSERT INTO series (id, library_id, title, age_rating, language, publisher, release_date, status, complete, read_status, deleted, oneshot, created, last_modified, file_last_modified, url) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    row.id,
                    row.library_id,
                    row.title,
                    row.age_rating,
                    row.language,
                    row.publisher,
                    row.release_date,
                    row.status,
                    row.complete,
                    row.read_status,
                    row.deleted,
                    row.oneshot,
                    row.created,
                    row.last_modified,
                    row.file_last_modified,
                    row.url,
                ],
            )
            .expect("series insert should succeed");

        for label in row.labels {
            connection
                .execute(
                    "INSERT INTO series_labels (series_id, label) VALUES (?1, ?2)",
                    params![row.id, label],
                )
                .expect("series label insert should succeed");
        }

        for genre in row.genres {
            connection
                .execute(
                    "INSERT INTO series_genres (series_id, genre) VALUES (?1, ?2)",
                    params![row.id, genre],
                )
                .expect("series genre insert should succeed");
        }

        for tag in row.tags {
            connection
                .execute(
                    "INSERT INTO series_tags (series_id, tag) VALUES (?1, ?2)",
                    params![row.id, tag],
                )
                .expect("series tag insert should succeed");
        }

        for author in row.authors {
            connection
                .execute(
                    "INSERT INTO series_authors (series_id, author) VALUES (?1, ?2)",
                    params![row.id, author],
                )
                .expect("series author insert should succeed");
        }
    }

    pub fn insert_collection(&mut self, row: CollectionRow) {
        let connection = self.connection.borrow_mut();
        connection
            .execute(
                "INSERT INTO collections (id, name, ordered, created_date, last_modified_date) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    row.id,
                    row.name,
                    row.ordered,
                    row.created_date,
                    row.last_modified_date,
                ],
            )
            .expect("collection insert should succeed");

        for (index, series_id) in row.series_ids.iter().enumerate() {
            connection
                .execute(
                    "INSERT INTO collection_series (collection_id, series_id, position) VALUES (?1, ?2, ?3)",
                    params![row.id, series_id, index as i64],
                )
                .expect("collection series insert should succeed");
        }
    }

    pub fn insert_read_list(&mut self, row: ReadListRow) {
        let connection = self.connection.borrow_mut();
        connection
            .execute(
                "INSERT INTO readlists (id, name, summary, ordered, created_date, last_modified_date) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    row.id,
                    row.name,
                    row.summary,
                    row.ordered,
                    row.created_date,
                    row.last_modified_date,
                ],
            )
            .expect("readlist insert should succeed");

        for (index, book_id) in row.book_ids.iter().enumerate() {
            connection
                .execute(
                    "INSERT INTO readlist_books (readlist_id, book_id, position) VALUES (?1, ?2, ?3)",
                    params![row.id, book_id, index as i64],
                )
                .expect("readlist book insert should succeed");
        }
    }

    pub fn insert_book(&mut self, row: BookRow) {
        let connection = self.connection.borrow_mut();
        connection
            .execute(
                "INSERT INTO books (id, series_id, library_id, title, url, created, last_modified, file_last_modified, size_bytes, media_status, media_profile, media_type, media_pages_count, metadata_release_date, number_sort, read_status, deleted, oneshot) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
                params![
                    row.id,
                    row.series_id,
                    row.library_id,
                    row.title,
                    row.url,
                    row.created,
                    row.last_modified,
                    row.file_last_modified,
                    row.size_bytes,
                    row.media_status,
                    row.media_profile,
                    row.media_type,
                    row.media_pages_count,
                    row.metadata_release_date,
                    row.number_sort,
                    row.read_status,
                    row.deleted,
                    row.oneshot,
                ],
            )
            .expect("book insert should succeed");

        for tag in row.tags {
            connection
                .execute(
                    "INSERT INTO book_tags (book_id, tag) VALUES (?1, ?2)",
                    params![row.id, tag],
                )
                .expect("book tag insert should succeed");
        }

        for author in row.authors {
            connection
                .execute(
                    "INSERT INTO book_authors (book_id, author) VALUES (?1, ?2)",
                    params![row.id, author],
                )
                .expect("book author insert should succeed");
        }
    }

    pub fn insert_read_progress(&mut self, row: ReadProgressRow) {
        let connection = self.connection.borrow_mut();
        connection
            .execute(
                "INSERT INTO read_progress (book_id, user_id, page, completed, read_date, created, last_modified, device_id, device_name) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    row.book_id,
                    row.user_id,
                    row.page,
                    row.completed,
                    row.read_date,
                    row.created,
                    row.last_modified,
                    row.device_id,
                    row.device_name,
                ],
            )
            .expect("read progress insert should succeed");
    }

    pub fn get_readlist_book_sibling_previous(
        &self,
        context: &DiscoveryQueryContext,
        readlist_id: &str,
        book_id: &str,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        get_readlist_book_sibling(&self.connection, context, readlist_id, book_id, false)
    }

    pub fn get_readlist_book_sibling_next(
        &self,
        context: &DiscoveryQueryContext,
        readlist_id: &str,
        book_id: &str,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        get_readlist_book_sibling(&self.connection, context, readlist_id, book_id, true)
    }
}

fn bootstrap_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS libraries (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          root TEXT NOT NULL DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS series (
          id TEXT PRIMARY KEY,
          library_id TEXT NOT NULL,
          title TEXT NOT NULL,
          age_rating INTEGER NULL,
          language TEXT NOT NULL DEFAULT '',
          publisher TEXT NOT NULL DEFAULT '',
          release_date TEXT NULL,
          status TEXT NOT NULL DEFAULT '',
          complete INTEGER NOT NULL DEFAULT 0,
          read_status TEXT NOT NULL DEFAULT '',
          deleted INTEGER NOT NULL DEFAULT 0,
          oneshot INTEGER NOT NULL DEFAULT 0,
          created TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z',
          last_modified TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z',
          file_last_modified TEXT NOT NULL DEFAULT '2024-01-02T03:04:05Z',
          url TEXT NOT NULL DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS collections (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          ordered INTEGER NOT NULL DEFAULT 0,
          created_date TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z',
          last_modified_date TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z'
        );

        CREATE TABLE IF NOT EXISTS collection_series (
          collection_id TEXT NOT NULL,
          series_id TEXT NOT NULL,
          position INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS series_labels (
          series_id TEXT NOT NULL,
          label TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS series_genres (
          series_id TEXT NOT NULL,
          genre TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS series_tags (
          series_id TEXT NOT NULL,
          tag TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS series_authors (
          series_id TEXT NOT NULL,
          author TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS books (
          id TEXT PRIMARY KEY,
          series_id TEXT NOT NULL,
          library_id TEXT NOT NULL,
          title TEXT NOT NULL,
          url TEXT NOT NULL DEFAULT '',
          created TEXT NOT NULL DEFAULT '2024-01-02T03:04:05Z',
          last_modified TEXT NOT NULL DEFAULT '2024-01-02T03:04:05Z',
          file_last_modified TEXT NOT NULL DEFAULT '2024-01-02T08:04:05Z',
          size_bytes INTEGER NOT NULL DEFAULT 0,
          media_status TEXT NOT NULL DEFAULT 'UNKNOWN',
          media_profile TEXT NOT NULL DEFAULT '',
          media_type TEXT NOT NULL DEFAULT '',
          media_pages_count INTEGER NOT NULL DEFAULT 0,
          metadata_release_date TEXT NULL,
          number_sort INTEGER NOT NULL DEFAULT 1,
          read_status TEXT NOT NULL DEFAULT '',
          deleted INTEGER NOT NULL DEFAULT 0,
          oneshot INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS book_tags (
          book_id TEXT NOT NULL,
          tag TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS book_authors (
          book_id TEXT NOT NULL,
          author TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS read_progress (
          book_id TEXT NOT NULL,
          user_id TEXT NOT NULL,
          page INTEGER NOT NULL,
          completed INTEGER NOT NULL DEFAULT 0,
          read_date TEXT NOT NULL,
          created TEXT NOT NULL,
          last_modified TEXT NOT NULL,
          device_id TEXT NOT NULL DEFAULT '',
          device_name TEXT NOT NULL DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS readlists (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          summary TEXT NOT NULL DEFAULT '',
          ordered INTEGER NOT NULL DEFAULT 1,
          created_date TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z',
          last_modified_date TEXT NOT NULL DEFAULT '2026-01-01T00:00:00Z'
        );

        CREATE TABLE IF NOT EXISTS readlist_books (
          readlist_id TEXT NOT NULL,
          book_id TEXT NOT NULL,
          position INTEGER NOT NULL DEFAULT 0
        );
        ",
    )
}
