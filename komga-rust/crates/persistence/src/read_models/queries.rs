use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext, LibraryReadModel};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

#[path = "queries/book_detail.rs"]
pub(super) mod book_detail;
#[path = "queries/books.rs"]
pub(super) mod books;
#[path = "queries/books_media.rs"]
pub(super) mod books_media;
#[path = "queries/libraries.rs"]
pub(super) mod libraries;
#[path = "queries/readlists.rs"]
pub(super) mod readlists;
#[path = "queries/series.rs"]
pub(super) mod series;
use super::filters::{SqlxWhereState, append_in_clause_sqlx, effective_library_ids};

#[derive(sqlx::FromRow)]
struct SqlxLibraryRow {
    id: String,
    name: String,
    root: String,
}

impl From<SqlxLibraryRow> for LibraryReadModel {
    fn from(value: SqlxLibraryRow) -> Self {
        Self {
            id: value.id,
            name: value.name,
            root: value.root,
        }
    }
}

pub(in crate::read_models) async fn list_libraries_sqlx(
    pool: SqlitePool,
    context: &DiscoveryQueryContext,
) -> Result<Vec<LibraryReadModel>, DiscoveryError> {
    let allowed = effective_library_ids(context, None);
    if allowed.as_ref().is_some_and(Vec::is_empty) {
        return Ok(vec![]);
    }

    let mut builder = QueryBuilder::<Sqlite>::new(
        "SELECT id, name, root \
               FROM libraries",
    );
    let mut state = SqlxWhereState::default();
    if let Some(allowed_ids) = allowed.as_ref() {
        append_in_clause_sqlx("id", allowed_ids, &mut builder, &mut state);
    }
    builder.push(" ORDER BY name COLLATE NOCASE ASC");

    let rows = builder
        .build_query_as::<SqlxLibraryRow>()
        .fetch_all(&pool)
        .await
        .map_err(map_sqlx_error)?;

    Ok(rows.into_iter().map(LibraryReadModel::from).collect())
}

pub(super) fn map_sqlx_error(error: sqlx::Error) -> DiscoveryError {
    DiscoveryError::Persistence(error.to_string())
}

pub(super) fn parse_labels(raw: &str) -> Vec<String> {
    if raw.trim().is_empty() {
        return vec![];
    }

    raw.split(',')
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
    #[test]
    fn libraries_query_cluster_exposes_sqlx_entrypoints() {
        let _ = super::libraries::list_persisted_libraries_sqlx;
        let _ = super::libraries::get_persisted_library_sqlx;
    }

    #[test]
    fn series_query_cluster_exposes_sqlx_entrypoints() {
        let _ = super::series::list_series_sqlx;
        let _ = super::series::get_series_detail_sqlx;
        let _ = super::series::resolve_series_resource_sqlx;
    }

    #[test]
    fn books_media_query_cluster_exposes_sqlx_entrypoints() {
        let _ = super::books_media::list_books_sqlx;
        let _ = super::books_media::list_books_latest_sqlx;
        let _ = super::books_media::get_book_detail_sqlx;
        let _ = super::books_media::get_book_sibling_previous_sqlx;
        let _ = super::books_media::get_book_sibling_next_sqlx;
        let _ = super::books_media::resolve_book_resource_sqlx;
    }

    #[test]
    fn readlists_collections_query_cluster_exposes_sqlx_entrypoints() {
        let _ = super::readlists::list_readlists_sqlx;
        let _ = super::readlists::get_readlist_detail_sqlx;
        let _ = super::readlists::list_readlist_books_sqlx;
        let _ = super::readlists::list_book_readlists_sqlx;
        let _ = super::readlists::get_readlist_book_sibling_sqlx;
        let _ = super::readlists::list_series_collections_sqlx;
    }
}
