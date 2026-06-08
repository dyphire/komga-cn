use std::collections::HashMap;
use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::runtime_sse::register_runtime_sse_event;
use icu::collator::{
    Collator,
    options::{CollatorOptions, Strength},
};
use icu::locale::locale;
use komga_domain::common_ids::LibraryId;
use komga_domain::discovery::{
    DiscoveryError, DiscoveryQueryContext, PageEnvelope, content_allowed_by_restrictions,
};
use serde_json::json;

use super::{
    BookMetadataAuthorReadModel, BookReadModel, DiscoveryPersistedReadlistRecord,
    PersistedBookResourceRecord, ReadListReadModel, ReadlistBookPort, ReadlistPort,
    ReadlistSearchPort,
};

const READLIST_SEARCH_CANDIDATE_LIMIT: usize = 1000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadListsQuery {
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
    pub library_ids: Option<Vec<String>>,
    pub search: Option<String>,
    pub sort: ReadListsSort,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadListsSort {
    NameAsc,
    NameDesc,
    CreatedDateAsc,
    CreatedDateDesc,
    LastModifiedDateAsc,
    LastModifiedDateDesc,
    SearchOrName,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadListBooksQuery {
    pub readlist_id: String,
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
    pub library_ids: Option<Vec<String>>,
    pub deleted: Option<bool>,
    pub tags: Option<Vec<String>>,
    pub read_statuses: Option<Vec<String>>,
    pub media_statuses: Option<Vec<String>>,
    pub authors: Option<Vec<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadListDetailQuery {
    pub readlist_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadlistMutationInput {
    pub name: String,
    pub summary: String,
    pub ordered: bool,
    pub book_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadlistCreateResult {
    pub readlist_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReadlistMutationError {
    DuplicateName,
    Persistence(String),
}

impl std::fmt::Display for ReadlistMutationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadlistMutationError::DuplicateName => write!(f, "Read list name already exists"),
            ReadlistMutationError::Persistence(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ReadlistMutationError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadListBooksOwnership {
    RuntimeOwned,
    DependencyOnly,
}

pub fn classify_readlist_books_query(
    query: &ReadListBooksQuery,
) -> Result<ReadListBooksOwnership, DiscoveryError> {
    if !query.unpaged {
        return Ok(ReadListBooksOwnership::RuntimeOwned);
    }

    Ok(ReadListBooksOwnership::DependencyOnly)
}

pub fn normalize_readlists_search(search: Option<String>) -> Option<String> {
    search.and_then(|value| (!value.trim().is_empty()).then_some(value))
}

pub fn parse_readlists_sort(value: &str) -> ReadListsSort {
    let mut parts = value.splitn(2, ',');
    let field = parts.next().unwrap_or_default().trim();
    let direction = parts.next().unwrap_or("asc").trim();

    if field.eq_ignore_ascii_case("name") {
        if direction.eq_ignore_ascii_case("desc") {
            ReadListsSort::NameDesc
        } else {
            ReadListsSort::NameAsc
        }
    } else if field.eq_ignore_ascii_case("createdDate") {
        if direction.eq_ignore_ascii_case("desc") {
            ReadListsSort::CreatedDateDesc
        } else {
            ReadListsSort::CreatedDateAsc
        }
    } else if field.eq_ignore_ascii_case("lastModifiedDate") {
        if direction.eq_ignore_ascii_case("desc") {
            ReadListsSort::LastModifiedDateDesc
        } else {
            ReadListsSort::LastModifiedDateAsc
        }
    } else {
        ReadListsSort::SearchOrName
    }
}

pub fn resolve_readlists_query(query: &str) -> ReadListsQuery {
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20);
    let library_ids = {
        let values = query_values(query, "library_id")
            .into_iter()
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        (!values.is_empty()).then_some(values)
    };
    let search_values = query_values(query, "search")
        .into_iter()
        .map(decode_query_component)
        .collect::<Vec<_>>();
    let search = normalize_readlists_search(match search_values.as_slice() {
        [] => None,
        [single] => Some(single.clone()),
        _ => Some(search_values.join(",")),
    });
    let sort = query_values(query, "sort")
        .into_iter()
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(parse_readlists_sort)
        .unwrap_or(ReadListsSort::SearchOrName);

    ReadListsQuery {
        page,
        size,
        unpaged: query_bool(query, "unpaged"),
        library_ids,
        search,
        sort,
    }
}

pub fn resolve_readlist_books_query(
    readlist_id: impl Into<String>,
    query: &str,
) -> ReadListBooksQuery {
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20);
    let library_ids = {
        let values = query_values(query, "library_id")
            .into_iter()
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        (!values.is_empty()).then_some(values)
    };

    ReadListBooksQuery {
        readlist_id: readlist_id.into(),
        page,
        size,
        unpaged: query_bool(query, "unpaged"),
        library_ids,
        deleted: query_value(query, "deleted").map(|value| value.eq_ignore_ascii_case("true")),
        tags: decoded_query_values(query, "tag"),
        read_statuses: decoded_query_values(query, "read_status"),
        media_statuses: decoded_query_values(query, "media_status"),
        authors: decoded_query_values(query, "author"),
    }
}

pub struct ReadlistListService<'a> {
    readlists: &'a dyn ReadlistPort,
    books: &'a dyn ReadlistBookPort,
    search: &'a dyn ReadlistSearchPort,
}

impl<'a> ReadlistListService<'a> {
    pub fn new(
        readlists: &'a dyn ReadlistPort,
        books: &'a dyn ReadlistBookPort,
        search: &'a dyn ReadlistSearchPort,
    ) -> Self {
        Self {
            readlists,
            books,
            search,
        }
    }

    pub async fn list_readlists(
        &self,
        requested_context: &DiscoveryQueryContext,
        visibility_context: &DiscoveryQueryContext,
        query: ReadListsQuery,
    ) -> Result<PageEnvelope<ReadListReadModel>, String> {
        let visibility = ReadlistVisibilityService::new(self.readlists, self.books);
        let requested_library_ids =
            library_ids_to_strings(requested_context.authorized_library_ids.as_ref());
        let mut content = visibility
            .load_readlists(requested_library_ids.as_deref())
            .await?;

        let search_ranks = match query.search.as_deref() {
            Some(search) => self.search_ranks(search).await?,
            None => None,
        };
        if let Some(search_ranks) = search_ranks.as_ref() {
            content.retain(|readlist| search_ranks.contains_key(readlist.id.as_str()));
        }

        let mut visible_content = Vec::with_capacity(content.len());
        for readlist in content {
            let Some(mut visible_readlist) = visibility
                .load_readlist_detail(&readlist.id, visibility_context)
                .await?
            else {
                continue;
            };

            if let Some(library_ids) = query.library_ids.as_ref() {
                let requested_library_query =
                    readlist_books_visibility_query(readlist.id.clone(), Some(library_ids.clone()));
                let Some(requested_library_books) = visibility
                    .visible_readlist_books(visibility_context, &requested_library_query)
                    .await?
                else {
                    continue;
                };

                if requested_library_books.is_empty() {
                    continue;
                }
            }

            let visibility_query = readlist_books_visibility_query(readlist.id.clone(), None);
            let Some(visible_books) = visibility
                .visible_readlist_books(visibility_context, &visibility_query)
                .await?
            else {
                continue;
            };

            let visible_book_ids = visible_books
                .into_iter()
                .map(|book| book.id)
                .collect::<Vec<_>>();
            if visible_book_ids.is_empty() {
                if visible_readlist.book_ids.is_empty() && !visible_readlist.filtered {
                    visible_content.push(visible_readlist);
                }
                continue;
            }

            visible_readlist.filtered =
                visible_readlist.filtered || visible_readlist.book_ids != visible_book_ids;
            visible_readlist.book_ids = visible_book_ids;
            visible_content.push(visible_readlist);
        }

        sort_readlists(&mut visible_content, query.sort, search_ranks.as_ref());
        Ok(paginate_readlists(visible_content, &query))
    }

    async fn search_ranks(&self, search: &str) -> Result<Option<HashMap<String, usize>>, String> {
        let search_groups = search
            .split(',')
            .map(str::trim)
            .filter(|group| !group.is_empty())
            .collect::<Vec<_>>();
        if search_groups.is_empty() {
            return Ok(None);
        }

        let mut next_rank = 0_usize;
        let mut ranks = HashMap::new();
        for search_group in search_groups {
            let ranked_hits = self
                .search
                .search_readlist_scored_ids(search_group, READLIST_SEARCH_CANDIDATE_LIMIT)
                .await?;
            for (_score, id) in ranked_hits {
                if let std::collections::hash_map::Entry::Vacant(entry) = ranks.entry(id) {
                    entry.insert(next_rank);
                    next_rank += 1;
                }
            }
        }

        Ok(Some(ranks))
    }
}

pub struct ReadlistMutationService<'a> {
    readlists: &'a dyn ReadlistPort,
}

pub struct ReadlistVisibilityService<'a> {
    readlists: &'a dyn ReadlistPort,
    books: &'a dyn ReadlistBookPort,
}

impl<'a> ReadlistVisibilityService<'a> {
    pub fn new(readlists: &'a dyn ReadlistPort, books: &'a dyn ReadlistBookPort) -> Self {
        Self { readlists, books }
    }

    pub async fn list_readlists(
        &self,
        library_ids: Option<&[String]>,
    ) -> Result<Vec<ReadListReadModel>, String> {
        self.load_readlists(library_ids).await
    }

    pub async fn readlist_detail(
        &self,
        context: &DiscoveryQueryContext,
        readlist_id: &str,
    ) -> Result<Option<ReadListReadModel>, String> {
        let Some(mut readlist) = self.load_readlist_detail(readlist_id, context).await? else {
            return Ok(None);
        };
        let query = readlist_books_visibility_query(readlist_id, None);
        let Some(visible_books) = self.visible_readlist_books(context, &query).await? else {
            return Ok(None);
        };
        let visible_book_ids = visible_books
            .into_iter()
            .map(|book| book.id)
            .collect::<Vec<_>>();
        if visible_book_ids.is_empty() {
            return Ok(None);
        }

        readlist.filtered = readlist.book_ids != visible_book_ids;
        readlist.book_ids = visible_book_ids;
        Ok(Some(readlist))
    }

    pub async fn list_readlist_books(
        &self,
        context: &DiscoveryQueryContext,
        query: ReadListBooksQuery,
    ) -> Result<Option<PageEnvelope<BookReadModel>>, String> {
        let Some(readlist) = self
            .load_readlist_detail(&query.readlist_id, context)
            .await?
        else {
            return Ok(None);
        };
        let Some(mut visible_books) = self.visible_readlist_books(context, &query).await? else {
            return Ok(None);
        };

        sort_readlist_books(&mut visible_books, readlist.ordered);
        Ok(Some(paginate_readlist_books(visible_books, &query)))
    }

    pub async fn visible_readlist_book_ids(
        &self,
        context: &DiscoveryQueryContext,
        readlist_id: &str,
    ) -> Result<Option<Vec<String>>, String> {
        let query = readlist_books_visibility_query(readlist_id, None);
        let Some(visible_books) = self.visible_readlist_books(context, &query).await? else {
            return Ok(None);
        };

        Ok(Some(
            visible_books.into_iter().map(|book| book.id).collect(),
        ))
    }

    pub async fn readlist_book_sibling(
        &self,
        context: &DiscoveryQueryContext,
        readlist_id: &str,
        book_id: &str,
        next: bool,
    ) -> Result<Option<BookReadModel>, String> {
        let query = readlist_books_visibility_query(readlist_id, None);
        let Some(readlist) = self.load_readlist_detail(readlist_id, context).await? else {
            return Ok(None);
        };
        let Some(mut visible_books) = self.visible_readlist_books(context, &query).await? else {
            return Ok(None);
        };

        sort_readlist_books(&mut visible_books, readlist.ordered);
        let Some(current_index) = visible_books.iter().position(|book| book.id == book_id) else {
            return Ok(None);
        };
        let sibling_index = if next {
            current_index + 1
        } else if current_index == 0 {
            return Ok(None);
        } else {
            current_index - 1
        };

        Ok(visible_books.get(sibling_index).cloned())
    }

    pub async fn readlists_for_book(
        &self,
        candidate_library_ids: Option<&[String]>,
        visibility_context: &DiscoveryQueryContext,
        book_id: &str,
    ) -> Result<Vec<ReadListReadModel>, String> {
        let mut readlists = self.load_readlists(candidate_library_ids).await?;
        readlists.retain(|readlist| readlist.book_ids.iter().any(|id| id == book_id));

        let mut visible_readlists = Vec::with_capacity(readlists.len());
        for mut readlist in readlists {
            let query = readlist_books_visibility_query(readlist.id.clone(), None);
            let Some(visible_books) = self
                .visible_readlist_books(visibility_context, &query)
                .await?
            else {
                continue;
            };
            let visible_book_ids = visible_books
                .into_iter()
                .map(|book| book.id)
                .collect::<Vec<_>>();
            if !visible_book_ids.iter().any(|id| id == book_id) {
                continue;
            }

            readlist.filtered = readlist.book_ids != visible_book_ids;
            readlist.book_ids = visible_book_ids;
            visible_readlists.push(readlist);
        }

        Ok(visible_readlists)
    }

    async fn load_readlists(
        &self,
        library_ids: Option<&[String]>,
    ) -> Result<Vec<ReadListReadModel>, String> {
        let rows = self.readlists.load_persisted_readlists().await?;

        let mut readlists = Vec::with_capacity(rows.len());
        for row in rows {
            let id = row.id.clone();
            let (book_ids, filtered) =
                load_readlist_book_ids(self.readlists, &id, library_ids).await?;
            if library_ids.is_some() && book_ids.is_empty() {
                continue;
            }

            readlists.push(readlist_from_record(row, book_ids, filtered));
        }

        Ok(readlists)
    }

    async fn load_readlist_detail(
        &self,
        readlist_id: &str,
        context: &DiscoveryQueryContext,
    ) -> Result<Option<ReadListReadModel>, String> {
        let Some(row) = self
            .readlists
            .load_persisted_readlist_detail(readlist_id)
            .await?
        else {
            return Ok(None);
        };

        let authorized_library_ids =
            library_ids_to_strings(context.authorized_library_ids.as_ref());
        let (book_ids, filtered) = load_readlist_book_ids(
            self.readlists,
            readlist_id,
            authorized_library_ids.as_deref(),
        )
        .await?;

        Ok(Some(readlist_from_record(row, book_ids, filtered)))
    }

    async fn visible_readlist_books(
        &self,
        context: &DiscoveryQueryContext,
        query: &ReadListBooksQuery,
    ) -> Result<Option<Vec<BookReadModel>>, String> {
        let Some(readlist) = self
            .load_readlist_detail(&query.readlist_id, context)
            .await?
        else {
            return Ok(None);
        };
        if context.authorized_library_ids.is_some() && readlist.book_ids.is_empty() {
            return Ok(None);
        }

        let authorized_library_ids =
            library_ids_to_strings(context.authorized_library_ids.as_ref());
        let user_id = context.user_id.as_ref().map(|user_id| user_id.as_str());
        let rows = self
            .readlists
            .load_persisted_readlist_book_rows(&query.readlist_id)
            .await?;
        let mut visible = Vec::new();

        for row in rows {
            if authorized_library_ids
                .as_ref()
                .is_some_and(|ids| !contains_id(ids, &row.library_id))
            {
                continue;
            }
            if query
                .library_ids
                .as_ref()
                .is_some_and(|ids| !contains_id(ids, &row.library_id))
            {
                continue;
            }

            let Some(resource) = self
                .books
                .load_persisted_book_resource(&row.book_id)
                .await?
            else {
                continue;
            };
            if !book_resource_allowed(context, &resource) {
                continue;
            }

            let Some(detail) = self
                .books
                .load_persisted_book_detail(&row.book_id, user_id)
                .await?
            else {
                continue;
            };

            let authors = self.books.load_persisted_book_authors(&row.book_id).await?;
            if !matches_readlist_book_filters(&detail, &authors, query) {
                continue;
            }

            visible.push(detail);
        }

        Ok(Some(visible))
    }
}

impl<'a> ReadlistMutationService<'a> {
    pub fn new(readlists: &'a dyn ReadlistPort) -> Self {
        Self { readlists }
    }

    pub async fn create_readlist(
        &self,
        input: ReadlistMutationInput,
    ) -> Result<ReadlistCreateResult, ReadlistMutationError> {
        self.ensure_unique_readlist_name(&input.name, None).await?;

        let readlist_id = generated_readlist_id();
        self.readlists
            .persist_readlist_create(
                &readlist_id,
                &input.name,
                &input.summary,
                input.ordered,
                &input.book_ids,
            )
            .await
            .map_err(ReadlistMutationError::Persistence)?;

        register_runtime_sse_event(
            "ReadListAdded",
            json!({
                "readListId": readlist_id,
                "bookIds": input.book_ids,
            }),
            false,
            None,
        );
        self.readlists
            .upsert_readlist_search_document(&readlist_id)
            .await
            .map_err(ReadlistMutationError::Persistence)?;

        Ok(ReadlistCreateResult { readlist_id })
    }

    pub async fn update_readlist(
        &self,
        readlist_id: &str,
        input: ReadlistMutationInput,
    ) -> Result<bool, ReadlistMutationError> {
        self.ensure_unique_readlist_name(&input.name, Some(readlist_id))
            .await?;

        let updated = self
            .readlists
            .persist_readlist_update(
                readlist_id,
                &input.name,
                &input.summary,
                input.ordered,
                &input.book_ids,
            )
            .await
            .map_err(ReadlistMutationError::Persistence)?;
        if !updated {
            return Ok(false);
        }

        register_runtime_sse_event(
            "ReadListChanged",
            json!({
                "readListId": readlist_id,
                "bookIds": input.book_ids,
            }),
            false,
            None,
        );
        self.readlists
            .upsert_readlist_search_document(readlist_id)
            .await
            .map_err(ReadlistMutationError::Persistence)?;

        Ok(true)
    }

    pub async fn delete_readlist(&self, readlist_id: &str) -> Result<bool, ReadlistMutationError> {
        let existing = self
            .load_readlist_for_mutation(readlist_id)
            .await
            .map_err(ReadlistMutationError::Persistence)?;
        let deleted = self
            .readlists
            .delete_persisted_readlist(readlist_id)
            .await
            .map_err(ReadlistMutationError::Persistence)?;
        if !deleted {
            return Ok(false);
        }

        if let Some(readlist) = existing {
            register_runtime_sse_event(
                "ReadListDeleted",
                json!({
                    "readListId": readlist_id,
                    "bookIds": readlist.book_ids,
                }),
                false,
                None,
            );
        }
        self.readlists
            .delete_readlist_search_document(readlist_id)
            .await
            .map_err(ReadlistMutationError::Persistence)?;

        Ok(true)
    }

    async fn ensure_unique_readlist_name(
        &self,
        name: &str,
        allowed_readlist_id: Option<&str>,
    ) -> Result<(), ReadlistMutationError> {
        let readlists = self
            .readlists
            .load_persisted_readlists()
            .await
            .map_err(ReadlistMutationError::Persistence)?;
        let duplicate = readlists.iter().any(|readlist| {
            allowed_readlist_id != Some(readlist.id.as_str())
                && readlist.name.eq_ignore_ascii_case(name)
        });
        if duplicate {
            return Err(ReadlistMutationError::DuplicateName);
        }

        Ok(())
    }

    async fn load_readlist_for_mutation(
        &self,
        readlist_id: &str,
    ) -> Result<Option<ReadListReadModel>, String> {
        let Some(row) = self
            .readlists
            .load_persisted_readlist_detail(readlist_id)
            .await?
        else {
            return Ok(None);
        };
        let (book_ids, filtered) =
            load_readlist_book_ids(self.readlists, readlist_id, None).await?;

        Ok(Some(readlist_from_record(row, book_ids, filtered)))
    }
}

fn readlist_from_record(
    row: DiscoveryPersistedReadlistRecord,
    book_ids: Vec<String>,
    filtered: bool,
) -> ReadListReadModel {
    ReadListReadModel {
        id: row.id,
        name: row.name,
        summary: row.summary,
        ordered: row.ordered,
        book_ids,
        created_date: row.created_date,
        last_modified_date: row.last_modified_date,
        filtered,
    }
}

async fn load_readlist_book_ids(
    readlists: &dyn ReadlistPort,
    readlist_id: &str,
    library_ids: Option<&[String]>,
) -> Result<(Vec<String>, bool), String> {
    let rows = readlists
        .load_persisted_readlist_book_rows(readlist_id)
        .await?;

    let total_count = rows.len();
    let book_ids = rows
        .into_iter()
        .filter(|row| library_ids.is_none_or(|ids| contains_id(ids, &row.library_id)))
        .map(|row| row.book_id)
        .collect::<Vec<_>>();

    Ok((book_ids.clone(), book_ids.len() < total_count))
}

fn readlist_books_visibility_query(
    readlist_id: impl Into<String>,
    library_ids: Option<Vec<String>>,
) -> ReadListBooksQuery {
    ReadListBooksQuery {
        readlist_id: readlist_id.into(),
        page: 0,
        size: 20,
        unpaged: false,
        library_ids,
        deleted: None,
        tags: None,
        read_statuses: None,
        media_statuses: None,
        authors: None,
    }
}

fn library_ids_to_strings(library_ids: Option<&Vec<LibraryId>>) -> Option<Vec<String>> {
    library_ids.map(|ids| ids.iter().map(|id| id.as_str().to_string()).collect())
}

fn contains_id(ids: &[String], id: &str) -> bool {
    ids.iter().any(|candidate| candidate == id)
}

fn book_resource_allowed(
    context: &DiscoveryQueryContext,
    resource: &PersistedBookResourceRecord,
) -> bool {
    if let Some(authorized_library_ids) =
        library_ids_to_strings(context.authorized_library_ids.as_ref())
        && !contains_id(&authorized_library_ids, &resource.library_id)
    {
        return false;
    }

    context.restrictions.as_ref().is_none_or(|restrictions| {
        content_allowed_by_restrictions(
            restrictions,
            resource.age_rating,
            &parse_csv_values(&resource.sharing_labels),
        )
    })
}

fn matches_readlist_book_filters(
    book: &BookReadModel,
    book_authors: &[BookMetadataAuthorReadModel],
    query: &ReadListBooksQuery,
) -> bool {
    if query.deleted.is_some_and(|deleted| deleted != book.deleted) {
        return false;
    }
    if query.tags.as_ref().is_some_and(|tags| {
        !tags.is_empty()
            && !tags.iter().any(|tag| {
                book.metadata_tags
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(tag))
            })
    }) {
        return false;
    }
    if query.media_statuses.as_ref().is_some_and(|statuses| {
        !statuses.is_empty()
            && !statuses
                .iter()
                .any(|status| book.media_status.eq_ignore_ascii_case(status))
    }) {
        return false;
    }
    if let Some(read_statuses) = query.read_statuses.as_ref()
        && !read_statuses.is_empty()
    {
        let read_status = persisted_read_status(book);
        if !read_statuses
            .iter()
            .any(|status| read_status.eq_ignore_ascii_case(status))
        {
            return false;
        }
    }
    if let Some(authors) = query.authors.as_ref()
        && !authors.is_empty()
    {
        let mut has_author_filters = false;
        let matches_author_filter = authors
            .iter()
            .filter_map(|author| parse_author_filter(author))
            .any(|(requested_name, requested_role)| {
                has_author_filters = true;
                book_authors.iter().any(|author| {
                    author.name.eq_ignore_ascii_case(&requested_name)
                        && author.role.eq_ignore_ascii_case(&requested_role)
                })
            });
        if has_author_filters && !matches_author_filter {
            return false;
        }
    }

    true
}

fn parse_author_filter(value: &str) -> Option<(String, String)> {
    let (name, role) = value.rsplit_once(',')?;
    let name = name.trim().to_string();
    if name.is_empty() {
        return None;
    }
    Some((name, role.trim().to_ascii_lowercase()))
}

fn persisted_read_status(book: &BookReadModel) -> &'static str {
    match book.read_progress.as_ref() {
        Some(progress) if progress.completed => "READ",
        Some(progress) if progress.page > 0 => "IN_PROGRESS",
        _ => "UNREAD",
    }
}

fn sort_readlist_books(books: &mut [BookReadModel], ordered: bool) {
    if ordered {
        return;
    }

    books.sort_by(|left, right| left.metadata_release_date.cmp(&right.metadata_release_date));
}

fn sort_readlists(
    content: &mut [ReadListReadModel],
    sort: ReadListsSort,
    search_ranks: Option<&HashMap<String, usize>>,
) {
    match sort {
        ReadListsSort::NameAsc => sort_readlists_by_name(content, false),
        ReadListsSort::NameDesc => sort_readlists_by_name(content, true),
        ReadListsSort::CreatedDateAsc => {
            content.sort_by(|left, right| left.created_date.cmp(&right.created_date));
        }
        ReadListsSort::CreatedDateDesc => {
            content.sort_by(|left, right| right.created_date.cmp(&left.created_date));
        }
        ReadListsSort::LastModifiedDateAsc => {
            content.sort_by(|left, right| left.last_modified_date.cmp(&right.last_modified_date));
        }
        ReadListsSort::LastModifiedDateDesc => {
            content.sort_by(|left, right| right.last_modified_date.cmp(&left.last_modified_date));
        }
        ReadListsSort::SearchOrName => {
            if let Some(search_ranks) = search_ranks {
                content.sort_by_key(|readlist| {
                    search_ranks
                        .get(readlist.id.as_str())
                        .copied()
                        .unwrap_or(usize::MAX)
                });
            } else {
                sort_readlists_by_name(content, false);
            }
        }
    }
}

fn sort_readlists_by_name(content: &mut [ReadListReadModel], descending: bool) {
    let collator = readlists_unicode_collator();
    content.sort_by(|left, right| {
        if descending {
            collator.compare(right.name.as_str(), left.name.as_str())
        } else {
            collator.compare(left.name.as_str(), right.name.as_str())
        }
    });
}

fn readlists_unicode_collator() -> icu::collator::CollatorBorrowed<'static> {
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Tertiary);
    Collator::try_new(locale!("und").into(), options)
        .expect("unicode collator for readlists sorting should construct")
}

fn paginate_readlists(
    content: Vec<ReadListReadModel>,
    query: &ReadListsQuery,
) -> PageEnvelope<ReadListReadModel> {
    let total_elements = content.len();
    let page_size = if query.unpaged {
        total_elements.max(20)
    } else {
        query.size.max(1)
    };
    let page_number = if query.unpaged { 0 } else { query.page };
    let page_content = if query.unpaged {
        content
    } else {
        let offset = query.page.saturating_mul(page_size);
        if offset >= total_elements {
            vec![]
        } else {
            content
                .into_iter()
                .skip(offset)
                .take(page_size)
                .collect::<Vec<_>>()
        }
    };

    PageEnvelope::from_slice(page_content, page_number, page_size, total_elements)
}

fn paginate_readlist_books(
    books: Vec<BookReadModel>,
    query: &ReadListBooksQuery,
) -> PageEnvelope<BookReadModel> {
    let total_elements = books.len();
    if query.unpaged {
        return PageEnvelope::from_slice(books, 0, total_elements.max(1), total_elements);
    }

    let offset = query.page.saturating_mul(query.size);
    let content = if offset >= total_elements {
        Vec::new()
    } else {
        books.into_iter().skip(offset).take(query.size).collect()
    };
    PageEnvelope::from_slice(content, query.page, query.size, total_elements)
}

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let name = parts.next().unwrap_or_default();
        if name != key {
            return None;
        }
        Some(parts.next().unwrap_or_default())
    })
}

fn query_values<'a>(query: &'a str, key: &str) -> Vec<&'a str> {
    query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let name = parts.next().unwrap_or_default();
            if name != key {
                return None;
            }
            Some(parts.next().unwrap_or_default())
        })
        .collect()
}

fn query_bool(query: &str, key: &str) -> bool {
    query_value(query, key)
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn decoded_query_values(query: &str, key: &str) -> Option<Vec<String>> {
    let values = query_values(query, key)
        .into_iter()
        .map(decode_query_component)
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>();

    (!values.is_empty()).then_some(values)
}

fn decode_query_component(value: &str) -> String {
    let mut decoded = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let first = (bytes[index + 1] as char).to_digit(16);
                let second = (bytes[index + 2] as char).to_digit(16);

                if let (Some(first), Some(second)) = (first, second) {
                    decoded.push((first * 16 + second) as u8);
                    index += 3;
                } else {
                    decoded.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }

    String::from_utf8_lossy(&decoded).into_owned()
}

fn parse_csv_values(raw: &str) -> Vec<String> {
    if raw.trim().is_empty() {
        return vec![];
    }

    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn generated_readlist_id() -> String {
    format!("readlist-{}", random_hex_token(12))
}

fn random_hex_token(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        let _ = file.read_exact(&mut bytes);
    } else {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = ((seed >> ((index % 8) * 8)) as u8) ^ (index as u8).wrapping_mul(31);
        }
    }

    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use komga_domain::common_ids::LibraryId;
    use komga_domain::discovery::DiscoveryQueryContext;
    use std::{collections::HashMap, sync::Mutex};

    use crate::discovery::{
        BookMetadataAuthorReadModel, BookMetadataLinkReadModel, BookReadModel,
        DiscoveryPersistedReadlistBookRecord, DiscoveryPersistedReadlistRecord,
        PersistedBookResourceRecord, PersistedComicrackMatchCandidateRecord, ReadlistBookPort,
        ReadlistPort, ReadlistSearchPort,
    };

    use super::{
        ReadListBooksOwnership, ReadListBooksQuery, ReadListsQuery, ReadListsSort,
        ReadlistListService, ReadlistMutationInput, ReadlistMutationService,
        ReadlistVisibilityService, classify_readlist_books_query, normalize_readlists_search,
        resolve_readlist_books_query,
    };

    #[test]
    fn normalize_readlists_search_returns_none_for_blank_effective_values() {
        assert_eq!(normalize_readlists_search(None), None);
        assert_eq!(normalize_readlists_search(Some(String::new())), None);
        assert_eq!(
            normalize_readlists_search(Some("   \t\n".to_string())),
            None
        );
    }

    #[test]
    fn normalize_readlists_search_preserves_non_blank_value_without_trimming() {
        let decoded = " alpha ".to_string();

        assert_eq!(
            normalize_readlists_search(Some(decoded.clone())),
            Some(decoded),
        );
    }

    #[test]
    fn classify_readlist_books_query_accepts_unpaged_with_library_and_extra_filters() {
        let query = ReadListBooksQuery {
            readlist_id: "readlist-1".to_string(),
            page: 0,
            size: 20,
            unpaged: true,
            library_ids: Some(vec!["library-1".to_string()]),
            deleted: Some(false),
            tags: Some(vec!["favorite".to_string()]),
            read_statuses: Some(vec!["read".to_string()]),
            media_statuses: Some(vec!["READY".to_string()]),
            authors: Some(vec!["alice".to_string()]),
        };

        assert_eq!(
            classify_readlist_books_query(&query),
            Ok(ReadListBooksOwnership::DependencyOnly),
        );
    }

    #[tokio::test]
    async fn readlists_list_service_uses_requested_libraries_only_for_candidate_scope() {
        let ports = TestReadlistPorts::new();
        let service = ReadlistListService::new(&ports, &ports, &ports);
        let requested_context = context_with_libraries(["library-a"]);
        let visibility_context = context_with_libraries(["library-a", "library-b"]);

        let page = service
            .list_readlists(
                &requested_context,
                &visibility_context,
                ReadListsQuery {
                    page: 0,
                    size: 20,
                    unpaged: false,
                    library_ids: Some(vec!["library-a".to_string()]),
                    search: Some("space".to_string()),
                    sort: ReadListsSort::SearchOrName,
                },
            )
            .await
            .expect("readlists should resolve");

        assert_eq!(page.total_elements, 1);
        let readlist = page
            .content
            .first()
            .expect("visible readlist should remain");
        assert_eq!(readlist.id, "readlist-1");
        assert_eq!(
            readlist.book_ids,
            vec!["book-a".to_string(), "book-b".to_string()]
        );
        assert!(!readlist.filtered);
    }

    #[tokio::test]
    async fn readlist_mutation_service_create_persists_sse_and_search_sync_as_one_boundary() {
        let ports = TestReadlistPorts::new();
        let service = ReadlistMutationService::new(&ports);
        let result = service
            .create_readlist(ReadlistMutationInput {
                name: "New ReadList".to_string(),
                summary: "Created from service".to_string(),
                ordered: true,
                book_ids: vec!["book-a".to_string()],
            })
            .await
            .expect("readlist create should complete");

        assert!(result.readlist_id.starts_with("readlist-"));
        assert_eq!(ports.created_readlists().len(), 1);
        assert_eq!(
            ports.search_upserts(),
            vec![result.readlist_id],
            "search sync belongs to the mutation boundary",
        );
    }

    #[tokio::test]
    async fn readlist_visibility_service_exposes_visible_book_ids_for_media_boundaries() {
        let ports = TestReadlistPorts::new();
        let service = ReadlistVisibilityService::new(&ports, &ports);

        let book_ids = service
            .visible_readlist_book_ids(&context_with_libraries(["library-a"]), "readlist-1")
            .await
            .expect("readlist visibility should resolve")
            .expect("readlist should remain visible");

        assert_eq!(book_ids, vec!["book-a"]);
    }

    #[tokio::test]
    async fn readlist_visibility_service_sorts_unordered_books_before_pagination() {
        let mut ports = TestReadlistPorts::new();
        ports.readlists.push(readlist_record_with_ordered(
            "readlist-unordered",
            "Unordered",
            false,
        ));
        ports.readlist_books.insert(
            "readlist-unordered".to_string(),
            vec![
                readlist_book_record("book-late", "library-a"),
                readlist_book_record("book-early", "library-a"),
                readlist_book_record("book-middle", "library-a"),
            ],
        );
        ports.books.insert(
            "book-late".to_string(),
            sample_book_with_release_date("book-late", Some("2024-03-01")),
        );
        ports.books.insert(
            "book-early".to_string(),
            sample_book_with_release_date("book-early", Some("2024-01-01")),
        );
        ports.books.insert(
            "book-middle".to_string(),
            sample_book_with_release_date("book-middle", Some("2024-02-01")),
        );
        for book_id in ["book-late", "book-early", "book-middle"] {
            ports.book_resources.insert(
                book_id.to_string(),
                PersistedBookResourceRecord {
                    library_id: "library-a".to_string(),
                    age_rating: None,
                    sharing_labels: String::new(),
                },
            );
        }

        let service = ReadlistVisibilityService::new(&ports, &ports);
        let page = service
            .list_readlist_books(
                &context_with_libraries(["library-a"]),
                resolve_readlist_books_query("readlist-unordered", "page=0&size=2"),
            )
            .await
            .expect("readlist books should resolve")
            .expect("readlist should be visible");

        assert_eq!(page.total_elements, 3);
        assert_eq!(
            page.content
                .iter()
                .map(|book| book.id.as_str())
                .collect::<Vec<_>>(),
            vec!["book-early", "book-middle"],
        );
    }

    fn context_with_libraries<const N: usize>(libraries: [&str; N]) -> DiscoveryQueryContext {
        DiscoveryQueryContext {
            user_id: None,
            is_admin: false,
            authorized_library_ids: Some(libraries.into_iter().map(LibraryId::from).collect()),
            restrictions: None,
        }
    }

    struct TestReadlistPorts {
        readlists: Vec<DiscoveryPersistedReadlistRecord>,
        readlist_books: HashMap<String, Vec<DiscoveryPersistedReadlistBookRecord>>,
        books: HashMap<String, BookReadModel>,
        book_resources: HashMap<String, PersistedBookResourceRecord>,
        search_hits: HashMap<String, Vec<(f32, String)>>,
        created_readlists: Mutex<Vec<String>>,
        updated_readlists: Mutex<Vec<String>>,
        deleted_readlists: Mutex<Vec<String>>,
        search_upserts: Mutex<Vec<String>>,
        search_deletes: Mutex<Vec<String>>,
    }

    impl TestReadlistPorts {
        fn new() -> Self {
            let mut readlist_books = HashMap::new();
            readlist_books.insert(
                "readlist-1".to_string(),
                vec![
                    readlist_book_record("book-a", "library-a"),
                    readlist_book_record("book-b", "library-b"),
                ],
            );
            readlist_books.insert(
                "readlist-2".to_string(),
                vec![readlist_book_record("book-c", "library-b")],
            );

            let books = ["book-a", "book-b", "book-c"]
                .into_iter()
                .map(|book_id| (book_id.to_string(), sample_book(book_id)))
                .collect::<HashMap<_, _>>();
            let book_resources = [
                ("book-a", "library-a"),
                ("book-b", "library-b"),
                ("book-c", "library-b"),
            ]
            .into_iter()
            .map(|(book_id, library_id)| {
                (
                    book_id.to_string(),
                    PersistedBookResourceRecord {
                        library_id: library_id.to_string(),
                        age_rating: None,
                        sharing_labels: String::new(),
                    },
                )
            })
            .collect::<HashMap<_, _>>();
            let search_hits = HashMap::from([(
                "space".to_string(),
                vec![
                    (2.0, "readlist-2".to_string()),
                    (1.0, "readlist-1".to_string()),
                ],
            )]);

            Self {
                readlists: vec![
                    readlist_record_with_ordered("readlist-1", "Visible", true),
                    readlist_record_with_ordered("readlist-2", "Library B Only", true),
                ],
                readlist_books,
                books,
                book_resources,
                search_hits,
                created_readlists: Mutex::new(Vec::new()),
                updated_readlists: Mutex::new(Vec::new()),
                deleted_readlists: Mutex::new(Vec::new()),
                search_upserts: Mutex::new(Vec::new()),
                search_deletes: Mutex::new(Vec::new()),
            }
        }

        fn created_readlists(&self) -> Vec<String> {
            self.created_readlists
                .lock()
                .expect("created readlists lock should not be poisoned")
                .clone()
        }

        fn search_upserts(&self) -> Vec<String> {
            self.search_upserts
                .lock()
                .expect("search upserts lock should not be poisoned")
                .clone()
        }
    }

    #[async_trait]
    impl ReadlistPort for TestReadlistPorts {
        async fn load_persisted_readlists(
            &self,
        ) -> Result<Vec<DiscoveryPersistedReadlistRecord>, String> {
            Ok(self.readlists.clone())
        }

        async fn load_persisted_readlist_detail(
            &self,
            readlist_id: &str,
        ) -> Result<Option<DiscoveryPersistedReadlistRecord>, String> {
            Ok(self
                .readlists
                .iter()
                .find(|readlist| readlist.id == readlist_id)
                .cloned())
        }

        async fn load_persisted_readlist_book_rows(
            &self,
            readlist_id: &str,
        ) -> Result<Vec<DiscoveryPersistedReadlistBookRecord>, String> {
            Ok(self
                .readlist_books
                .get(readlist_id)
                .cloned()
                .unwrap_or_default())
        }

        async fn load_comicrack_match_candidates(
            &self,
        ) -> Result<Vec<PersistedComicrackMatchCandidateRecord>, String> {
            Ok(vec![])
        }

        async fn persist_readlist_create(
            &self,
            readlist_id: &str,
            _name: &str,
            _summary: &str,
            _ordered: bool,
            _book_ids: &[String],
        ) -> Result<(), String> {
            self.created_readlists
                .lock()
                .expect("created readlists lock should not be poisoned")
                .push(readlist_id.to_string());
            Ok(())
        }

        async fn persist_readlist_update(
            &self,
            readlist_id: &str,
            _name: &str,
            _summary: &str,
            _ordered: bool,
            _book_ids: &[String],
        ) -> Result<bool, String> {
            self.updated_readlists
                .lock()
                .expect("updated readlists lock should not be poisoned")
                .push(readlist_id.to_string());
            Ok(self
                .readlists
                .iter()
                .any(|readlist| readlist.id == readlist_id))
        }

        async fn delete_persisted_readlist(&self, readlist_id: &str) -> Result<bool, String> {
            self.deleted_readlists
                .lock()
                .expect("deleted readlists lock should not be poisoned")
                .push(readlist_id.to_string());
            Ok(self
                .readlists
                .iter()
                .any(|readlist| readlist.id == readlist_id))
        }

        async fn upsert_readlist_search_document(&self, readlist_id: &str) -> Result<bool, String> {
            self.search_upserts
                .lock()
                .expect("search upserts lock should not be poisoned")
                .push(readlist_id.to_string());
            Ok(true)
        }

        async fn delete_readlist_search_document(&self, readlist_id: &str) -> Result<(), String> {
            self.search_deletes
                .lock()
                .expect("search deletes lock should not be poisoned")
                .push(readlist_id.to_string());
            Ok(())
        }
    }

    #[async_trait]
    impl ReadlistBookPort for TestReadlistPorts {
        async fn load_persisted_book_resource(
            &self,
            book_id: &str,
        ) -> Result<Option<PersistedBookResourceRecord>, String> {
            Ok(self.book_resources.get(book_id).cloned())
        }

        async fn load_persisted_book_detail(
            &self,
            book_id: &str,
            _user_id: Option<&str>,
        ) -> Result<Option<BookReadModel>, String> {
            Ok(self.books.get(book_id).cloned())
        }

        async fn load_persisted_book_authors(
            &self,
            _book_id: &str,
        ) -> Result<Vec<BookMetadataAuthorReadModel>, String> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl ReadlistSearchPort for TestReadlistPorts {
        async fn search_readlist_scored_ids(
            &self,
            query: &str,
            _limit: usize,
        ) -> Result<Vec<(f32, String)>, String> {
            Ok(self.search_hits.get(query).cloned().unwrap_or_default())
        }
    }

    fn readlist_record_with_ordered(
        id: &str,
        name: &str,
        ordered: bool,
    ) -> DiscoveryPersistedReadlistRecord {
        DiscoveryPersistedReadlistRecord {
            id: id.to_string(),
            name: name.to_string(),
            summary: String::new(),
            ordered,
            created_date: "2024-01-01 00:00:00".to_string(),
            last_modified_date: "2024-01-01 00:00:00".to_string(),
        }
    }

    fn readlist_book_record(
        book_id: &str,
        library_id: &str,
    ) -> DiscoveryPersistedReadlistBookRecord {
        DiscoveryPersistedReadlistBookRecord {
            book_id: book_id.to_string(),
            library_id: library_id.to_string(),
        }
    }

    fn sample_book(id: &str) -> BookReadModel {
        sample_book_with_release_date(id, None)
    }

    fn sample_book_with_release_date(id: &str, release_date: Option<&str>) -> BookReadModel {
        BookReadModel {
            id: id.to_string(),
            series_id: "series-1".to_string(),
            series_title: "Series".to_string(),
            series_title_sort: "Series".to_string(),
            library_id: "library-a".to_string(),
            name: id.to_string(),
            url: format!("/books/{id}.cbz"),
            number: 1,
            created: "2024-01-01T00:00:00Z".to_string(),
            last_modified: "2024-01-01T00:00:00Z".to_string(),
            file_last_modified: "2024-01-01T00:00:00Z".to_string(),
            size_bytes: 1,
            media_status: "READY".to_string(),
            media_type: "application/zip".to_string(),
            media_pages_count: 1,
            media_comment: String::new(),
            media_epub_divina_compatible: false,
            media_epub_is_kepub: false,
            metadata_title: id.to_string(),
            metadata_title_lock: false,
            metadata_summary: String::new(),
            metadata_summary_lock: false,
            metadata_number: "1".to_string(),
            metadata_number_lock: false,
            metadata_number_sort: 1.0,
            metadata_number_sort_lock: false,
            metadata_release_date: release_date.map(str::to_string),
            metadata_release_date_lock: false,
            metadata_authors: vec![],
            metadata_authors_lock: false,
            metadata_tags: vec![],
            metadata_tags_lock: false,
            metadata_isbn: String::new(),
            metadata_isbn_lock: false,
            metadata_links: vec![BookMetadataLinkReadModel {
                label: "Site".to_string(),
                url: "https://example.com".to_string(),
            }],
            metadata_links_lock: false,
            metadata_created: "2024-01-01T00:00:00Z".to_string(),
            metadata_last_modified: "2024-01-01T00:00:00Z".to_string(),
            read_progress: None,
            deleted: false,
            file_hash: String::new(),
            oneshot: false,
        }
    }
}
