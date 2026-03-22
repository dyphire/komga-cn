use std::cell::RefCell;

use komga_application::discovery::{
    BookDetailQuery, BookReadlistsQuery, BookSiblingQuery, DiscoveryQueryRepository,
    NativeBooksLatestQuery, NativeBooksListQuery, NativeReadListBooksQuery, NativeSeriesListQuery,
    SeriesCollectionsQuery, SeriesDetailQuery,
};
use komga_domain::discovery::{
    BookDetailReadModel, BookReadModel, BookResourceReadModel, CollectionReadModel, DiscoveryError,
    DiscoveryQueryContext, LibraryReadModel, PageEnvelope, ReadListReadModel,
    ReadProgressReadModel, SeriesDetailReadModel, SeriesReadModel, SeriesResourceReadModel,
};
use rusqlite::{Connection, params, params_from_iter, types::Value as SqlValue};

use super::adapter::SqliteDiscoveryAdapter;
use super::filters::{
    append_clause, append_exists_series_filter, append_in_clause, append_string_set_filter,
    append_u16_set_filter, apply_restrictions, effective_library_ids, query_filters,
};

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
            query.unpaged,
            book_ordering_from_sorts(&query.sort),
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

    fn list_readlist_books(
        &self,
        context: &DiscoveryQueryContext,
        query: NativeReadListBooksQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        list_readlist_books(&self.connection, context, &query.readlist_id)
    }

    fn resolve_series_resource(
        &self,
        series_id: &str,
    ) -> Result<Option<SeriesResourceReadModel>, DiscoveryError> {
        let connection = self.connection.borrow();
        let mut stmt = connection
            .prepare(
                "SELECT s.id, s.library_id, s.age_rating, COALESCE(GROUP_CONCAT(DISTINCT sl.label), '') \
                 FROM series s \
                 LEFT JOIN series_labels sl ON sl.series_id = s.id \
                 WHERE s.id = ? \
                 GROUP BY s.id, s.library_id, s.age_rating",
            )
            .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;

        let mut rows = stmt
            .query_map(params![series_id], |row| {
                Ok(SeriesResourceReadModel {
                    id: row.get(0)?,
                    library_id: row.get(1)?,
                    age_rating: row.get(2)?,
                    labels: parse_labels(&row.get::<_, String>(3)?),
                })
            })
            .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;

        match rows.next() {
            Some(row) => row
                .map(Some)
                .map_err(|err| DiscoveryError::Persistence(err.to_string())),
            None => Ok(None),
        }
    }

    fn get_series_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesDetailQuery,
    ) -> Result<Option<SeriesDetailReadModel>, DiscoveryError> {
        let allowed = effective_library_ids(context, None);
        if allowed.as_ref().is_some_and(Vec::is_empty) {
            return Ok(None);
        }

        let mut where_clause = String::new();
        let mut where_params = Vec::<SqlValue>::new();
        append_clause("s.id = ?", &mut where_clause);
        where_params.push(SqlValue::Text(query.series_id.clone()));
        if let Some(allowed_ids) = allowed.as_ref() {
            let placeholders = vec!["?"; allowed_ids.len()].join(",");
            append_clause(
                &format!("s.library_id IN ({placeholders})"),
                &mut where_clause,
            );
            where_params.extend(allowed_ids.iter().cloned().map(SqlValue::Text));
        }
        if let Some(restrictions) = context.restrictions.as_ref() {
            let mut restriction_clauses = Vec::<String>::new();
            let mut restriction_params = Vec::<SqlValue>::new();
            apply_restrictions(
                "s",
                restrictions,
                &mut restriction_clauses,
                &mut restriction_params,
            );
            for clause in restriction_clauses {
                append_clause(&clause, &mut where_clause);
            }
            where_params.extend(restriction_params);
        }

        let sql = format!(
            "SELECT \
                s.id, s.library_id, s.title, s.url, s.created, s.last_modified, s.file_last_modified, \
                s.status, s.publisher, s.age_rating, s.language, s.deleted, s.oneshot, \
                COALESCE(GROUP_CONCAT(DISTINCT sl.label), ''), \
                COALESCE(GROUP_CONCAT(DISTINCT sg.genre), ''), \
                COALESCE(GROUP_CONCAT(DISTINCT st.tag), ''), \
                COALESCE((SELECT COUNT(*) FROM books b WHERE b.series_id = s.id), 0), \
                COALESCE((SELECT COUNT(*) FROM books b WHERE b.series_id = s.id AND LOWER(b.read_status) = 'read'), 0), \
                COALESCE((SELECT COUNT(*) FROM books b WHERE b.series_id = s.id AND LOWER(b.read_status) = 'unread'), 0), \
                COALESCE((SELECT COUNT(*) FROM books b WHERE b.series_id = s.id AND LOWER(b.read_status) = 'in_progress'), 0), \
                COALESCE((SELECT MIN(b.metadata_release_date) FROM books b WHERE b.series_id = s.id), NULL) \
             FROM series s \
             LEFT JOIN series_labels sl ON sl.series_id = s.id \
             LEFT JOIN series_genres sg ON sg.series_id = s.id \
             LEFT JOIN series_tags st ON st.series_id = s.id \
             {} \
             GROUP BY s.id, s.library_id, s.title, s.url, s.created, s.last_modified, s.file_last_modified, \
                s.status, s.publisher, s.age_rating, s.language, s.deleted, s.oneshot",
            where_clause
        );

        let connection = self.connection.borrow();
        let mut stmt = connection
            .prepare(&sql)
            .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;
        let mut rows = stmt
            .query_map(params_from_iter(where_params), |row| {
                Ok(SeriesDetailReadModel {
                    id: row.get(0)?,
                    library_id: row.get(1)?,
                    title: row.get(2)?,
                    url: row.get(3)?,
                    created: row.get(4)?,
                    last_modified: row.get(5)?,
                    file_last_modified: row.get(6)?,
                    status: row.get(7)?,
                    publisher: row.get(8)?,
                    age_rating: row.get(9)?,
                    language: row.get(10)?,
                    deleted: row.get(11)?,
                    oneshot: row.get(12)?,
                    sharing_labels: parse_labels(&row.get::<_, String>(13)?),
                    genres: parse_labels(&row.get::<_, String>(14)?),
                    tags: parse_labels(&row.get::<_, String>(15)?),
                    books_count: row.get::<_, i64>(16)? as u32,
                    books_read_count: row.get::<_, i64>(17)? as u32,
                    books_unread_count: row.get::<_, i64>(18)? as u32,
                    books_in_progress_count: row.get::<_, i64>(19)? as u32,
                    books_metadata_release_date: row.get(20)?,
                    summary: String::new(),
                    reading_direction: String::new(),
                    total_book_count: None,
                    alternate_titles: vec![],
                    metadata_created: row.get(4)?,
                    metadata_last_modified: row.get(5)?,
                    books_metadata_authors: vec![],
                    books_metadata_tags: vec![],
                    books_metadata_summary: String::new(),
                    books_metadata_summary_number: String::new(),
                    books_metadata_created: row.get(4)?,
                    books_metadata_last_modified: row.get(5)?,
                })
            })
            .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;

        match rows.next() {
            Some(row) => row
                .map(Some)
                .map_err(|err| DiscoveryError::Persistence(err.to_string())),
            None => Ok(None),
        }
    }

    fn resolve_book_resource(
        &self,
        book_id: &str,
    ) -> Result<Option<BookResourceReadModel>, DiscoveryError> {
        let connection = self.connection.borrow();
        let mut stmt = connection
            .prepare(
                "SELECT b.id, b.library_id, s.age_rating, COALESCE(GROUP_CONCAT(DISTINCT sl.label), '') \
                 FROM books b \
                 JOIN series s ON s.id = b.series_id \
                 LEFT JOIN series_labels sl ON sl.series_id = s.id \
                 WHERE b.id = ? \
                 GROUP BY b.id, b.library_id, s.age_rating",
            )
            .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;

        let mut rows = stmt
            .query_map(params![book_id], |row| {
                Ok(BookResourceReadModel {
                    id: row.get(0)?,
                    library_id: row.get(1)?,
                    age_rating: row.get(2)?,
                    labels: parse_labels(&row.get::<_, String>(3)?),
                })
            })
            .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;

        match rows.next() {
            Some(row) => row
                .map(Some)
                .map_err(|err| DiscoveryError::Persistence(err.to_string())),
            None => Ok(None),
        }
    }

    fn get_book_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: BookDetailQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        fetch_book_detail(&self.connection, context, query.book_id)
    }

    fn get_book_sibling_previous(
        &self,
        context: &DiscoveryQueryContext,
        query: BookSiblingQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        get_book_sibling(&self.connection, context, query.book_id, false)
    }

    fn get_book_sibling_next(
        &self,
        context: &DiscoveryQueryContext,
        query: BookSiblingQuery,
    ) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
        get_book_sibling(&self.connection, context, query.book_id, true)
    }

    fn list_book_readlists(
        &self,
        context: &DiscoveryQueryContext,
        query: BookReadlistsQuery,
    ) -> Result<Vec<ReadListReadModel>, DiscoveryError> {
        list_book_readlists(&self.connection, context, &query.book_id)
    }

    fn list_series_collections(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesCollectionsQuery,
    ) -> Result<Vec<CollectionReadModel>, DiscoveryError> {
        let allowed = effective_library_ids(context, None);
        if allowed.as_ref().is_some_and(Vec::is_empty) {
            return Ok(vec![]);
        }

        let mut visibility_where = String::new();
        let mut visibility_params = Vec::<SqlValue>::new();
        if let Some(allowed_ids) = allowed.as_ref() {
            let placeholders = vec!["?"; allowed_ids.len()].join(",");
            append_clause(
                &format!("s.library_id IN ({placeholders})"),
                &mut visibility_where,
            );
            visibility_params.extend(allowed_ids.iter().cloned().map(SqlValue::Text));
        }
        if let Some(restrictions) = context.restrictions.as_ref() {
            let mut restriction_clauses = Vec::<String>::new();
            let mut restriction_params = Vec::<SqlValue>::new();
            apply_restrictions(
                "s",
                restrictions,
                &mut restriction_clauses,
                &mut restriction_params,
            );
            for clause in restriction_clauses {
                append_clause(&clause, &mut visibility_where);
            }
            visibility_params.extend(restriction_params);
        }

        let mut candidate_where = visibility_where.clone();
        append_clause("cs_target.series_id = ?", &mut candidate_where);
        let mut candidate_params = visibility_params.clone();
        candidate_params.push(SqlValue::Text(query.series_id.clone()));

        let candidate_sql = format!(
            "SELECT DISTINCT c.id, c.name, c.ordered, c.created_date, c.last_modified_date \
             FROM collections c \
             JOIN collection_series cs_target ON cs_target.collection_id = c.id \
             JOIN series s ON s.id = cs_target.series_id{} \
             ORDER BY c.name COLLATE NOCASE ASC",
            candidate_where
        );

        let connection = self.connection.borrow();
        let mut stmt = connection
            .prepare(&candidate_sql)
            .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;
        let candidates = stmt
            .query_map(params_from_iter(candidate_params), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;

        let mut collections = vec![];
        for candidate in candidates {
            let (id, name, ordered, created_date, last_modified_date) =
                candidate.map_err(|err| DiscoveryError::Persistence(err.to_string()))?;

            let mut visible_where = visibility_where.clone();
            append_clause("cs.collection_id = ?", &mut visible_where);
            let mut visible_params = visibility_params.clone();
            visible_params.push(SqlValue::Text(id.clone()));

            let visible_sql = format!(
                "SELECT cs.series_id \
                 FROM collection_series cs \
                 JOIN series s ON s.id = cs.series_id{} \
                 ORDER BY cs.position ASC",
                visible_where
            );
            let mut visible_stmt = connection
                .prepare(&visible_sql)
                .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;
            let visible_rows = visible_stmt
                .query_map(params_from_iter(visible_params), |row| {
                    row.get::<_, String>(0)
                })
                .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;
            let mut visible_series_ids = vec![];
            for row in visible_rows {
                visible_series_ids
                    .push(row.map_err(|err| DiscoveryError::Persistence(err.to_string()))?);
            }

            if visible_series_ids.is_empty() {
                continue;
            }

            let total_count: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM collection_series WHERE collection_id = ?",
                    params![id.clone()],
                    |row| row.get(0),
                )
                .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;

            collections.push(CollectionReadModel {
                id,
                name,
                ordered,
                series_ids: visible_series_ids.clone(),
                created_date,
                last_modified_date,
                filtered: (visible_series_ids.len() as i64) < total_count,
            });
        }

        Ok(collections)
    }
}

fn get_book_sibling(
    connection: &RefCell<Connection>,
    context: &DiscoveryQueryContext,
    book_id: String,
    next: bool,
) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
    let connection_ref = connection.borrow();

    let anchor = connection_ref
        .query_row(
            "SELECT series_id, number_sort FROM books WHERE id = ?",
            params![book_id.clone()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .ok();

    let Some((series_id, number_sort)) = anchor else {
        return Ok(None);
    };

    let allowed = effective_library_ids(context, None);
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(None);
    }

    let filters = query_filters(
        "b.library_id",
        allowed.as_ref(),
        None,
        None,
        context.restrictions.as_ref(),
        "s",
    );
    let mut where_clause = filters.where_clause;
    let mut params = filters.params;
    append_clause("b.series_id = ?", &mut where_clause);
    params.push(SqlValue::Text(series_id));

    if next {
        append_clause("b.number_sort > ?", &mut where_clause);
    } else {
        append_clause("b.number_sort < ?", &mut where_clause);
    }
    params.push(SqlValue::Integer(number_sort));

    let sql = format!(
        "SELECT b.id \
         FROM books b \
         JOIN series s ON s.id = b.series_id{} \
         ORDER BY b.number_sort {}, b.title COLLATE NOCASE {} \
         LIMIT 1",
        where_clause,
        if next { "ASC" } else { "DESC" },
        if next { "ASC" } else { "DESC" }
    );

    let sibling_id = connection_ref
        .query_row(&sql, params_from_iter(params), |row| {
            row.get::<_, String>(0)
        })
        .ok();
    drop(connection_ref);

    let Some(sibling_id) = sibling_id else {
        return Ok(None);
    };

    fetch_book_detail(connection, context, sibling_id)
}

pub(super) fn get_readlist_book_sibling(
    connection: &RefCell<Connection>,
    context: &DiscoveryQueryContext,
    readlist_id: &str,
    book_id: &str,
    next: bool,
) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
    let page = list_readlist_books(connection, context, readlist_id)?;
    let visible_book_ids = page
        .content
        .iter()
        .map(|it| it.id.as_str())
        .collect::<Vec<_>>();

    let Some(current_index) = visible_book_ids.iter().position(|id| *id == book_id) else {
        return Ok(None);
    };

    let sibling_id = if next {
        visible_book_ids.get(current_index + 1)
    } else if current_index == 0 {
        None
    } else {
        visible_book_ids.get(current_index - 1)
    };

    let Some(sibling_id) = sibling_id else {
        return Ok(None);
    };

    fetch_book_detail(connection, context, (*sibling_id).to_string())
}

fn fetch_book_detail(
    connection: &RefCell<Connection>,
    context: &DiscoveryQueryContext,
    book_id: String,
) -> Result<Option<BookDetailReadModel>, DiscoveryError> {
    let allowed = effective_library_ids(context, None);
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(None);
    }

    let mut where_clause = String::new();
    let mut where_params = Vec::<SqlValue>::new();
    append_clause("b.id = ?", &mut where_clause);
    where_params.push(SqlValue::Text(book_id));
    if let Some(allowed_ids) = allowed.as_ref() {
        let placeholders = vec!["?"; allowed_ids.len()].join(",");
        append_clause(
            &format!("b.library_id IN ({placeholders})"),
            &mut where_clause,
        );
        where_params.extend(allowed_ids.iter().cloned().map(SqlValue::Text));
    }
    if let Some(restrictions) = context.restrictions.as_ref() {
        let mut restriction_clauses = Vec::<String>::new();
        let mut restriction_params = Vec::<SqlValue>::new();
        apply_restrictions(
            "s",
            restrictions,
            &mut restriction_clauses,
            &mut restriction_params,
        );
        for clause in restriction_clauses {
            append_clause(&clause, &mut where_clause);
        }
        where_params.extend(restriction_params);
    }

    let user_id = context.user_id.clone().unwrap_or_default();

    let sql = format!(
        "SELECT \
            b.id, b.series_id, b.library_id, b.title, b.url, b.number_sort, \
            b.created, b.last_modified, b.file_last_modified, b.size_bytes, \
            b.media_status, b.media_type, b.media_pages_count, b.metadata_release_date, \
            b.deleted, b.oneshot, s.title, \
            COALESCE(GROUP_CONCAT(DISTINCT ba.author), ''), \
            COALESCE(GROUP_CONCAT(DISTINCT bt.tag), ''), \
            rp.page, rp.completed, rp.read_date, rp.created, rp.last_modified, rp.device_id, rp.device_name \
         FROM books b \
         JOIN series s ON s.id = b.series_id \
         LEFT JOIN book_authors ba ON ba.book_id = b.id \
         LEFT JOIN book_tags bt ON bt.book_id = b.id \
         LEFT JOIN read_progress rp ON rp.book_id = b.id AND rp.user_id = ? \
         {} \
         GROUP BY \
            b.id, b.series_id, b.library_id, b.title, b.url, b.number_sort, \
            b.created, b.last_modified, b.file_last_modified, b.size_bytes, \
            b.media_status, b.media_type, b.media_pages_count, b.metadata_release_date, \
            b.deleted, b.oneshot, s.title, \
            rp.page, rp.completed, rp.read_date, rp.created, rp.last_modified, rp.device_id, rp.device_name",
        where_clause
    );

    let mut sql_params = vec![SqlValue::Text(user_id)];
    sql_params.extend(where_params);

    let connection = connection.borrow();
    let mut stmt = connection
        .prepare(&sql)
        .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;
    let mut rows = stmt
        .query_map(params_from_iter(sql_params), |row| {
            let page = row.get::<_, Option<u32>>(19)?;
            let completed = row.get::<_, Option<bool>>(20)?;
            let read_date = row.get::<_, Option<String>>(21)?;
            let created = row.get::<_, Option<String>>(22)?;
            let last_modified = row.get::<_, Option<String>>(23)?;
            let device_id = row.get::<_, Option<String>>(24)?;
            let device_name = row.get::<_, Option<String>>(25)?;
            let read_progress = match (
                page,
                completed,
                read_date,
                created,
                last_modified,
                device_id,
                device_name,
            ) {
                (
                    Some(page),
                    Some(completed),
                    Some(read_date),
                    Some(created),
                    Some(last_modified),
                    Some(device_id),
                    Some(device_name),
                ) => Some(ReadProgressReadModel {
                    page,
                    completed,
                    read_date,
                    created,
                    last_modified,
                    device_id,
                    device_name,
                }),
                _ => None,
            };

            Ok(BookDetailReadModel {
                id: row.get(0)?,
                series_id: row.get(1)?,
                library_id: row.get(2)?,
                name: row.get(3)?,
                url: row.get(4)?,
                number: row.get(5)?,
                created: row.get(6)?,
                last_modified: row.get(7)?,
                file_last_modified: row.get(8)?,
                size_bytes: row.get(9)?,
                media_status: row.get(10)?,
                media_type: row.get(11)?,
                media_pages_count: row.get(12)?,
                metadata_release_date: row.get(13)?,
                deleted: row.get(14)?,
                oneshot: row.get(15)?,
                series_title: row.get(16)?,
                metadata_authors: parse_labels(&row.get::<_, String>(17)?),
                metadata_tags: parse_labels(&row.get::<_, String>(18)?),
                read_progress,
                media_comment: String::new(),
                metadata_title: row.get(3)?,
                metadata_summary: String::new(),
                metadata_number: row.get::<_, i32>(5)?.to_string(),
                metadata_number_sort: row.get::<_, i32>(5)? as f64,
                metadata_isbn: String::new(),
                metadata_created: row.get(6)?,
                metadata_last_modified: row.get(7)?,
                file_hash: String::new(),
            })
        })
        .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;

    match rows.next() {
        Some(row) => row
            .map(Some)
            .map_err(|err| DiscoveryError::Persistence(err.to_string())),
        None => Ok(None),
    }
}

fn list_book_readlists(
    connection: &RefCell<Connection>,
    context: &DiscoveryQueryContext,
    book_id: &str,
) -> Result<Vec<ReadListReadModel>, DiscoveryError> {
    let allowed = effective_library_ids(context, None);
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(vec![]);
    }

    let connection = connection.borrow();
    let mut stmt = connection
        .prepare(
            "SELECT DISTINCT rl.id, rl.name, rl.summary, rl.ordered, rl.created_date, rl.last_modified_date \
             FROM readlists rl \
             JOIN readlist_books rlb ON rlb.readlist_id = rl.id \
             WHERE rlb.book_id = ? \
             ORDER BY rl.name COLLATE NOCASE ASC",
        )
        .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;
    let candidates = stmt
        .query_map(params![book_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;

    let mut readlists = vec![];
    for candidate in candidates {
        let (id, name, summary, ordered, created_date, last_modified_date) =
            candidate.map_err(|err| DiscoveryError::Persistence(err.to_string()))?;

        let visible_book_ids =
            visible_readlist_book_ids(&connection, context, &id, allowed.as_ref())?;
        if visible_book_ids.is_empty() {
            continue;
        }

        let total_count = connection
            .query_row(
                "SELECT COUNT(*) FROM readlist_books WHERE readlist_id = ?",
                params![id.clone()],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;

        readlists.push(ReadListReadModel {
            id,
            name,
            summary,
            ordered,
            book_ids: visible_book_ids.clone(),
            created_date,
            last_modified_date,
            filtered: (visible_book_ids.len() as i64) < total_count,
        });
    }

    Ok(readlists)
}

fn visible_readlist_book_ids(
    connection: &Connection,
    context: &DiscoveryQueryContext,
    readlist_id: &str,
    allowed_library_ids: Option<&Vec<String>>,
) -> Result<Vec<String>, DiscoveryError> {
    let filters = query_filters(
        "b.library_id",
        allowed_library_ids,
        None,
        None,
        context.restrictions.as_ref(),
        "s",
    );

    let mut where_clause = filters.where_clause;
    let mut params = filters.params;
    append_clause("rlb.readlist_id = ?", &mut where_clause);
    params.push(SqlValue::Text(readlist_id.to_string()));

    let sql = format!(
        "SELECT rlb.book_id \
         FROM readlist_books rlb \
         JOIN books b ON b.id = rlb.book_id \
         JOIN series s ON s.id = b.series_id{} \
         ORDER BY rlb.position ASC",
        where_clause
    );

    let mut stmt = connection
        .prepare(&sql)
        .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;
    let rows = stmt
        .query_map(params_from_iter(params), |row| row.get::<_, String>(0))
        .map_err(|err| DiscoveryError::Persistence(err.to_string()))?;

    let mut book_ids = vec![];
    for row in rows {
        book_ids.push(row.map_err(|err| DiscoveryError::Persistence(err.to_string()))?);
    }
    Ok(book_ids)
}

fn list_readlist_books(
    connection: &RefCell<Connection>,
    context: &DiscoveryQueryContext,
    readlist_id: &str,
) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
    let allowed = effective_library_ids(context, None);
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(PageEnvelope::from_slice(vec![], 0, 1, 0));
    }

    let connection = connection.borrow();

    let ordered = connection
        .query_row(
            "SELECT ordered FROM readlists WHERE id = ?",
            params![readlist_id],
            |row| row.get::<_, bool>(0),
        )
        .ok();
    let Some(ordered) = ordered else {
        return Ok(PageEnvelope::from_slice(vec![], 0, 1, 0));
    };

    let filters = query_filters(
        "b.library_id",
        allowed.as_ref(),
        None,
        None,
        context.restrictions.as_ref(),
        "s",
    );
    let mut where_clause = filters.where_clause;
    let mut params = filters.params;
    append_clause("rlb.readlist_id = ?", &mut where_clause);
    params.push(SqlValue::Text(readlist_id.to_string()));

    let count_sql = format!(
        "SELECT COUNT(DISTINCT b.id) \
         FROM readlist_books rlb \
         JOIN books b ON b.id = rlb.book_id \
         JOIN series s ON s.id = b.series_id{}",
        where_clause
    );

    let select_sql = format!(
        "SELECT \
            b.id, b.series_id, b.library_id, b.title, b.url, b.created, b.last_modified, b.file_last_modified, \
            b.size_bytes, b.media_status, b.media_type, b.media_pages_count, b.metadata_release_date, b.deleted, b.oneshot, \
            s.title, COALESCE(GROUP_CONCAT(DISTINCT sl.label), ''), MIN(rlb.position) \
         FROM readlist_books rlb \
         JOIN books b ON b.id = rlb.book_id \
         JOIN series s ON s.id = b.series_id \
         LEFT JOIN series_labels sl ON sl.series_id = s.id{} \
         GROUP BY b.id, b.series_id, b.library_id, b.title, b.url, b.created, b.last_modified, b.file_last_modified, \
             b.size_bytes, b.media_status, b.media_type, b.media_pages_count, b.metadata_release_date, b.deleted, b.oneshot, s.title \
         ORDER BY {}",
        where_clause,
        readlist_book_order_sql(ordered),
    );

    let total_elements = connection
        .query_row(&count_sql, params_from_iter(params.clone()), |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|err| DiscoveryError::Persistence(err.to_string()))?
        as usize;

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
        0,
        total_elements.max(1),
        total_elements,
    ))
}

fn readlist_book_order_sql(ordered: bool) -> &'static str {
    if ordered {
        "MIN(rlb.position) ASC"
    } else {
        "b.metadata_release_date ASC, b.title COLLATE NOCASE ASC"
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BookOrdering {
    TitleAsc,
    CreatedDateDesc,
    MetadataReleaseDateDesc,
    NumberSortAsc,
    LastModifiedDesc,
}

fn book_ordering_from_sorts(sorts: &[String]) -> BookOrdering {
    let Some(sort) = sorts.first() else {
        return BookOrdering::NumberSortAsc;
    };

    let property = sort.split(',').next().unwrap_or(sort).trim();
    match property {
        "metadata.title" => BookOrdering::TitleAsc,
        "createdDate" => BookOrdering::CreatedDateDesc,
        "lastModifiedDate" => BookOrdering::LastModifiedDesc,
        "metadata.releaseDate" => BookOrdering::MetadataReleaseDateDesc,
        "metadata.numberSort" => BookOrdering::NumberSortAsc,
        _ => BookOrdering::NumberSortAsc,
    }
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

    let scoped_to_series = requested_series_ids.is_some_and(|series_ids| !series_ids.is_empty());

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
        append_clause(
            &format!("b.series_id IN ({placeholders})"),
            &mut where_clause,
        );
        params.extend(series_ids.iter().cloned().map(SqlValue::Text));
    }

    if let Some(value) = deleted {
        append_clause("b.deleted = ?", &mut where_clause);
        params.push(SqlValue::Integer(i64::from(value)));
    }

    if let Some(value) = oneshot {
        append_clause("s.oneshot = ?", &mut where_clause);
        params.push(SqlValue::Integer(i64::from(value)));
    } else if !scoped_to_series {
        append_clause("s.oneshot = ?", &mut where_clause);
        params.push(SqlValue::Integer(0));
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
        .query_row(&count_sql, params_from_iter(params.clone()), |row| {
            row.get::<_, i64>(0)
        })
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
        BookOrdering::CreatedDateDesc => "b.created DESC, b.title COLLATE NOCASE ASC",
        BookOrdering::MetadataReleaseDateDesc => {
            "b.metadata_release_date DESC, b.title COLLATE NOCASE ASC"
        }
        BookOrdering::NumberSortAsc => "b.number_sort ASC, b.title COLLATE NOCASE ASC",
        BookOrdering::LastModifiedDesc => "b.last_modified DESC, b.title COLLATE NOCASE ASC",
    }
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
