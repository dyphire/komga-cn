use std::collections::{BTreeMap, HashMap};

use crate::random_tokens::random_hex_token;
use crate::runtime_sse::{RuntimeSseEvent, RuntimeSseEventSink};
use komga_domain::common_ids::LibraryId;
use komga_domain::discovery::{
    DiscoveryError, DiscoveryQueryContext, MediaStatus, PageEnvelope, ReadStatus,
    compare_book_names, content_allowed_by_restrictions,
};
use quick_xml::Reader as XmlReader;
use quick_xml::XmlVersion;
use quick_xml::events::Event as XmlEvent;

use super::{
    BookReadModel, DiscoveryPersistedReadlistRecord, PersistedBookResourceRecord,
    PersistedComicrackMatchCandidateRecord, ReadListReadModel, ReadlistBookPort,
    ReadlistComicRackMatchPort, ReadlistMutationPort, ReadlistProjectionPort, ReadlistSearchPort,
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
    pub read_statuses: Option<Vec<ReadStatus>>,
    pub media_statuses: Option<Vec<MediaStatus>>,
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

#[derive(Debug)]
pub enum ReadlistMutationError {
    DuplicateName,
    Persistence(anyhow::Error),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComicRackReadListRequestBook {
    pub series_candidates: Vec<String>,
    pub number: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComicRackReadListRequest {
    pub name: String,
    pub books: Vec<ComicRackReadListRequestBook>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComicRackReadListParseError {
    InvalidXml,
    MissingBooks,
    MissingName,
    MissingBookIdentity,
}

impl ComicRackReadListParseError {
    pub fn error_code(self) -> &'static str {
        match self {
            Self::InvalidXml => "ERR_1015",
            Self::MissingBooks => "ERR_1029",
            Self::MissingName => "ERR_1030",
            Self::MissingBookIdentity => "ERR_1031",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComicRackReadListMatchError {
    DuplicateName,
}

impl ComicRackReadListMatchError {
    pub fn error_code(self) -> &'static str {
        match self {
            Self::DuplicateName => "ERR_1009",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComicRackMatchSeries {
    pub series_id: String,
    pub title: String,
    pub release_date: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComicRackMatchBook {
    pub book_id: String,
    pub number: String,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComicRackReadListMatchGroup {
    pub series: ComicRackMatchSeries,
    pub books: Vec<ComicRackMatchBook>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComicRackReadListRequestMatch {
    pub request: ComicRackReadListRequestBook,
    pub matches: Vec<ComicRackReadListMatchGroup>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComicRackReadListMatchResult {
    pub name: String,
    pub error: Option<ComicRackReadListMatchError>,
    pub requests: Vec<ComicRackReadListRequestMatch>,
}

struct ReadlistBookVisibility {
    book_ids: Vec<String>,
    filtered: bool,
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

pub struct ReadlistProjectionService<'a> {
    readlists: &'a dyn ReadlistProjectionPort,
    books: &'a dyn ReadlistBookPort,
    search: &'a dyn ReadlistSearchPort,
}

struct VisibleReadlistProjection {
    readlist: ReadListReadModel,
    books: Vec<BookReadModel>,
}

impl<'a> ReadlistProjectionService<'a> {
    pub fn new(
        readlists: &'a dyn ReadlistProjectionPort,
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
    ) -> anyhow::Result<PageEnvelope<ReadListReadModel>> {
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

        let mut visible_content = Vec::with_capacity(content.len());
        for readlist in content {
            if let Some(library_ids) = query.library_ids.as_ref() {
                let requested_library_query =
                    readlist_books_visibility_query(readlist.id.clone(), Some(library_ids.clone()));
                let Some(requested_library_projection) = self
                    .visible_readlist_projection(visibility_context, &requested_library_query)
                    .await?
                else {
                    continue;
                };

                if requested_library_projection.books.is_empty() {
                    continue;
                }
            }

            let visibility_query = readlist_books_visibility_query(readlist.id.clone(), None);
            let Some(projection) = self
                .visible_readlist_projection(visibility_context, &visibility_query)
                .await?
            else {
                continue;
            };

            if projection.books.is_empty() {
                if projection.readlist.book_ids.is_empty() && !projection.readlist.filtered {
                    visible_content.push(projection.readlist);
                }
                continue;
            }

            visible_content.push(projection.readlist);
        }

        sort_readlists(&mut visible_content, query.sort, search_ranks.as_ref());
        Ok(paginate_readlists(visible_content, &query))
    }

    async fn search_ranks(&self, search: &str) -> anyhow::Result<Option<HashMap<String, usize>>> {
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
            for hit in ranked_hits {
                if let std::collections::hash_map::Entry::Vacant(entry) = ranks.entry(hit.id) {
                    entry.insert(next_rank);
                    next_rank += 1;
                }
            }
        }

        Ok(Some(ranks))
    }
}

pub struct ReadlistMutationService<'a> {
    readlists: &'a dyn ReadlistMutationPort,
    runtime_events: &'a dyn RuntimeSseEventSink,
}

pub struct ComicRackReadListMatchService<'a> {
    readlists: &'a dyn ReadlistComicRackMatchPort,
}

impl<'a> ComicRackReadListMatchService<'a> {
    pub fn new(readlists: &'a dyn ReadlistComicRackMatchPort) -> Self {
        Self { readlists }
    }

    pub async fn match_readlist(
        &self,
        request: &ComicRackReadListRequest,
    ) -> anyhow::Result<ComicRackReadListMatchResult> {
        let readlists = self.readlists.load_persisted_readlists().await?;
        let error = readlists
            .iter()
            .any(|readlist| readlist.name.eq_ignore_ascii_case(&request.name))
            .then_some(ComicRackReadListMatchError::DuplicateName);

        let candidates = self.readlists.load_comicrack_match_candidates().await?;
        let requests = request
            .books
            .iter()
            .map(|book| match_comicrack_request_book(book, &candidates))
            .collect::<Vec<_>>();

        Ok(ComicRackReadListMatchResult {
            name: request.name.clone(),
            error,
            requests,
        })
    }
}

impl<'a> ReadlistProjectionService<'a> {
    pub async fn readlist_detail(
        &self,
        context: &DiscoveryQueryContext,
        readlist_id: &str,
    ) -> anyhow::Result<Option<ReadListReadModel>> {
        let query = readlist_books_visibility_query(readlist_id, None);
        let Some(projection) = self.visible_readlist_projection(context, &query).await? else {
            return Ok(None);
        };
        if projection.books.is_empty() {
            return Ok(None);
        }

        Ok(Some(projection.readlist))
    }

    pub async fn list_readlist_books(
        &self,
        context: &DiscoveryQueryContext,
        query: ReadListBooksQuery,
    ) -> anyhow::Result<Option<PageEnvelope<BookReadModel>>> {
        let Some(mut projection) = self.visible_readlist_projection(context, &query).await? else {
            return Ok(None);
        };

        sort_readlist_books(&mut projection.books, projection.readlist.ordered);
        Ok(Some(paginate_readlist_books(projection.books, &query)))
    }

    pub async fn visible_readlist_book_ids(
        &self,
        context: &DiscoveryQueryContext,
        readlist_id: &str,
    ) -> anyhow::Result<Option<Vec<String>>> {
        let query = readlist_books_visibility_query(readlist_id, None);
        let Some(projection) = self.visible_readlist_projection(context, &query).await? else {
            return Ok(None);
        };

        Ok(Some(projection.readlist.book_ids))
    }

    pub async fn readlist_book_sibling(
        &self,
        context: &DiscoveryQueryContext,
        readlist_id: &str,
        book_id: &str,
        next: bool,
    ) -> anyhow::Result<Option<BookReadModel>> {
        let query = readlist_books_visibility_query(readlist_id, None);
        let Some(mut projection) = self.visible_readlist_projection(context, &query).await? else {
            return Ok(None);
        };

        sort_readlist_books(&mut projection.books, projection.readlist.ordered);
        let Some(current_index) = projection.books.iter().position(|book| book.id == book_id)
        else {
            return Ok(None);
        };
        let sibling_index = if next {
            current_index + 1
        } else if current_index == 0 {
            return Ok(None);
        } else {
            current_index - 1
        };

        Ok(projection.books.get(sibling_index).cloned())
    }

    pub async fn readlists_for_book(
        &self,
        candidate_library_ids: Option<&[String]>,
        visibility_context: &DiscoveryQueryContext,
        book_id: &str,
    ) -> anyhow::Result<Vec<ReadListReadModel>> {
        let mut readlists = self.load_readlists(candidate_library_ids).await?;
        readlists.retain(|readlist| readlist.book_ids.iter().any(|id| id == book_id));

        let mut visible_readlists = Vec::with_capacity(readlists.len());
        for readlist in readlists {
            let query = readlist_books_visibility_query(readlist.id.clone(), None);
            let Some(projection) = self
                .visible_readlist_projection(visibility_context, &query)
                .await?
            else {
                continue;
            };
            if !projection.readlist.book_ids.iter().any(|id| id == book_id) {
                continue;
            }

            visible_readlists.push(projection.readlist);
        }

        Ok(visible_readlists)
    }

    async fn load_readlists(
        &self,
        library_ids: Option<&[String]>,
    ) -> anyhow::Result<Vec<ReadListReadModel>> {
        let rows = self.readlists.load_persisted_readlists().await?;

        let mut readlists = Vec::with_capacity(rows.len());
        for row in rows {
            let id = row.id.clone();
            let visibility = load_readlist_book_ids(self.readlists, &id, library_ids).await?;
            if library_ids.is_some() && visibility.book_ids.is_empty() {
                continue;
            }

            readlists.push(readlist_from_record(
                row,
                visibility.book_ids,
                visibility.filtered,
            ));
        }

        Ok(readlists)
    }

    async fn load_readlist_detail(
        &self,
        readlist_id: &str,
        context: &DiscoveryQueryContext,
    ) -> anyhow::Result<Option<ReadListReadModel>> {
        let Some(row) = self
            .readlists
            .load_persisted_readlist_detail(readlist_id)
            .await?
        else {
            return Ok(None);
        };

        let authorized_library_ids =
            library_ids_to_strings(context.authorized_library_ids.as_ref());
        let visibility = load_readlist_book_ids(
            self.readlists,
            readlist_id,
            authorized_library_ids.as_deref(),
        )
        .await?;

        Ok(Some(readlist_from_record(
            row,
            visibility.book_ids,
            visibility.filtered,
        )))
    }

    async fn visible_readlist_projection(
        &self,
        context: &DiscoveryQueryContext,
        query: &ReadListBooksQuery,
    ) -> anyhow::Result<Option<VisibleReadlistProjection>> {
        let Some(mut readlist) = self
            .load_readlist_detail(&query.readlist_id, context)
            .await?
        else {
            return Ok(None);
        };
        if context.authorized_library_ids.is_some() && readlist.book_ids.is_empty() {
            return Ok(None);
        }

        let books = self.visible_readlist_books(context, query).await?;
        let visible_book_ids = books.iter().map(|book| book.id.clone()).collect::<Vec<_>>();
        readlist.filtered = readlist.filtered || readlist.book_ids != visible_book_ids;
        readlist.book_ids = visible_book_ids;

        Ok(Some(VisibleReadlistProjection { readlist, books }))
    }

    async fn visible_readlist_books(
        &self,
        context: &DiscoveryQueryContext,
        query: &ReadListBooksQuery,
    ) -> anyhow::Result<Vec<BookReadModel>> {
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

            if !matches_readlist_book_filters(&detail, query) {
                continue;
            }

            visible.push(detail);
        }

        Ok(visible)
    }
}

impl<'a> ReadlistMutationService<'a> {
    pub fn new(
        readlists: &'a dyn ReadlistMutationPort,
        runtime_events: &'a dyn RuntimeSseEventSink,
    ) -> Self {
        Self {
            readlists,
            runtime_events,
        }
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

        self.runtime_events
            .register(RuntimeSseEvent::ReadListAdded {
                readlist_id: readlist_id.clone(),
                book_ids: input.book_ids.clone(),
            });
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

        self.runtime_events
            .register(RuntimeSseEvent::ReadListChanged {
                readlist_id: readlist_id.to_string(),
                book_ids: input.book_ids.clone(),
            });
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
            self.runtime_events
                .register(RuntimeSseEvent::ReadListDeleted {
                    readlist_id: readlist_id.to_string(),
                    book_ids: readlist.book_ids,
                });
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
    ) -> anyhow::Result<Option<ReadListReadModel>> {
        let Some(row) = self
            .readlists
            .load_persisted_readlist_detail(readlist_id)
            .await?
        else {
            return Ok(None);
        };
        let visibility = load_readlist_book_ids(self.readlists, readlist_id, None).await?;

        Ok(Some(readlist_from_record(
            row,
            visibility.book_ids,
            visibility.filtered,
        )))
    }
}

pub fn parse_comicrack_readlist(
    bytes: &[u8],
) -> Result<ComicRackReadListRequest, ComicRackReadListParseError> {
    let mut reader = XmlReader::from_reader(bytes);
    reader.config_mut().trim_text(true);

    let mut buffer = Vec::new();
    let mut readlist_name = None::<String>;
    let mut books = Vec::<ComicRackReadListRequestBook>::new();
    let mut reading_name = false;
    let mut depth = 0usize;

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) => {
                depth += 1;
                if xml_name_matches(event.name().as_ref(), "Name") {
                    reading_name = true;
                } else if xml_name_matches(event.name().as_ref(), "Book") {
                    books.push(parse_comicrack_book(&event)?);
                }
            }
            Ok(XmlEvent::Empty(event)) if xml_name_matches(event.name().as_ref(), "Book") => {
                books.push(parse_comicrack_book(&event)?);
            }
            Ok(XmlEvent::Text(text)) if reading_name => {
                let value = text.as_ref().trim().to_string();
                readlist_name = Some(value);
            }
            Ok(XmlEvent::End(event)) if xml_name_matches(event.name().as_ref(), "Name") => {
                depth = depth.saturating_sub(1);
                reading_name = false;
            }
            Ok(XmlEvent::End(_)) => {
                depth = depth.saturating_sub(1);
            }
            Ok(XmlEvent::Eof) => {
                if depth != 0 {
                    return Err(ComicRackReadListParseError::InvalidXml);
                }
                break;
            }
            Err(_) => return Err(ComicRackReadListParseError::InvalidXml),
            _ => {}
        }
        buffer.clear();
    }

    let name = readlist_name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or(ComicRackReadListParseError::MissingName)?;
    if books.is_empty() {
        return Err(ComicRackReadListParseError::MissingBooks);
    }

    Ok(ComicRackReadListRequest { name, books })
}

fn parse_comicrack_book(
    event: &quick_xml::events::BytesStart<'_>,
) -> Result<ComicRackReadListRequestBook, ComicRackReadListParseError> {
    let mut series = None::<String>;
    let mut number = None::<String>;
    let mut volume = None::<String>;

    for attribute in event.attributes() {
        let attribute = attribute.map_err(|_| ComicRackReadListParseError::InvalidXml)?;
        if xml_name_matches(attribute.key.as_ref(), "Series") {
            series = Some(comicrack_attribute_value(attribute)?);
        } else if xml_name_matches(attribute.key.as_ref(), "Number") {
            number = Some(comicrack_attribute_value(attribute)?);
        } else if xml_name_matches(attribute.key.as_ref(), "Volume") {
            volume = Some(comicrack_attribute_value(attribute)?);
        }
    }

    let series = series
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or(ComicRackReadListParseError::MissingBookIdentity)?;
    let number = number
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or(ComicRackReadListParseError::MissingBookIdentity)?;

    let mut series_candidates = vec![series.clone()];
    if let Some(volume) = volume
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != "1")
    {
        series_candidates.push(format!("{series} ({volume})"));
    }

    Ok(ComicRackReadListRequestBook {
        series_candidates,
        number,
    })
}

fn comicrack_attribute_value(
    attribute: quick_xml::events::attributes::Attribute<'_>,
) -> Result<String, ComicRackReadListParseError> {
    attribute
        .normalized_value(XmlVersion::Implicit1_0)
        .map(|value| value.into_owned())
        .map_err(|_| ComicRackReadListParseError::InvalidXml)
}

fn match_comicrack_request_book(
    book: &ComicRackReadListRequestBook,
    candidates: &[PersistedComicrackMatchCandidateRecord],
) -> ComicRackReadListRequestMatch {
    let mut grouped = BTreeMap::<String, ComicRackReadListMatchGroup>::new();
    for candidate in candidates.iter().filter(|candidate| {
        book.series_candidates
            .iter()
            .any(|series| series.eq_ignore_ascii_case(&candidate.series_title))
            && normalized_comicrack_number(&book.number)
                == normalized_comicrack_number(&candidate.book_number)
    }) {
        grouped
            .entry(candidate.series_id.clone())
            .or_insert_with(|| ComicRackReadListMatchGroup {
                series: ComicRackMatchSeries {
                    series_id: candidate.series_id.clone(),
                    title: candidate.series_title.clone(),
                    release_date: candidate.series_release_date.clone(),
                },
                books: Vec::new(),
            })
            .books
            .push(ComicRackMatchBook {
                book_id: candidate.book_id.clone(),
                number: candidate.book_number.clone(),
                title: candidate.book_title.clone(),
            });
    }

    ComicRackReadListRequestMatch {
        request: book.clone(),
        matches: grouped.into_values().collect(),
    }
}

fn xml_name_matches(actual: &str, expected: &str) -> bool {
    actual.eq_ignore_ascii_case(expected)
}

fn normalized_comicrack_number(value: &str) -> String {
    let normalized = value.trim().trim_start_matches('0').to_ascii_lowercase();
    if normalized.is_empty() {
        value.trim().to_ascii_lowercase()
    } else {
        normalized
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
    readlists: &dyn ReadlistProjectionPort,
    readlist_id: &str,
    library_ids: Option<&[String]>,
) -> anyhow::Result<ReadlistBookVisibility> {
    let rows = readlists
        .load_persisted_readlist_book_rows(readlist_id)
        .await?;

    let total_count = rows.len();
    let book_ids = rows
        .into_iter()
        .filter(|row| library_ids.is_none_or(|ids| contains_id(ids, &row.library_id)))
        .map(|row| row.book_id)
        .collect::<Vec<_>>();

    Ok(ReadlistBookVisibility {
        filtered: book_ids.len() < total_count,
        book_ids,
    })
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
            &parse_group_concat_values(&resource.sharing_labels),
        )
    })
}

fn matches_readlist_book_filters(book: &BookReadModel, query: &ReadListBooksQuery) -> bool {
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
    if query
        .media_statuses
        .as_ref()
        .is_some_and(|statuses| !statuses.is_empty() && !statuses.contains(&book.media_status))
    {
        return false;
    }
    if let Some(read_statuses) = query.read_statuses.as_ref()
        && !read_statuses.is_empty()
    {
        let read_status = persisted_read_status(book);
        if !read_statuses.contains(&read_status) {
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
            .any(|filter| {
                has_author_filters = true;
                book.metadata_authors.iter().any(|author| {
                    author.name.eq_ignore_ascii_case(&filter.name)
                        && author.role.eq_ignore_ascii_case(&filter.role)
                })
            });
        if has_author_filters && !matches_author_filter {
            return false;
        }
    }

    true
}

struct ReadListAuthorFilter {
    name: String,
    role: String,
}

impl ReadListAuthorFilter {
    fn parse(value: &str) -> Option<Self> {
        match value.rsplit_once(',') {
            Some((name, role)) => {
                let name = name.trim().to_string();
                if name.is_empty() {
                    return None;
                }
                Some(Self {
                    name,
                    role: role.trim().to_ascii_lowercase(),
                })
            }
            None => None,
        }
    }
}

fn parse_author_filter(value: &str) -> Option<ReadListAuthorFilter> {
    ReadListAuthorFilter::parse(value)
}

fn persisted_read_status(book: &BookReadModel) -> ReadStatus {
    match book.read_progress.as_ref() {
        Some(progress) => ReadStatus::from_progress(progress.page, progress.completed),
        None => ReadStatus::Unread,
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
    content.sort_by(|left, right| {
        if descending {
            compare_book_names(right.name.as_str(), left.name.as_str())
        } else {
            compare_book_names(left.name.as_str(), right.name.as_str())
        }
    });
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

fn parse_group_concat_values(raw: &str) -> Vec<String> {
    const SEPARATOR: char = '\u{1e}';

    if raw.trim().is_empty() {
        return vec![];
    }

    raw.split(SEPARATOR)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn generated_readlist_id() -> String {
    format!("readlist-{}", random_hex_token(12))
}

#[cfg(test)]
mod tests {

    use komga_domain::common_ids::LibraryId;
    use komga_domain::discovery::{DiscoveryQueryContext, MediaStatus, ReadStatus};
    use std::{collections::HashMap, sync::Mutex};

    use crate::discovery::{
        BookMetadataLinkReadModel, BookReadModel, DiscoveryPersistedReadlistBookRecord,
        DiscoveryPersistedReadlistRecord, PersistedBookResourceRecord,
        PersistedComicrackMatchCandidateRecord, ReadlistBookPort, ReadlistComicRackMatchPort,
        ReadlistMutationPort, ReadlistProjectionPort, ReadlistSearchPort, ScoredSearchHit,
    };
    use crate::runtime_sse::RuntimeSseEventStore;

    use super::{
        ComicRackReadListMatchError, ComicRackReadListMatchService, ComicRackReadListParseError,
        ComicRackReadListRequest, ComicRackReadListRequestBook, ReadListBooksOwnership,
        ReadListBooksQuery, ReadListsQuery, ReadListsSort, ReadlistMutationInput,
        ReadlistMutationService, ReadlistProjectionService, classify_readlist_books_query,
        parse_comicrack_readlist,
    };

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
            read_statuses: Some(vec![ReadStatus::Read]),
            media_statuses: Some(vec![MediaStatus::Ready]),
            authors: Some(vec!["alice".to_string()]),
        };

        assert_eq!(
            classify_readlist_books_query(&query),
            Ok(ReadListBooksOwnership::DependencyOnly),
        );
    }

    #[tokio::test]
    async fn readlist_projection_service_uses_requested_libraries_only_for_candidate_scope() {
        let ports = TestReadlistPorts::new();
        let service = ReadlistProjectionService::new(&ports, &ports, &ports);
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
        let runtime_events = RuntimeSseEventStore::default();
        let service = ReadlistMutationService::new(&ports, &runtime_events);
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
    async fn readlist_projection_service_applies_visibility_consistently_across_readlist_surfaces()
    {
        let ports = TestReadlistPorts::new();
        let service = ReadlistProjectionService::new(&ports, &ports, &ports);
        let context = context_with_libraries(["library-a"]);

        let detail = service
            .readlist_detail(&context, "readlist-1")
            .await
            .expect("readlist detail should resolve")
            .expect("readlist should remain visible");
        let book_ids = service
            .visible_readlist_book_ids(&context, "readlist-1")
            .await
            .expect("readlist book ids should resolve")
            .expect("readlist book ids should remain visible");
        let books = service
            .list_readlist_books(&context, readlist_books_query("readlist-1", 0, 20))
            .await
            .expect("readlist books should resolve")
            .expect("readlist books should remain visible");
        let next_book = service
            .readlist_book_sibling(&context, "readlist-1", "book-a", true)
            .await
            .expect("readlist sibling should resolve");

        assert_eq!(detail.book_ids, vec!["book-a".to_string()]);
        assert_eq!(book_ids, detail.book_ids);
        assert_eq!(
            books
                .content
                .iter()
                .map(|book| book.id.as_str())
                .collect::<Vec<_>>(),
            vec!["book-a"],
        );
        assert!(
            next_book.is_none(),
            "hidden books must not leak through sibling navigation"
        );
    }

    #[tokio::test]
    async fn readlist_projection_service_sorts_unordered_books_before_pagination() {
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

        let service = ReadlistProjectionService::new(&ports, &ports, &ports);
        let page = service
            .list_readlist_books(
                &context_with_libraries(["library-a"]),
                readlist_books_query("readlist-unordered", 0, 2),
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

    #[test]
    fn parse_comicrack_readlist_normalizes_request_at_application_boundary() {
        let request = parse_comicrack_readlist(
            br#"<ReadingList><Name>ReadList 1</Name><Books><Book Series="Series 1" Number="001" Volume="2" /></Books></ReadingList>"#,
        )
        .expect("valid ComicRack readlist should parse");

        assert_eq!(
            request,
            ComicRackReadListRequest {
                name: "ReadList 1".to_string(),
                books: vec![ComicRackReadListRequestBook {
                    series_candidates: vec!["Series 1".to_string(), "Series 1 (2)".to_string()],
                    number: "001".to_string(),
                }],
            },
        );
        assert_eq!(
            parse_comicrack_readlist(b"<ReadingList>"),
            Err(ComicRackReadListParseError::InvalidXml),
        );
        assert_eq!(
            parse_comicrack_readlist(
                br#"<ReadingList><Name>ReadList 1</Name><Books><Book Series= Number="1" /></Books></ReadingList>"#,
            ),
            Err(ComicRackReadListParseError::InvalidXml),
        );
        assert_eq!(
            parse_comicrack_readlist(
                br#"<ReadingList><Name>   </Name><Books><Book Series="Series 1" Number="1" /></Books></ReadingList>"#,
            ),
            Err(ComicRackReadListParseError::MissingName),
        );
        assert_eq!(
            parse_comicrack_readlist(br#"<ReadingList><Name>ReadList 1</Name></ReadingList>"#),
            Err(ComicRackReadListParseError::MissingBooks),
        );
        assert_eq!(
            parse_comicrack_readlist(
                br#"<ReadingList><Name>ReadList 1</Name><Books><Book Series="Series 1" /></Books></ReadingList>"#,
            ),
            Err(ComicRackReadListParseError::MissingBookIdentity),
        );
    }

    #[tokio::test]
    async fn comicrack_match_service_groups_candidates_and_marks_duplicate_names() {
        let mut ports = TestReadlistPorts::new();
        ports.readlists.push(readlist_record_with_ordered(
            "readlist-3",
            "Imported CBL",
            true,
        ));
        ports.comicrack_candidates = vec![
            comicrack_candidate("series-1", "Series 1", "book-1", "1", "Book 1"),
            comicrack_candidate("series-1", "Series 1", "book-2", "001", "Book 1 Variant"),
            comicrack_candidate("series-2", "Series 1 (2)", "book-3", "1", "Book 1 Volume 2"),
            comicrack_candidate("series-3", "Other Series", "book-4", "1", "Other Book"),
        ];

        let request = ComicRackReadListRequest {
            name: "imported cbl".to_string(),
            books: vec![
                ComicRackReadListRequestBook {
                    series_candidates: vec!["Series 1".to_string(), "Series 1 (2)".to_string()],
                    number: "0001".to_string(),
                },
                ComicRackReadListRequestBook {
                    series_candidates: vec!["Missing".to_string()],
                    number: "1".to_string(),
                },
            ],
        };
        let result = ComicRackReadListMatchService::new(&ports)
            .match_readlist(&request)
            .await
            .expect("ComicRack match should resolve");

        assert_eq!(
            result.error,
            Some(ComicRackReadListMatchError::DuplicateName)
        );
        assert_eq!(result.requests.len(), 2);
        assert_eq!(result.requests[0].request, request.books[0]);
        assert_eq!(result.requests[0].matches.len(), 2);
        assert_eq!(result.requests[0].matches[0].series.series_id, "series-1");
        assert_eq!(
            result.requests[0].matches[0]
                .books
                .iter()
                .map(|book| book.book_id.as_str())
                .collect::<Vec<_>>(),
            vec!["book-1", "book-2"],
        );
        assert_eq!(result.requests[0].matches[1].series.series_id, "series-2");
        assert!(result.requests[1].matches.is_empty());
    }

    fn context_with_libraries<const N: usize>(libraries: [&str; N]) -> DiscoveryQueryContext {
        DiscoveryQueryContext {
            user_id: None,
            is_admin: false,
            authorized_library_ids: Some(libraries.into_iter().map(LibraryId::from).collect()),
            restrictions: None,
        }
    }

    fn readlist_books_query(readlist_id: &str, page: usize, size: usize) -> ReadListBooksQuery {
        ReadListBooksQuery {
            readlist_id: readlist_id.to_string(),
            page,
            size,
            unpaged: false,
            library_ids: None,
            deleted: None,
            tags: None,
            read_statuses: None,
            media_statuses: None,
            authors: None,
        }
    }

    struct TestReadlistPorts {
        readlists: Vec<DiscoveryPersistedReadlistRecord>,
        readlist_books: HashMap<String, Vec<DiscoveryPersistedReadlistBookRecord>>,
        books: HashMap<String, BookReadModel>,
        book_resources: HashMap<String, PersistedBookResourceRecord>,
        search_hits: HashMap<String, Vec<ScoredSearchHit>>,
        comicrack_candidates: Vec<PersistedComicrackMatchCandidateRecord>,
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
                    ScoredSearchHit {
                        score: 2.0,
                        id: "readlist-2".to_string(),
                    },
                    ScoredSearchHit {
                        score: 1.0,
                        id: "readlist-1".to_string(),
                    },
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
                comicrack_candidates: Vec::new(),
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

    #[async_trait::async_trait]
    impl ReadlistProjectionPort for TestReadlistPorts {
        async fn load_persisted_readlists(
            &self,
        ) -> anyhow::Result<Vec<DiscoveryPersistedReadlistRecord>> {
            Ok(self.readlists.clone())
        }

        async fn load_persisted_readlist_detail(
            &self,
            readlist_id: &str,
        ) -> anyhow::Result<Option<DiscoveryPersistedReadlistRecord>> {
            Ok(self
                .readlists
                .iter()
                .find(|readlist| readlist.id == readlist_id)
                .cloned())
        }

        async fn load_persisted_readlist_book_rows(
            &self,
            readlist_id: &str,
        ) -> anyhow::Result<Vec<DiscoveryPersistedReadlistBookRecord>> {
            Ok(self
                .readlist_books
                .get(readlist_id)
                .cloned()
                .unwrap_or_default())
        }
    }

    #[async_trait::async_trait]
    impl ReadlistMutationPort for TestReadlistPorts {
        async fn persist_readlist_create(
            &self,
            readlist_id: &str,
            _name: &str,
            _summary: &str,
            _ordered: bool,
            _book_ids: &[String],
        ) -> anyhow::Result<()> {
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
        ) -> anyhow::Result<bool> {
            self.updated_readlists
                .lock()
                .expect("updated readlists lock should not be poisoned")
                .push(readlist_id.to_string());
            Ok(self
                .readlists
                .iter()
                .any(|readlist| readlist.id == readlist_id))
        }

        async fn delete_persisted_readlist(&self, readlist_id: &str) -> anyhow::Result<bool> {
            self.deleted_readlists
                .lock()
                .expect("deleted readlists lock should not be poisoned")
                .push(readlist_id.to_string());
            Ok(self
                .readlists
                .iter()
                .any(|readlist| readlist.id == readlist_id))
        }

        async fn upsert_readlist_search_document(&self, readlist_id: &str) -> anyhow::Result<bool> {
            self.search_upserts
                .lock()
                .expect("search upserts lock should not be poisoned")
                .push(readlist_id.to_string());
            Ok(true)
        }

        async fn delete_readlist_search_document(&self, readlist_id: &str) -> anyhow::Result<()> {
            self.search_deletes
                .lock()
                .expect("search deletes lock should not be poisoned")
                .push(readlist_id.to_string());
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl ReadlistBookPort for TestReadlistPorts {
        async fn load_persisted_book_resource(
            &self,
            book_id: &str,
        ) -> anyhow::Result<Option<PersistedBookResourceRecord>> {
            Ok(self.book_resources.get(book_id).cloned())
        }

        async fn load_persisted_book_detail(
            &self,
            book_id: &str,
            _user_id: Option<&str>,
        ) -> anyhow::Result<Option<BookReadModel>> {
            Ok(self.books.get(book_id).cloned())
        }
    }

    #[async_trait::async_trait]
    impl ReadlistSearchPort for TestReadlistPorts {
        async fn search_readlist_scored_ids(
            &self,
            query: &str,
            _limit: usize,
        ) -> anyhow::Result<Vec<ScoredSearchHit>> {
            Ok(self.search_hits.get(query).cloned().unwrap_or_default())
        }
    }

    #[async_trait::async_trait]
    impl ReadlistComicRackMatchPort for TestReadlistPorts {
        async fn load_persisted_readlists(
            &self,
        ) -> anyhow::Result<Vec<DiscoveryPersistedReadlistRecord>> {
            Ok(self.readlists.clone())
        }

        async fn load_comicrack_match_candidates(
            &self,
        ) -> anyhow::Result<Vec<PersistedComicrackMatchCandidateRecord>> {
            Ok(self.comicrack_candidates.clone())
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

    fn comicrack_candidate(
        series_id: &str,
        series_title: &str,
        book_id: &str,
        book_number: &str,
        book_title: &str,
    ) -> PersistedComicrackMatchCandidateRecord {
        PersistedComicrackMatchCandidateRecord {
            series_id: series_id.to_string(),
            series_title: series_title.to_string(),
            series_release_date: Some("2024-01-15".to_string()),
            book_id: book_id.to_string(),
            book_title: book_title.to_string(),
            book_number: book_number.to_string(),
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
            media_status: MediaStatus::Ready,
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
