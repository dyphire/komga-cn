use super::*;

pub(crate) async fn load_persisted_ondeck_books(
    database_file: &FsPath,
    user_id: &str,
) -> Result<Vec<PersistedBookBrowseEntry>, String> {
    persisted_runtime_queries::load_persisted_ondeck_books(database_file, user_id).await
}

pub(crate) async fn load_persisted_duplicate_books(
    database_file: &FsPath,
) -> Result<Vec<PersistedBookBrowseEntry>, String> {
    persisted_runtime_queries::load_persisted_duplicate_books(database_file).await
}

pub(crate) async fn load_persisted_book_tags(
    database_file: &FsPath,
    scope: Option<&PersistedBookTagsScope>,
    authorized_library_ids: Option<&[String]>,
) -> Result<Vec<String>, String> {
    persisted_runtime_queries::load_persisted_book_tags(
        database_file,
        scope,
        authorized_library_ids,
    )
    .await
}

pub(crate) async fn load_persisted_author_names(
    database_file: &FsPath,
    search: &str,
    authorized_library_ids: Option<&[String]>,
) -> Result<Vec<String>, String> {
    authors_queries::load_persisted_author_names(database_file, search, authorized_library_ids)
        .await
}

pub(crate) async fn load_persisted_author_roles(
    database_file: &FsPath,
    authorized_library_ids: Option<&[String]>,
) -> Result<Vec<String>, String> {
    authors_queries::load_persisted_author_roles(database_file, authorized_library_ids).await
}

pub(crate) async fn load_persisted_authors_by_scope(
    database_file: &FsPath,
    scope: &PersistedAuthorsScope,
    authorized_library_ids: Option<&[String]>,
) -> Result<Vec<PersistedAuthorEntry>, String> {
    authors_queries::load_persisted_authors_by_scope(database_file, scope, authorized_library_ids)
        .await
}

pub(crate) async fn load_persisted_series_tags(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    facets_queries::load_persisted_series_tags(database_file, library_ids, collection_id).await
}

pub(crate) fn authors_v2_page_payload(
    authors: Vec<PersistedAuthorEntry>,
    page: usize,
    size: usize,
    unpaged: bool,
) -> Value {
    authors_queries::authors_v2_page_payload(authors, page, size, unpaged)
}

pub(crate) async fn load_persisted_genres(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    facets_queries::load_persisted_genres(database_file, library_ids, collection_id).await
}

pub(crate) async fn load_persisted_tags(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    facets_queries::load_persisted_tags(database_file, library_ids, collection_id).await
}

pub(crate) async fn load_persisted_languages(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    facets_queries::load_persisted_languages(database_file, library_ids, collection_id).await
}

pub(crate) async fn load_persisted_publishers(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    facets_queries::load_persisted_publishers(database_file, library_ids, collection_id).await
}

pub(crate) async fn load_persisted_age_ratings(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    facets_queries::load_persisted_age_ratings(database_file, library_ids, collection_id).await
}

pub(crate) async fn load_persisted_sharing_labels(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    facets_queries::load_persisted_sharing_labels(database_file, library_ids, collection_id).await
}

pub(crate) async fn load_persisted_series_release_dates(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    facets_queries::load_persisted_series_release_dates(database_file, library_ids, collection_id)
        .await
}

pub(crate) async fn load_persisted_series_page(
    database_file: &FsPath,
    context: &DiscoveryQueryContext,
    query: PersistedSeriesBrowseQuery,
) -> Result<PageEnvelope<PersistedSeriesSummary>, String> {
    series_queries::load_persisted_series_page(database_file, context, query).await
}

pub(crate) async fn load_persisted_alphabetical_groups(
    database_file: &FsPath,
    context: &DiscoveryQueryContext,
    filters: RuntimeSeriesFilters,
    full_text_search: Option<String>,
) -> Result<Vec<Value>, String> {
    series_queries::load_persisted_alphabetical_groups(
        database_file,
        context,
        filters,
        full_text_search,
    )
    .await
}

pub(crate) async fn remap_requested_library_ids_for_persisted(
    database_file: &FsPath,
    requested: Option<&Vec<String>>,
) -> Option<Vec<String>> {
    library_mappings::remap_requested_library_ids_for_persisted(database_file, requested).await
}

pub(crate) async fn load_collection_memberships(
    database_file: &FsPath,
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    library_mappings::load_collection_memberships(database_file).await
}

pub(crate) async fn load_readlist_memberships(
    database_file: &FsPath,
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    library_mappings::load_readlist_memberships(database_file).await
}

pub(crate) fn requested_query_values(query: &str, key: &str) -> Option<Vec<String>> {
    common_helpers::requested_query_values(query, key)
}

pub(crate) fn first_group_key(title: &str) -> String {
    common_helpers::first_group_key(title)
}

pub(crate) fn internal_error_response(error: String) -> Response {
    common_helpers::internal_error_response(error)
}

pub(crate) fn invalid_runtime_series_list_response(error: DiscoveryError) -> Response {
    common_helpers::invalid_runtime_series_list_response(error)
}

pub(crate) fn invalid_runtime_books_list_response(error: DiscoveryError) -> Response {
    common_helpers::invalid_runtime_books_list_response(error)
}

pub(crate) async fn persisted_utc_date_minus_days(
    database_file: &FsPath,
    days: i64,
) -> Result<Option<String>, String> {
    persisted_runtime_queries::persisted_utc_date_minus_days(database_file, days).await
}

pub(crate) async fn load_series_read_progress_counts(
    database_file: &FsPath,
    user_id: &str,
) -> Result<HashMap<String, (i64, i64)>, String> {
    persisted_runtime_queries::load_series_read_progress_counts(database_file, user_id).await
}

pub(crate) async fn load_series_total_book_counts(
    database_file: &FsPath,
) -> Result<HashMap<String, i64>, String> {
    persisted_runtime_queries::load_series_total_book_counts(database_file).await
}

pub(crate) async fn load_persisted_books_page(
    database_file: &FsPath,
    context: &DiscoveryQueryContext,
    query: PersistedBooksBrowseQuery,
) -> Result<PageEnvelope<BookReadModel>, String> {
    books_queries::load_persisted_books_page(database_file, context, query).await
}

pub(crate) async fn runtime_owned_books_list_response(
    headers: &HeaderMap,
    uri: &Uri,
    payload: Option<&Value>,
    full_text_search: Option<String>,
    auth_state: &DiscoveryAuthState,
    database_file: &FsPath,
    strict_runtime_shape: bool,
) -> Option<Response> {
    books_queries::runtime_owned_books_list_response(
        headers,
        uri,
        payload,
        full_text_search,
        auth_state,
        database_file,
        strict_runtime_shape,
    )
    .await
}

pub(crate) async fn runtime_owned_books_latest_response(
    headers: &HeaderMap,
    uri: &Uri,
    auth_state: &DiscoveryAuthState,
    database_file: &FsPath,
) -> Option<Response> {
    books_queries::runtime_owned_books_latest_response(headers, uri, auth_state, database_file)
        .await
}

pub(crate) async fn runtime_owned_series_list_response(
    headers: &HeaderMap,
    uri: &Uri,
    payload: Option<&Value>,
    full_text_search: Option<String>,
    auth_state: &DiscoveryAuthState,
    database_file: &FsPath,
    strict_runtime_shape: bool,
) -> Option<Response> {
    series_queries::runtime_owned_series_list_response(
        headers,
        uri,
        payload,
        full_text_search,
        auth_state,
        database_file,
        strict_runtime_shape,
    )
    .await
}

pub(crate) fn series_page_payload(
    page: PageEnvelope<PersistedSeriesSummary>,
    paged: bool,
    sorted: bool,
) -> Value {
    series_queries::series_page_payload(page, paged, sorted)
}
