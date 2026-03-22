use std::cell::RefCell;

use komga_application::discovery::{
    DiscoveryQueryRepository, NativeBooksLatestQuery, NativeBooksListQuery, NativeSeriesListQuery,
};
use komga_domain::discovery::{
    AgeRestrictionKind, BookReadModel, DiscoveryError, DiscoveryQueryContext, LibraryReadModel,
    PageEnvelope, QueryRestrictions, SeriesReadModel,
};
use rusqlite::{params, params_from_iter, types::Value as SqlValue, Connection};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryRow {
    pub id: String,
    pub name: String,
    pub root: String,
}

impl LibraryRow {
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            root: String::new(),
        }
    }

    pub fn with_root(mut self, root: &str) -> Self {
        self.root = root.to_string();
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesRow {
    pub id: String,
    pub library_id: String,
    pub title: String,
    pub labels: Vec<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub language: String,
    pub publisher: String,
    pub age_rating: Option<u16>,
    pub release_date: Option<String>,
    pub status: String,
    pub complete: bool,
    pub read_status: String,
    pub authors: Vec<String>,
    pub deleted: bool,
    pub oneshot: bool,
}

impl SeriesRow {
    pub fn new(id: &str, library_id: &str, title: &str) -> Self {
        Self {
            id: id.to_string(),
            library_id: library_id.to_string(),
            title: title.to_string(),
            labels: vec![],
            genres: vec![],
            tags: vec![],
            language: String::new(),
            publisher: String::new(),
            age_rating: None,
            release_date: None,
            status: String::new(),
            complete: false,
            read_status: String::new(),
            authors: vec![],
            deleted: false,
            oneshot: false,
        }
    }

    pub fn with_labels<I, S>(mut self, labels: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.labels = labels
            .into_iter()
            .map(|it| it.as_ref().to_ascii_lowercase())
            .collect();
        self
    }

    pub fn with_age_rating(mut self, age_rating: u16) -> Self {
        self.age_rating = Some(age_rating);
        self
    }

    pub fn with_genres<I, S>(mut self, genres: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.genres = genres
            .into_iter()
            .map(|it| it.as_ref().to_ascii_lowercase())
            .collect();
        self
    }

    pub fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.tags = tags
            .into_iter()
            .map(|it| it.as_ref().to_ascii_lowercase())
            .collect();
        self
    }

    pub fn with_language(mut self, language: &str) -> Self {
        self.language = language.to_ascii_lowercase();
        self
    }

    pub fn with_publisher(mut self, publisher: &str) -> Self {
        self.publisher = publisher.to_ascii_lowercase();
        self
    }

    pub fn with_release_date(mut self, release_date: &str) -> Self {
        self.release_date = Some(release_date.to_string());
        self
    }

    pub fn with_status(mut self, status: &str) -> Self {
        self.status = status.to_ascii_lowercase();
        self
    }

    pub fn with_complete(mut self, complete: bool) -> Self {
        self.complete = complete;
        self
    }

    pub fn with_read_status(mut self, read_status: &str) -> Self {
        self.read_status = read_status.to_ascii_lowercase();
        self
    }

    pub fn with_authors<I, S>(mut self, authors: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.authors = authors
            .into_iter()
            .map(|it| it.as_ref().to_ascii_lowercase())
            .collect();
        self
    }

    pub fn with_deleted(mut self, deleted: bool) -> Self {
        self.deleted = deleted;
        self
    }

    pub fn with_oneshot(mut self, oneshot: bool) -> Self {
        self.oneshot = oneshot;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookRow {
    pub id: String,
    pub series_id: String,
    pub library_id: String,
    pub title: String,
    pub url: String,
    pub created: String,
    pub last_modified: String,
    pub file_last_modified: String,
    pub size_bytes: u64,
    pub media_status: String,
    pub media_profile: String,
    pub media_type: String,
    pub media_pages_count: u32,
    pub metadata_release_date: Option<String>,
    pub deleted: bool,
    pub oneshot: bool,
    pub tags: Vec<String>,
    pub read_status: String,
    pub authors: Vec<String>,
}

impl BookRow {
    pub fn new(id: &str, series_id: &str, library_id: &str, title: &str) -> Self {
        Self {
            id: id.to_string(),
            series_id: series_id.to_string(),
            library_id: library_id.to_string(),
            title: title.to_string(),
            url: format!("/library/{library_id}/{title}"),
            created: "2024-01-02T03:04:05Z".to_string(),
            last_modified: "2024-01-02T03:04:05Z".to_string(),
            file_last_modified: "2024-01-02T08:04:05Z".to_string(),
            size_bytes: 0,
            media_status: "UNKNOWN".to_string(),
            media_profile: String::new(),
            media_type: String::new(),
            media_pages_count: 0,
            metadata_release_date: None,
            deleted: false,
            oneshot: false,
            tags: vec![],
            read_status: String::new(),
            authors: vec![],
        }
    }

    pub fn with_url(mut self, url: &str) -> Self {
        self.url = url.to_string();
        self
    }

    pub fn with_last_modified(mut self, last_modified: &str) -> Self {
        self.last_modified = last_modified.to_string();
        self
    }

    pub fn with_media(mut self, status: &str, media_type: &str, pages_count: u32) -> Self {
        self.media_status = status.to_string();
        self.media_type = media_type.to_string();
        self.media_pages_count = pages_count;
        self
    }

    pub fn with_media_profile(mut self, media_profile: &str) -> Self {
        self.media_profile = media_profile.to_ascii_lowercase();
        self
    }

    pub fn with_release_date(mut self, release_date: &str) -> Self {
        self.metadata_release_date = Some(release_date.to_string());
        self
    }

    pub fn with_deleted(mut self, deleted: bool) -> Self {
        self.deleted = deleted;
        self
    }

    pub fn with_oneshot(mut self, oneshot: bool) -> Self {
        self.oneshot = oneshot;
        self
    }

    pub fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.tags = tags
            .into_iter()
            .map(|it| it.as_ref().to_ascii_lowercase())
            .collect();
        self
    }

    pub fn with_read_status(mut self, read_status: &str) -> Self {
        self.read_status = read_status.to_ascii_lowercase();
        self
    }

    pub fn with_authors<I, S>(mut self, authors: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.authors = authors
            .into_iter()
            .map(|it| it.as_ref().to_ascii_lowercase())
            .collect();
        self
    }
}

pub struct SqliteDiscoveryAdapter {
    connection: RefCell<Connection>,
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
                "INSERT INTO series (id, library_id, title, age_rating, language, publisher, release_date, status, complete, read_status, deleted, oneshot) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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

    pub fn insert_book(&mut self, row: BookRow) {
        let connection = self.connection.borrow_mut();
        connection
            .execute(
                "INSERT INTO books (id, series_id, library_id, title, url, created, last_modified, file_last_modified, size_bytes, media_status, media_profile, media_type, media_pages_count, metadata_release_date, read_status, deleted, oneshot) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
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
}

impl DiscoveryQueryRepository for SqliteDiscoveryAdapter {
    fn list_libraries(
        &self,
        context: &DiscoveryQueryContext,
    ) -> Result<Vec<LibraryReadModel>, DiscoveryError> {
        let allowed = effective_library_ids(context, None);
        if allowed.as_ref().is_some_and(Vec::is_empty) {
            return Ok(vec![]);
        }

        let mut sql = "SELECT id, name, root FROM libraries".to_string();
        let mut params = Vec::<SqlValue>::new();
        if let Some(allowed_ids) = allowed {
            append_in_clause("id", &allowed_ids, &mut sql, &mut params);
        }
        sql.push_str(" ORDER BY name COLLATE NOCASE ASC");

        let connection = self.connection.borrow();
        let mut stmt = connection
            .prepare(&sql)
            .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;
        let rows = stmt
            .query_map(params_from_iter(params), |row| {
                Ok(LibraryReadModel {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    root: row.get(2)?,
                })
            })
            .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;

        let mut result = vec![];
        for row in rows {
            result.push(row.map_err(|err| DiscoveryError::Persistence(err.to_string()))?);
        }
        Ok(result)
    }

    fn list_series(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeSeriesListQuery,
    ) -> Result<PageEnvelope<SeriesReadModel>, DiscoveryError> {
        let allowed = effective_library_ids(context, query.library_ids.as_deref());
        if allowed.as_ref().is_some_and(Vec::is_empty) {
            return Ok(PageEnvelope::from_slice(
                vec![],
                query.page,
                query.size.max(1),
                0,
            ));
        }

        let filters = query_filters(
            "s.library_id",
            allowed.as_ref(),
            query.search.as_deref(),
            Some("s.title"),
            context.restrictions.as_ref(),
            "s",
        );

        let mut where_clause = filters.where_clause;
        let mut where_params = filters.params;

        if let Some(deleted) = query.deleted {
            append_clause("s.deleted = ?", &mut where_clause);
            where_params.push(SqlValue::Integer(i64::from(deleted)));
        }

        if let Some(oneshot) = query.oneshot {
            append_clause("s.oneshot = ?", &mut where_clause);
            where_params.push(SqlValue::Integer(i64::from(oneshot)));
        }

        append_string_set_filter(
            "s.read_status",
            query.read_statuses.as_deref(),
            &mut where_clause,
            &mut where_params,
            true,
        );
        append_exists_series_filter(
            "series_genres",
            "genre",
            query.genres.as_deref(),
            &mut where_clause,
            &mut where_params,
        );
        append_exists_series_filter(
            "series_tags",
            "tag",
            query.tags.as_deref(),
            &mut where_clause,
            &mut where_params,
        );
        append_string_set_filter(
            "s.language",
            query.languages.as_deref(),
            &mut where_clause,
            &mut where_params,
            true,
        );
        append_string_set_filter(
            "s.publisher",
            query.publishers.as_deref(),
            &mut where_clause,
            &mut where_params,
            true,
        );
        append_u16_set_filter(
            "s.age_rating",
            query.age_ratings.as_deref(),
            &mut where_clause,
            &mut where_params,
        );
        append_string_set_filter(
            "s.release_date",
            query.release_dates.as_deref(),
            &mut where_clause,
            &mut where_params,
            false,
        );
        append_exists_series_filter(
            "series_labels",
            "label",
            query.sharing_labels.as_deref(),
            &mut where_clause,
            &mut where_params,
        );
        append_string_set_filter(
            "s.status",
            query.series_statuses.as_deref(),
            &mut where_clause,
            &mut where_params,
            true,
        );
        if let Some(complete) = query.complete {
            append_clause("s.complete = ?", &mut where_clause);
            where_params.push(SqlValue::Integer(i64::from(complete)));
        }
        append_exists_series_filter(
            "series_authors",
            "author",
            query.authors.as_deref(),
            &mut where_clause,
            &mut where_params,
        );

        let count_sql = format!("SELECT COUNT(DISTINCT s.id) FROM series s{}", where_clause);
        let select_sql = format!(
            "SELECT s.id, s.library_id, s.title, COALESCE(GROUP_CONCAT(DISTINCT sl.label), '') \
             FROM series s \
             LEFT JOIN series_labels sl ON sl.series_id = s.id{} \
             GROUP BY s.id, s.library_id, s.title \
             ORDER BY s.title COLLATE NOCASE ASC \
             LIMIT ? OFFSET ?",
            where_clause
        );

        let connection = self.connection.borrow();
        let total_elements = connection
            .query_row(&count_sql, params_from_iter(where_params.clone()), |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|err| DiscoveryError::Persistence(err.to_string()))?
            as usize;

        let safe_size = query.size.max(1);
        let offset = query.page.saturating_mul(safe_size);
        let mut params = where_params;
        params.push(SqlValue::Integer(safe_size as i64));
        params.push(SqlValue::Integer(offset as i64));

        let mut stmt = connection
            .prepare(&select_sql)
            .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;
        let rows = stmt
            .query_map(params_from_iter(params), |row| {
                Ok(SeriesReadModel {
                    id: row.get(0)?,
                    library_id: row.get(1)?,
                    title: row.get(2)?,
                    labels: parse_labels(&row.get::<_, String>(3)?),
                })
            })
            .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;

        let mut content = vec![];
        for row in rows {
            content.push(row.map_err(|err| DiscoveryError::Persistence(err.to_string()))?);
        }

        Ok(PageEnvelope::from_slice(
            content,
            query.page,
            safe_size,
            total_elements,
        ))
    }

    fn list_books(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeBooksListQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        list_books_common(
            &self.connection,
            context,
            query.page,
            query.size,
            query.library_ids.as_deref(),
            query.series_ids.as_deref(),
            query.deleted,
            query.oneshot,
            query.tags.as_deref(),
            query.read_statuses.as_deref(),
            query.media_profiles.as_deref(),
            query.media_statuses.as_deref(),
            query.authors.as_deref(),
            query.release_dates.as_deref(),
            query.search.as_deref(),
            false,
            BookOrdering::TitleAsc,
        )
    }

    fn list_books_latest(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeBooksLatestQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        list_books_common(
            &self.connection,
            context,
            query.page,
            query.size,
            query.library_ids.as_deref(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            query.unpaged,
            BookOrdering::LastModifiedDesc,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BookOrdering {
    TitleAsc,
    LastModifiedDesc,
}

fn list_books_common(
    connection: &RefCell<Connection>,
    context: &DiscoveryQueryContext,
    page: usize,
    size: usize,
    requested_library_ids: Option<&[String]>,
    requested_series_ids: Option<&[String]>,
    deleted: Option<bool>,
    oneshot: Option<bool>,
    tags: Option<&[String]>,
    read_statuses: Option<&[String]>,
    media_profiles: Option<&[String]>,
    media_statuses: Option<&[String]>,
    authors: Option<&[String]>,
    release_dates: Option<&[String]>,
    search: Option<&str>,
    unpaged: bool,
    ordering: BookOrdering,
) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
    let allowed = effective_library_ids(context, requested_library_ids);
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(PageEnvelope::from_slice(vec![], page, size.max(1), 0));
    }

    let filters = query_filters(
        "b.library_id",
        allowed.as_ref(),
        search,
        Some("b.title"),
        context.restrictions.as_ref(),
        "s",
    );

    let mut where_clause = filters.where_clause;
    let mut params = filters.params;

    if let Some(series_ids) = requested_series_ids
        && !series_ids.is_empty()
    {
        let placeholders = vec!["?"; series_ids.len()].join(",");
        append_clause(&format!("b.series_id IN ({placeholders})"), &mut where_clause);
        params.extend(series_ids.iter().cloned().map(SqlValue::Text));
    }

    if let Some(value) = deleted {
        append_clause("b.deleted = ?", &mut where_clause);
        params.push(SqlValue::Integer(i64::from(value)));
    }

    if let Some(value) = oneshot {
        append_clause("b.oneshot = ?", &mut where_clause);
        params.push(SqlValue::Integer(i64::from(value)));
    }

    if let Some(tag_values) = tags
        && !tag_values.is_empty()
    {
        let placeholders = vec!["?"; tag_values.len()].join(",");
        append_clause(
            &format!(
                "EXISTS (SELECT 1 FROM book_tags bt WHERE bt.book_id = b.id AND LOWER(bt.tag) IN ({placeholders}))"
            ),
            &mut where_clause,
        );
        params.extend(
            tag_values
                .iter()
                .map(|value| SqlValue::Text(value.to_ascii_lowercase())),
        );
    }

    append_string_set_filter(
        "b.read_status",
        read_statuses,
        &mut where_clause,
        &mut params,
        true,
    );
    append_string_set_filter(
        "b.media_profile",
        media_profiles,
        &mut where_clause,
        &mut params,
        true,
    );
    append_string_set_filter(
        "b.media_status",
        media_statuses,
        &mut where_clause,
        &mut params,
        true,
    );
    if let Some(author_values) = authors
        && !author_values.is_empty()
    {
        let placeholders = vec!["?"; author_values.len()].join(",");
        append_clause(
            &format!(
                "EXISTS (SELECT 1 FROM book_authors ba WHERE ba.book_id = b.id AND LOWER(ba.author) IN ({placeholders}))"
            ),
            &mut where_clause,
        );
        params.extend(
            author_values
                .iter()
                .map(|value| SqlValue::Text(value.to_ascii_lowercase())),
        );
    }
    append_string_set_filter(
        "b.metadata_release_date",
        release_dates,
        &mut where_clause,
        &mut params,
        false,
    );

    let count_sql = format!(
        "SELECT COUNT(DISTINCT b.id) \
         FROM books b \
         JOIN series s ON s.id = b.series_id{}",
        where_clause
    );
    let select_sql_base = format!(
        "SELECT b.id, b.series_id, b.library_id, b.title, b.url, b.created, b.last_modified, b.file_last_modified, b.size_bytes, b.media_status, b.media_type, b.media_pages_count, b.metadata_release_date, b.deleted, b.oneshot, s.title, COALESCE(GROUP_CONCAT(DISTINCT sl.label), '') \
         FROM books b \
         JOIN series s ON s.id = b.series_id \
         LEFT JOIN series_labels sl ON sl.series_id = s.id{} \
         GROUP BY b.id, b.series_id, b.library_id, b.title, b.url, b.created, b.last_modified, b.file_last_modified, b.size_bytes, b.media_status, b.media_type, b.media_pages_count, b.metadata_release_date, b.deleted, b.oneshot, s.title \
         ORDER BY {}",
        where_clause,
        book_order_sql(ordering),
    );

    let connection = connection.borrow();
    let total_elements = connection
        .query_row(
            &count_sql,
            params_from_iter(params.clone()),
            |row| row.get::<_, i64>(0),
        )
        .map_err(|err| DiscoveryError::Persistence(err.to_string()))?
        as usize;

    let (select_sql, params, envelope_page, envelope_size) = if unpaged {
        let safe_size = total_elements.max(1);
        (select_sql_base, params, 0, safe_size)
    } else {
        let safe_size = size.max(1);
        let offset = page.saturating_mul(safe_size);
        let mut params = params;
        params.push(SqlValue::Integer(safe_size as i64));
        params.push(SqlValue::Integer(offset as i64));

        (
            format!("{select_sql_base} LIMIT ? OFFSET ?"),
            params,
            page,
            safe_size,
        )
    };

    let mut stmt = connection
        .prepare(&select_sql)
        .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;
    let rows = stmt
        .query_map(params_from_iter(params), |row| {
            Ok(BookReadModel {
                id: row.get(0)?,
                series_id: row.get(1)?,
                library_id: row.get(2)?,
                title: row.get(3)?,
                url: row.get(4)?,
                created: row.get(5)?,
                last_modified: row.get(6)?,
                file_last_modified: row.get(7)?,
                size_bytes: row.get(8)?,
                media_status: row.get(9)?,
                media_type: row.get(10)?,
                media_pages_count: row.get(11)?,
                metadata_release_date: row.get(12)?,
                deleted: row.get(13)?,
                oneshot: row.get(14)?,
                series_title: row.get(15)?,
                labels: parse_labels(&row.get::<_, String>(16)?),
            })
        })
        .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;

    let mut content = vec![];
    for row in rows {
        content.push(row.map_err(|err| DiscoveryError::Persistence(err.to_string()))?);
    }

    Ok(PageEnvelope::from_slice(
        content,
        envelope_page,
        envelope_size,
        total_elements,
    ))
}

fn book_order_sql(ordering: BookOrdering) -> &'static str {
    match ordering {
        BookOrdering::TitleAsc => "b.title COLLATE NOCASE ASC",
        BookOrdering::LastModifiedDesc => "b.last_modified DESC, b.title COLLATE NOCASE ASC",
    }
}

struct SqlFilters {
    where_clause: String,
    params: Vec<SqlValue>,
}

fn query_filters(
    library_column: &str,
    allowed_library_ids: Option<&Vec<String>>,
    search: Option<&str>,
    search_column: Option<&str>,
    restrictions: Option<&QueryRestrictions>,
    restriction_series_alias: &str,
) -> SqlFilters {
    let mut clauses = Vec::<String>::new();
    let mut params = Vec::<SqlValue>::new();

    if let Some(allowed) = allowed_library_ids {
        let placeholders = vec!["?"; allowed.len()].join(",");
        clauses.push(format!("{library_column} IN ({placeholders})"));
        params.extend(allowed.iter().cloned().map(SqlValue::Text));
    }

    if let (Some(term), Some(column)) = (search, search_column) {
        clauses.push(format!("LOWER({column}) LIKE ?"));
        params.push(SqlValue::Text(format!("%{}%", term.to_ascii_lowercase())));
    }

    if let Some(restrictions) = restrictions {
        apply_restrictions(
            restriction_series_alias,
            restrictions,
            &mut clauses,
            &mut params,
        );
    }

    let where_clause = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };

    SqlFilters {
        where_clause,
        params,
    }
}

fn apply_restrictions(
    series_alias: &str,
    restrictions: &QueryRestrictions,
    clauses: &mut Vec<String>,
    params: &mut Vec<SqlValue>,
) {
    if !restrictions.labels_exclude.is_empty() {
        let placeholders = vec!["?"; restrictions.labels_exclude.len()].join(",");
        clauses.push(format!(
            "NOT EXISTS (SELECT 1 FROM series_labels ex WHERE ex.series_id = {series_alias}.id AND LOWER(ex.label) IN ({placeholders}))"
        ));
        params.extend(
            restrictions
                .labels_exclude
                .iter()
                .map(|label| SqlValue::Text(label.to_ascii_lowercase())),
        );
    }

    if let (Some(AgeRestrictionKind::Exclude), Some(max_age)) =
        (restrictions.age_restriction, restrictions.age)
    {
        clauses.push(format!(
            "({series_alias}.age_rating IS NULL OR {series_alias}.age_rating < ?)"
        ));
        params.push(SqlValue::Integer(max_age as i64));
    }

    if !restrictions.labels_allow.is_empty() {
        let placeholders = vec!["?"; restrictions.labels_allow.len()].join(",");
        clauses.push(format!(
            "EXISTS (SELECT 1 FROM series_labels al WHERE al.series_id = {series_alias}.id AND LOWER(al.label) IN ({placeholders}))"
        ));
        params.extend(
            restrictions
                .labels_allow
                .iter()
                .map(|label| SqlValue::Text(label.to_ascii_lowercase())),
        );
    }
}

fn append_in_clause(column: &str, values: &[String], sql: &mut String, params: &mut Vec<SqlValue>) {
    let placeholders = vec!["?"; values.len()].join(",");
    let prefix = if sql.contains(" WHERE ") {
        " AND "
    } else {
        " WHERE "
    };
    sql.push_str(prefix);
    sql.push_str(&format!("{column} IN ({placeholders})"));
    params.extend(values.iter().cloned().map(SqlValue::Text));
}

fn append_clause(clause: &str, where_clause: &mut String) {
    if where_clause.contains(" WHERE ") {
        where_clause.push_str(" AND ");
        where_clause.push_str(clause);
    } else {
        where_clause.push_str(" WHERE ");
        where_clause.push_str(clause);
    }
}

fn append_string_set_filter(
    column: &str,
    values: Option<&[String]>,
    where_clause: &mut String,
    params: &mut Vec<SqlValue>,
    lowercase: bool,
) {
    if let Some(values) = values
        && !values.is_empty()
    {
        let placeholders = vec!["?"; values.len()].join(",");
        let lhs = if lowercase {
            format!("LOWER({column})")
        } else {
            column.to_string()
        };
        append_clause(&format!("{lhs} IN ({placeholders})"), where_clause);
        if lowercase {
            params.extend(
                values
                    .iter()
                    .map(|value| SqlValue::Text(value.to_ascii_lowercase())),
            );
        } else {
            params.extend(values.iter().cloned().map(SqlValue::Text));
        }
    }
}

fn append_u16_set_filter(
    column: &str,
    values: Option<&[u16]>,
    where_clause: &mut String,
    params: &mut Vec<SqlValue>,
) {
    if let Some(values) = values
        && !values.is_empty()
    {
        let placeholders = vec!["?"; values.len()].join(",");
        append_clause(&format!("{column} IN ({placeholders})"), where_clause);
        params.extend(
            values
                .iter()
                .map(|value| SqlValue::Integer(*value as i64)),
        );
    }
}

fn append_exists_series_filter(
    table: &str,
    value_column: &str,
    values: Option<&[String]>,
    where_clause: &mut String,
    params: &mut Vec<SqlValue>,
) {
    if let Some(values) = values
        && !values.is_empty()
    {
        let placeholders = vec!["?"; values.len()].join(",");
        append_clause(
            &format!(
                "EXISTS (SELECT 1 FROM {table} f WHERE f.series_id = s.id AND LOWER(f.{value_column}) IN ({placeholders}))"
            ),
            where_clause,
        );
        params.extend(
            values
                .iter()
                .map(|value| SqlValue::Text(value.to_ascii_lowercase())),
        );
    }
}

fn effective_library_ids(
    context: &DiscoveryQueryContext,
    requested_library_ids: Option<&[String]>,
) -> Option<Vec<String>> {
    match (&context.authorized_library_ids, requested_library_ids) {
        (Some(authorized), Some(requested)) => Some(intersection(authorized, requested)),
        (Some(authorized), None) => Some(authorized.clone()),
        (None, Some(requested)) => Some(requested.to_vec()),
        (None, None) => None,
    }
}

fn intersection(authorized: &[String], requested: &[String]) -> Vec<String> {
    requested
        .iter()
        .filter(|candidate| authorized.contains(*candidate))
        .cloned()
        .collect()
}

fn parse_labels(raw: &str) -> Vec<String> {
    if raw.trim().is_empty() {
        return vec![];
    }

    raw.split(',')
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>()
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
          oneshot INTEGER NOT NULL DEFAULT 0
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
        ",
    )
}
