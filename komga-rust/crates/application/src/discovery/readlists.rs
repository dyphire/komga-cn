use std::collections::HashMap;

use icu::collator::{
    Collator,
    options::{CollatorOptions, Strength},
};
use icu::locale::locale;
use komga_domain::common_ids::LibraryId;
use komga_domain::discovery::{
    DiscoveryError, DiscoveryQueryContext, PageEnvelope, content_allowed_by_restrictions,
};

use super::{
    BookDetailPort, BookMetadataAuthorReadModel, BookReadModel, DiscoveryPersistedReadlistRecord,
    DiscoverySearchService, PersistedBookResourceRecord, ReadListReadModel, ReadlistPort,
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

pub struct ReadlistListService<'a> {
    readlists: &'a dyn ReadlistPort,
    books: &'a dyn BookDetailPort,
    search: &'a dyn DiscoverySearchService,
}

impl<'a> ReadlistListService<'a> {
    pub fn new(
        readlists: &'a dyn ReadlistPort,
        books: &'a dyn BookDetailPort,
        search: &'a dyn DiscoverySearchService,
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
        let requested_library_ids =
            library_ids_to_strings(requested_context.authorized_library_ids.as_ref());
        let mut content = self
            .load_readlists(requested_library_ids.as_deref())
            .await?;

        let search_ranks = match query.search.as_deref() {
            Some(search) => self.search_ranks(search).await?,
            None => None,
        };
        if let Some(search_ranks) = search_ranks.as_ref() {
            content.retain(|readlist| search_ranks.contains_key(readlist.id.as_str()));
        }

        let visibility_query = readlist_books_visibility_query(None);
        let requested_library_query = query
            .library_ids
            .clone()
            .map(|library_ids| readlist_books_visibility_query(Some(library_ids)));

        let mut visible_content = Vec::with_capacity(content.len());
        for readlist in content {
            let Some(mut visible_readlist) = self
                .load_readlist_detail(&readlist.id, visibility_context)
                .await?
            else {
                continue;
            };

            if let Some(requested_library_query) = requested_library_query.as_ref() {
                let Some(requested_library_books) = self
                    .visible_readlist_books(
                        &readlist.id,
                        visibility_context,
                        requested_library_query,
                    )
                    .await?
                else {
                    continue;
                };

                if requested_library_books.is_empty() {
                    continue;
                }
            }

            let Some(visible_books) = self
                .visible_readlist_books(&readlist.id, visibility_context, &visibility_query)
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

    async fn load_readlists(
        &self,
        library_ids: Option<&[String]>,
    ) -> Result<Vec<ReadListReadModel>, String> {
        let rows = self.readlists.load_persisted_readlists().await?;

        let mut readlists = Vec::with_capacity(rows.len());
        for row in rows {
            let id = row.id.clone();
            let (book_ids, filtered) = self.load_readlist_book_ids(&id, library_ids).await?;
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
        let (book_ids, filtered) = self
            .load_readlist_book_ids(readlist_id, authorized_library_ids.as_deref())
            .await?;

        Ok(Some(readlist_from_record(row, book_ids, filtered)))
    }

    async fn load_readlist_book_ids(
        &self,
        readlist_id: &str,
        library_ids: Option<&[String]>,
    ) -> Result<(Vec<String>, bool), String> {
        let rows = self
            .readlists
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

    async fn visible_readlist_books(
        &self,
        readlist_id: &str,
        context: &DiscoveryQueryContext,
        query: &ReadListBooksQuery,
    ) -> Result<Option<Vec<BookReadModel>>, String> {
        let Some(readlist) = self.load_readlist_detail(readlist_id, context).await? else {
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
            .load_persisted_readlist_book_rows(readlist_id)
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

fn readlist_books_visibility_query(library_ids: Option<Vec<String>>) -> ReadListBooksQuery {
    ReadListBooksQuery {
        readlist_id: String::new(),
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

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use komga_domain::common_ids::LibraryId;
    use komga_domain::discovery::DiscoveryQueryContext;
    use std::collections::HashMap;

    use crate::discovery::{
        BookDetailPort, BookMetadataAuthorReadModel, BookMetadataLinkReadModel, BookReadModel,
        DiscoveryPersistedReadlistBookRecord, DiscoveryPersistedReadlistRecord,
        DiscoverySearchService, PersistedAuthorEntry, PersistedAuthorsScope,
        PersistedBookBrowseEntry, PersistedBookResourceRecord, PersistedBookSiblingDirectionRecord,
        PersistedComicrackMatchCandidateRecord, ReadlistPort,
    };

    use super::{
        ReadListBooksOwnership, ReadListBooksQuery, ReadListsQuery, ReadListsSort,
        ReadlistListService, classify_readlist_books_query, normalize_readlists_search,
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
    }

    impl TestReadlistPorts {
        fn new() -> Self {
            let mut readlist_books = HashMap::new();
            readlist_books.insert(
                "readlist-1".to_string(),
                vec![
                    DiscoveryPersistedReadlistBookRecord {
                        book_id: "book-a".to_string(),
                        library_id: "library-a".to_string(),
                    },
                    DiscoveryPersistedReadlistBookRecord {
                        book_id: "book-b".to_string(),
                        library_id: "library-b".to_string(),
                    },
                ],
            );
            readlist_books.insert(
                "readlist-2".to_string(),
                vec![DiscoveryPersistedReadlistBookRecord {
                    book_id: "book-c".to_string(),
                    library_id: "library-b".to_string(),
                }],
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
                    readlist_record("readlist-1", "Visible"),
                    readlist_record("readlist-2", "Library B Only"),
                ],
                readlist_books,
                books,
                book_resources,
                search_hits,
            }
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
            _readlist_id: &str,
            _name: &str,
            _summary: &str,
            _ordered: bool,
            _book_ids: &[String],
        ) -> Result<(), String> {
            unimplemented!("not used by readlist list service tests")
        }

        async fn persist_readlist_update(
            &self,
            _readlist_id: &str,
            _name: &str,
            _summary: &str,
            _ordered: bool,
            _book_ids: &[String],
        ) -> Result<bool, String> {
            unimplemented!("not used by readlist list service tests")
        }

        async fn delete_persisted_readlist(&self, _readlist_id: &str) -> Result<bool, String> {
            unimplemented!("not used by readlist list service tests")
        }

        async fn upsert_readlist_search_document(
            &self,
            _readlist_id: &str,
        ) -> Result<bool, String> {
            unimplemented!("not used by readlist list service tests")
        }

        async fn delete_readlist_search_document(&self, _readlist_id: &str) -> Result<(), String> {
            unimplemented!("not used by readlist list service tests")
        }
    }

    #[async_trait]
    impl BookDetailPort for TestReadlistPorts {
        async fn load_book_id_by_sorted_position(
            &self,
            _index: usize,
        ) -> Result<Option<String>, String> {
            unimplemented!("not used by readlist list service tests")
        }

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

        async fn load_persisted_book_sibling_id(
            &self,
            _book_id: &str,
            _direction: PersistedBookSiblingDirectionRecord,
        ) -> Result<Option<String>, String> {
            unimplemented!("not used by readlist list service tests")
        }

        async fn load_persisted_book_authors(
            &self,
            _book_id: &str,
        ) -> Result<Vec<BookMetadataAuthorReadModel>, String> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl DiscoverySearchService for TestReadlistPorts {
        async fn load_author_names(
            &self,
            _search: &str,
            _authorized_library_ids: Option<&[String]>,
        ) -> Result<Vec<String>, String> {
            unimplemented!("not used by readlist list service tests")
        }

        async fn load_author_roles(
            &self,
            _authorized_library_ids: Option<&[String]>,
        ) -> Result<Vec<String>, String> {
            unimplemented!("not used by readlist list service tests")
        }

        async fn load_authors_by_scope(
            &self,
            _scope: PersistedAuthorsScope,
            _authorized_library_ids: Option<&[String]>,
        ) -> Result<Vec<PersistedAuthorEntry>, String> {
            unimplemented!("not used by readlist list service tests")
        }

        async fn load_persisted_library_ids(&self) -> Result<Vec<String>, String> {
            unimplemented!("not used by readlist list service tests")
        }

        async fn search_collection_ids(
            &self,
            _query: &str,
            _limit: usize,
        ) -> Result<Vec<String>, String> {
            unimplemented!("not used by readlist list service tests")
        }

        async fn search_readlist_scored_ids(
            &self,
            query: &str,
            _limit: usize,
        ) -> Result<Vec<(f32, String)>, String> {
            Ok(self.search_hits.get(query).cloned().unwrap_or_default())
        }

        async fn load_ondeck_books(
            &self,
            _user_id: &str,
        ) -> Result<Vec<PersistedBookBrowseEntry>, String> {
            unimplemented!("not used by readlist list service tests")
        }

        async fn load_duplicate_books(&self) -> Result<Vec<PersistedBookBrowseEntry>, String> {
            unimplemented!("not used by readlist list service tests")
        }
    }

    fn readlist_record(id: &str, name: &str) -> DiscoveryPersistedReadlistRecord {
        DiscoveryPersistedReadlistRecord {
            id: id.to_string(),
            name: name.to_string(),
            summary: String::new(),
            ordered: true,
            created_date: "2024-01-01 00:00:00".to_string(),
            last_modified_date: "2024-01-01 00:00:00".to_string(),
        }
    }

    fn sample_book(id: &str) -> BookReadModel {
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
            metadata_release_date: None,
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
