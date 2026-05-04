use super::*;

pub fn parse_persisted_series_sort_modes(
    sorts: &[String],
    full_text_search: Option<&str>,
) -> Vec<PersistedSeriesSortMode> {
    let has_full_text_search = full_text_search
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some();
    let mut modes = sorts
        .iter()
        .filter_map(|sort| match sort.as_str() {
            "metadata.titleSort,asc" | "titleSort,asc" => Some(PersistedSeriesSortMode::TitleAsc),
            "metadata.titleSort,desc" | "titleSort,desc" => {
                Some(PersistedSeriesSortMode::TitleDesc)
            }
            "name,asc" => Some(PersistedSeriesSortMode::NameAsc),
            "name,desc" => Some(PersistedSeriesSortMode::NameDesc),
            "readDate,asc" => Some(PersistedSeriesSortMode::ReadDateAsc),
            "readDate,desc" => Some(PersistedSeriesSortMode::ReadDateDesc),
            "collection.number,asc" => Some(PersistedSeriesSortMode::CollectionNumberAsc),
            "collection.number,desc" => Some(PersistedSeriesSortMode::CollectionNumberDesc),
            "random,asc" | "random,desc" => Some(PersistedSeriesSortMode::Random),
            "createdDate,asc" | "created,asc" => Some(PersistedSeriesSortMode::CreatedAsc),
            "createdDate,desc" | "created,desc" => Some(PersistedSeriesSortMode::CreatedDesc),
            "lastModifiedDate,asc" | "lastModified,asc" => {
                Some(PersistedSeriesSortMode::LastModifiedAsc)
            }
            "lastModifiedDate,desc" | "lastModified,desc" => {
                Some(PersistedSeriesSortMode::LastModifiedDesc)
            }
            "booksMetadata.releaseDate,asc" => Some(PersistedSeriesSortMode::ReleaseDateAsc),
            "booksMetadata.releaseDate,desc" => Some(PersistedSeriesSortMode::ReleaseDateDesc),
            "booksCount,asc" => Some(PersistedSeriesSortMode::BooksCountAsc),
            "booksCount,desc" => Some(PersistedSeriesSortMode::BooksCountDesc),
            "relevance,asc" if has_full_text_search => Some(PersistedSeriesSortMode::RelevanceAsc),
            "relevance,desc" if has_full_text_search => {
                Some(PersistedSeriesSortMode::RelevanceDesc)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    modes.dedup();
    if modes.is_empty() && sorts.is_empty() && has_full_text_search {
        modes.push(PersistedSeriesSortMode::RelevanceAsc);
    }
    modes
}
