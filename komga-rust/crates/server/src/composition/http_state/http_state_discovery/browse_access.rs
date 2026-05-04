use async_trait::async_trait;
use komga_application::discovery::{
    BookReadModel, BookTagScope, BooksBrowseQuery, BooksFeedQuery, DiscoveryListService,
    SeriesBrowseQuery, SeriesReadModel,
};
use komga_domain::discovery::PageEnvelope;
use komga_domain::discovery::{
    AgeRatingCondition, BookCondition, BookFilter, BookPosterCondition, BookSort,
    BookValueCondition, DateCondition, DiscoveryError, DiscoveryQueryContext, InclusionCondition,
    NumberCondition, ReadStatusCondition, SeriesCondition, SeriesFilter, SeriesSort,
    SeriesStatusCondition, SeriesValueCondition, StringCondition,
};
use komga_interfaces::discovery::persisted::books_queries::load_persisted_books_page;
use komga_interfaces::discovery::persisted::models::{
    BooksFilterCriteria, PersistedBookTagsScope, PersistedBooksBrowseQuery, PersistedBooksSortMode,
    PersistedSeriesBrowseQuery, PersistedSeriesSortMode, PersistedSeriesSummary,
    SeriesFilterCriteria,
};
use komga_interfaces::discovery::persisted::series_queries::{
    load_persisted_alphabetical_groups, load_persisted_series_page,
};
use komga_interfaces::discovery_auth::context::{
    DiscoveryQueryContext as InterfacesDiscoveryQueryContext,
    QueryRestrictions as InterfacesQueryRestrictions,
};
use komga_interfaces::discovery_auth::principal::AgeRestrictionKind as InterfacesAgeRestrictionKind;
use komga_interfaces::state::PersistedDiscoveryListDataSource;

pub struct PersistedDiscoveryListAdapter {
    persisted: Box<dyn PersistedDiscoveryListDataSource>,
}

impl PersistedDiscoveryListAdapter {
    pub fn new(persisted: Box<dyn PersistedDiscoveryListDataSource>) -> Self {
        Self { persisted }
    }
}

fn to_interfaces_context(context: &DiscoveryQueryContext) -> InterfacesDiscoveryQueryContext {
    InterfacesDiscoveryQueryContext {
        user_id: context.user_id.as_ref().map(|id| id.as_str().to_string()),
        is_admin: context.is_admin,
        authorized_library_ids: context
            .authorized_library_ids
            .as_ref()
            .map(|ids| ids.iter().map(|id| id.as_str().to_string()).collect()),
        restrictions: context
            .restrictions
            .as_ref()
            .map(|r| InterfacesQueryRestrictions {
                age: r.age,
                age_restriction: r.age_restriction.map(|kind| match kind {
                    komga_domain::discovery::AgeRestrictionKind::AllowOnly => {
                        InterfacesAgeRestrictionKind::AllowOnly
                    }
                    komga_domain::discovery::AgeRestrictionKind::Exclude => {
                        InterfacesAgeRestrictionKind::Exclude
                    }
                }),
                labels_allow: r.labels_allow.clone(),
                labels_exclude: r.labels_exclude.clone(),
            }),
    }
}

fn series_sort_to_persisted(sort: &[SeriesSort]) -> Vec<PersistedSeriesSortMode> {
    if sort.is_empty() {
        return vec![];
    }
    sort.iter()
        .map(|s| match s {
            SeriesSort::MetadataTitleSortAsc => PersistedSeriesSortMode::TitleAsc,
            SeriesSort::MetadataTitleSortDesc => PersistedSeriesSortMode::TitleDesc,
            SeriesSort::NameAsc => PersistedSeriesSortMode::NameAsc,
            SeriesSort::NameDesc => PersistedSeriesSortMode::NameDesc,
            SeriesSort::CreatedDateAsc => PersistedSeriesSortMode::CreatedAsc,
            SeriesSort::CreatedDateDesc => PersistedSeriesSortMode::CreatedDesc,
            SeriesSort::LastModifiedDateAsc => PersistedSeriesSortMode::LastModifiedAsc,
            SeriesSort::LastModifiedDateDesc => PersistedSeriesSortMode::LastModifiedDesc,
            SeriesSort::ReleaseDateAsc => PersistedSeriesSortMode::ReleaseDateAsc,
            SeriesSort::ReleaseDateDesc => PersistedSeriesSortMode::ReleaseDateDesc,
            SeriesSort::BooksCountAsc => PersistedSeriesSortMode::BooksCountAsc,
            SeriesSort::BooksCountDesc => PersistedSeriesSortMode::BooksCountDesc,
            SeriesSort::CollectionNumberAsc => PersistedSeriesSortMode::CollectionNumberAsc,
            SeriesSort::CollectionNumberDesc => PersistedSeriesSortMode::CollectionNumberDesc,
            SeriesSort::ReadDateAsc => PersistedSeriesSortMode::ReadDateAsc,
            SeriesSort::ReadDateDesc => PersistedSeriesSortMode::ReadDateDesc,
            SeriesSort::Random => PersistedSeriesSortMode::Random,
            SeriesSort::RelevanceAsc => PersistedSeriesSortMode::RelevanceAsc,
            SeriesSort::RelevanceDesc => PersistedSeriesSortMode::RelevanceDesc,
        })
        .collect()
}

fn extract_include_values<T: Clone>(condition: &InclusionCondition<T>) -> Option<Vec<T>> {
    match condition {
        InclusionCondition::Include(v) => Some(v.clone()),
        InclusionCondition::Exclude(_) => None,
    }
}

fn series_filter_to_criteria(filter: &SeriesFilter) -> SeriesFilterCriteria {
    let mut criteria = SeriesFilterCriteria::default();

    let Some(condition) = &filter.condition else {
        return criteria;
    };

    flatten_series_condition(condition, &mut criteria);
    criteria
}

fn flatten_series_condition(condition: &SeriesCondition, criteria: &mut SeriesFilterCriteria) {
    match condition {
        SeriesCondition::Value(value) => apply_series_value_condition(value, criteria),
        SeriesCondition::Composite(composite) => {
            for child in &composite.conditions {
                flatten_series_condition(child, criteria);
            }
        }
    }
}

fn apply_series_value_condition(
    condition: &SeriesValueCondition,
    criteria: &mut SeriesFilterCriteria,
) {
    match condition {
        SeriesValueCondition::LibraryId(inc) => {
            criteria.library_ids = extract_include_values(inc)
                .map(|ids| ids.into_iter().map(|id| id.as_str().to_string()).collect());
        }
        SeriesValueCondition::CollectionId(inc) => {
            criteria.collection_ids = extract_include_values(inc)
                .map(|ids| ids.into_iter().map(|id| id.as_str().to_string()).collect());
        }
        SeriesValueCondition::Deleted(val) => {
            criteria.deleted = Some(*val);
        }
        SeriesValueCondition::OneShot(val) => {
            criteria.oneshot = Some(*val);
        }
        SeriesValueCondition::Title(StringCondition::Contains(InclusionCondition::Include(v))) => {
            criteria.titles_contains = Some(v.clone());
        }
        SeriesValueCondition::Title(StringCondition::Exact(InclusionCondition::Include(v))) => {
            criteria.titles = Some(v.clone());
        }
        SeriesValueCondition::Title(StringCondition::Exact(InclusionCondition::Exclude(v))) => {
            criteria.titles_excluded = Some(v.clone());
        }
        SeriesValueCondition::Title(StringCondition::StartsWith(InclusionCondition::Include(
            v,
        ))) => {
            criteria.titles_begins_with = Some(v.clone());
        }
        SeriesValueCondition::Title(StringCondition::EndsWith(InclusionCondition::Include(v))) => {
            criteria.titles_ends_with = Some(v.clone());
        }
        SeriesValueCondition::TitleSort(StringCondition::Contains(
            InclusionCondition::Include(v),
        )) => {
            criteria.title_sorts_contains = Some(v.clone());
        }
        SeriesValueCondition::TitleSort(StringCondition::Exact(InclusionCondition::Include(v))) => {
            criteria.title_sorts = Some(v.clone());
        }
        SeriesValueCondition::TitleSort(StringCondition::Exact(InclusionCondition::Exclude(v))) => {
            criteria.title_sorts_excluded = Some(v.clone());
        }
        SeriesValueCondition::Genre(StringCondition::Contains(InclusionCondition::Include(v))) => {
            criteria.genres = Some(v.clone());
        }
        SeriesValueCondition::Genre(StringCondition::Exact(InclusionCondition::Include(v))) => {
            criteria.genres = Some(v.clone());
        }
        SeriesValueCondition::Genre(StringCondition::Contains(InclusionCondition::Exclude(v)))
        | SeriesValueCondition::Genre(StringCondition::Exact(InclusionCondition::Exclude(v))) => {
            criteria.genres_excluded = Some(v.clone());
        }
        SeriesValueCondition::Genre(StringCondition::IsEmpty) => {
            criteria.genres_null = Some(true);
        }
        SeriesValueCondition::Genre(StringCondition::IsNotEmpty) => {
            criteria.genres_null = Some(false);
        }
        SeriesValueCondition::Tag(StringCondition::Contains(InclusionCondition::Include(v))) => {
            criteria.tags = Some(v.clone());
        }
        SeriesValueCondition::Tag(StringCondition::Exact(InclusionCondition::Include(v))) => {
            criteria.tags = Some(v.clone());
        }
        SeriesValueCondition::Tag(StringCondition::Contains(InclusionCondition::Exclude(v)))
        | SeriesValueCondition::Tag(StringCondition::Exact(InclusionCondition::Exclude(v))) => {
            criteria.tags_excluded = Some(v.clone());
        }
        SeriesValueCondition::Tag(StringCondition::IsEmpty) => {
            criteria.tags_null = Some(true);
        }
        SeriesValueCondition::Tag(StringCondition::IsNotEmpty) => {
            criteria.tags_null = Some(false);
        }
        SeriesValueCondition::Language(InclusionCondition::Include(v)) => {
            criteria.languages = Some(v.clone());
        }
        SeriesValueCondition::Language(InclusionCondition::Exclude(v)) => {
            criteria.languages_excluded = Some(v.clone());
        }
        SeriesValueCondition::Publisher(InclusionCondition::Include(v)) => {
            criteria.publishers = Some(v.clone());
        }
        SeriesValueCondition::Publisher(InclusionCondition::Exclude(v)) => {
            criteria.publishers_excluded = Some(v.clone());
        }
        SeriesValueCondition::AgeRating(AgeRatingCondition::Exact(
            InclusionCondition::Include(v),
        )) => {
            criteria.age_ratings = Some(v.clone());
        }
        SeriesValueCondition::AgeRating(AgeRatingCondition::ExactOrEmpty(v)) => {
            criteria.age_ratings_or_empty = Some(v.clone());
        }
        SeriesValueCondition::AgeRating(AgeRatingCondition::Exact(
            InclusionCondition::Exclude(v),
        )) => {
            criteria.age_ratings_excluded = Some(v.clone());
        }
        SeriesValueCondition::AgeRating(AgeRatingCondition::GreaterThan(value)) => {
            criteria.age_rating_gt = Some(*value);
        }
        SeriesValueCondition::AgeRating(AgeRatingCondition::LessThan(value)) => {
            criteria.age_rating_lt = Some(*value);
        }
        SeriesValueCondition::AgeRating(AgeRatingCondition::IsEmpty) => {
            criteria.age_ratings_null = Some(true);
        }
        SeriesValueCondition::AgeRating(AgeRatingCondition::IsNotEmpty) => {
            criteria.age_ratings_null = Some(false);
        }
        SeriesValueCondition::ReleaseDate(DateCondition::Exact(InclusionCondition::Include(v))) => {
            criteria.release_dates = Some(v.clone());
        }
        SeriesValueCondition::ReleaseDate(DateCondition::Exact(InclusionCondition::Exclude(v))) => {
            criteria.release_dates_excluded = Some(v.clone());
        }
        SeriesValueCondition::ReleaseDate(DateCondition::Before(v)) => {
            criteria.release_date_lt = Some(v.clone());
        }
        SeriesValueCondition::ReleaseDate(DateCondition::After(v)) => {
            criteria.release_date_gt = Some(v.clone());
        }
        SeriesValueCondition::ReleaseDate(DateCondition::IsEmpty) => {
            criteria.release_dates_null = Some(true);
        }
        SeriesValueCondition::ReleaseDate(DateCondition::IsNotEmpty) => {
            criteria.release_dates_null = Some(false);
        }
        SeriesValueCondition::ReleaseDate(DateCondition::StartsWith(
            InclusionCondition::Include(v),
        )) => {
            criteria.release_date_begins_with = Some(v.clone());
        }
        SeriesValueCondition::ReleaseDate(DateCondition::StartsWith(
            InclusionCondition::Exclude(v),
        )) => {
            criteria.release_date_begins_with_excluded = Some(v.clone());
        }
        SeriesValueCondition::ReleaseDate(DateCondition::EndsWith(
            InclusionCondition::Include(v),
        )) => {
            criteria.release_date_ends_with = Some(v.clone());
        }
        SeriesValueCondition::ReleaseDate(DateCondition::EndsWith(
            InclusionCondition::Exclude(v),
        )) => {
            criteria.release_date_ends_with_excluded = Some(v.clone());
        }
        SeriesValueCondition::ReleaseDate(DateCondition::Contains(
            InclusionCondition::Exclude(v),
        )) => {
            criteria.release_date_contains_excluded = Some(v.clone());
        }
        SeriesValueCondition::ReleaseDate(DateCondition::WithinLastDays(days)) => {
            criteria.release_date_in_last_days = Some(*days);
        }
        SeriesValueCondition::ReleaseDate(DateCondition::OutsideLastDays(days)) => {
            criteria.release_date_not_in_last_days = Some(*days);
        }
        SeriesValueCondition::SharingLabel(StringCondition::Contains(
            InclusionCondition::Include(v),
        )) => {
            criteria.sharing_labels_contains = Some(v.clone());
        }
        SeriesValueCondition::SharingLabel(StringCondition::Exact(
            InclusionCondition::Include(v),
        )) => {
            criteria.sharing_labels = Some(v.clone());
        }
        SeriesValueCondition::SharingLabel(StringCondition::Contains(
            InclusionCondition::Exclude(v),
        ))
        | SeriesValueCondition::SharingLabel(StringCondition::Exact(
            InclusionCondition::Exclude(v),
        )) => {
            criteria.sharing_labels_excluded = Some(v.clone());
        }
        SeriesValueCondition::SharingLabel(StringCondition::IsEmpty) => {
            criteria.sharing_labels_null = Some(true);
        }
        SeriesValueCondition::SharingLabel(StringCondition::IsNotEmpty) => {
            criteria.sharing_labels_null = Some(false);
        }
        SeriesValueCondition::ReadStatus(ReadStatusCondition::Include(v)) => {
            criteria.read_statuses = Some(v.clone());
        }
        SeriesValueCondition::ReadStatus(ReadStatusCondition::Exclude(v)) => {
            criteria.read_statuses_excluded = Some(v.clone());
        }
        SeriesValueCondition::SeriesStatus(SeriesStatusCondition::Include(v)) => {
            criteria.series_statuses = Some(v.clone());
        }
        SeriesValueCondition::SeriesStatus(SeriesStatusCondition::Exclude(v)) => {
            criteria.series_statuses_excluded = Some(v.clone());
        }
        SeriesValueCondition::Complete(val) => {
            criteria.complete = Some(*val);
        }
        SeriesValueCondition::Author(StringCondition::Contains(InclusionCondition::Include(v))) => {
            criteria.authors_contains = Some(v.clone());
        }
        SeriesValueCondition::Author(StringCondition::Exact(InclusionCondition::Include(v))) => {
            criteria.authors = Some(v.clone());
        }
        SeriesValueCondition::Author(StringCondition::Exact(InclusionCondition::Exclude(v))) => {
            criteria.authors_excluded = Some(v.clone());
        }
        SeriesValueCondition::ExcludeNewlyAdded(value) => {
            criteria.exclude_newly_added = *value;
        }
        // Unsupported variants are silently skipped; the persisted adapter handles
        // them through its composite fallthrough if needed.
        _ => {}
    }
}

fn persisted_series_to_read_model(series: &PersistedSeriesSummary) -> SeriesReadModel {
    SeriesReadModel {
        id: series.id.clone(),
        name: series.name.clone(),
        title: series.title.clone(),
    }
}

fn book_sort_to_persisted(sort: &[BookSort]) -> Vec<PersistedBooksSortMode> {
    if sort.is_empty() {
        return vec![];
    }
    sort.iter()
        .map(|s| match s {
            BookSort::MetadataTitleAsc => PersistedBooksSortMode::TitleAsc,
            BookSort::CreatedDateDesc => PersistedBooksSortMode::CreatedDateDesc,
            BookSort::LastModifiedDateDesc => PersistedBooksSortMode::LastModifiedDateDesc,
            BookSort::ReadProgressLastModifiedAsc => {
                PersistedBooksSortMode::ReadProgressLastModifiedDateAsc
            }
            BookSort::ReadProgressLastModifiedDesc => {
                PersistedBooksSortMode::ReadProgressLastModifiedDateDesc
            }
            BookSort::ReadProgressReadDateAsc => PersistedBooksSortMode::ReadProgressReadDateAsc,
            BookSort::ReadProgressReadDateDesc => PersistedBooksSortMode::ReadProgressReadDateDesc,
            BookSort::ReleaseDateDesc => PersistedBooksSortMode::ReleaseDateDesc,
            BookSort::NumberSortAsc => PersistedBooksSortMode::NumberSortAsc,
            BookSort::SeriesIdAsc => PersistedBooksSortMode::SeriesIdAsc,
            BookSort::RelevanceAsc => PersistedBooksSortMode::RelevanceAsc,
            BookSort::RelevanceDesc => PersistedBooksSortMode::RelevanceDesc,
            _ => PersistedBooksSortMode::TitleAsc,
        })
        .collect()
}

fn book_filter_to_criteria(filter: &BookFilter) -> BooksFilterCriteria {
    let mut criteria = BooksFilterCriteria::default();
    if let Some(condition) = &filter.condition {
        flatten_book_condition(condition, &mut criteria);
    }
    criteria
}

fn flatten_book_condition(condition: &BookCondition, criteria: &mut BooksFilterCriteria) {
    match condition {
        BookCondition::Value(value) => apply_book_value_condition(value, criteria),
        BookCondition::Composite(composite) => {
            for child in &composite.conditions {
                flatten_book_condition(child, criteria);
            }
        }
    }
}

fn parsed_numbers(values: &[String]) -> Vec<f64> {
    values
        .iter()
        .filter_map(|value| value.parse::<f64>().ok())
        .collect()
}

fn apply_book_poster_condition(
    condition: &BookPosterCondition,
    criteria: &mut BooksFilterCriteria,
    excluded: bool,
) {
    if let Some(thumbnail_type) = condition.thumbnail_type.as_ref() {
        if excluded {
            criteria.poster_types_excluded = Some(vec![thumbnail_type.clone()]);
        } else {
            criteria.poster_types = Some(vec![thumbnail_type.clone()]);
        }
    }
    if let Some(selected) = condition.selected {
        if excluded {
            criteria.poster_selected_excluded = Some(selected);
        } else {
            criteria.poster_selected = Some(selected);
        }
    }
}

fn apply_book_value_condition(condition: &BookValueCondition, criteria: &mut BooksFilterCriteria) {
    match condition {
        BookValueCondition::LibraryId(inc) => match inc {
            InclusionCondition::Include(ids) => {
                criteria.library_ids = Some(ids.iter().map(|id| id.as_str().to_string()).collect());
            }
            InclusionCondition::Exclude(_) => {}
        },
        BookValueCondition::SeriesId(inc) => match inc {
            InclusionCondition::Include(ids) => {
                criteria.series_ids = Some(ids.iter().map(|id| id.as_str().to_string()).collect());
            }
            InclusionCondition::Exclude(ids) => {
                criteria.series_ids_excluded =
                    Some(ids.iter().map(|id| id.as_str().to_string()).collect());
            }
        },
        BookValueCondition::ReadListId(inc) => match inc {
            InclusionCondition::Include(ids) => {
                criteria.read_list_ids =
                    Some(ids.iter().map(|id| id.as_str().to_string()).collect());
            }
            InclusionCondition::Exclude(ids) => {
                criteria.read_list_ids_excluded =
                    Some(ids.iter().map(|id| id.as_str().to_string()).collect());
            }
        },
        BookValueCondition::Deleted(val) => {
            criteria.deleted = Some(*val);
        }
        BookValueCondition::OneShot(val) => {
            criteria.oneshot = Some(*val);
        }
        BookValueCondition::Title(StringCondition::Contains(InclusionCondition::Include(v))) => {
            criteria.titles_contains = Some(v.clone());
        }
        BookValueCondition::Title(StringCondition::Contains(InclusionCondition::Exclude(v))) => {
            criteria.titles_contains_excluded = Some(v.clone());
        }
        BookValueCondition::Title(StringCondition::Exact(InclusionCondition::Include(v))) => {
            criteria.titles = Some(v.clone());
        }
        BookValueCondition::Title(StringCondition::Exact(InclusionCondition::Exclude(v))) => {
            criteria.titles_excluded = Some(v.clone());
        }
        BookValueCondition::Title(StringCondition::StartsWith(InclusionCondition::Include(v))) => {
            criteria.titles_begins_with = Some(v.clone());
        }
        BookValueCondition::Title(StringCondition::StartsWith(InclusionCondition::Exclude(v))) => {
            criteria.titles_begins_with_excluded = Some(v.clone());
        }
        BookValueCondition::Title(StringCondition::EndsWith(InclusionCondition::Include(v))) => {
            criteria.titles_ends_with = Some(v.clone());
        }
        BookValueCondition::Title(StringCondition::EndsWith(InclusionCondition::Exclude(v))) => {
            criteria.titles_ends_with_excluded = Some(v.clone());
        }
        BookValueCondition::Tag(StringCondition::Contains(InclusionCondition::Include(v)))
        | BookValueCondition::Tag(StringCondition::Exact(InclusionCondition::Include(v))) => {
            criteria.tags = Some(v.clone());
        }
        BookValueCondition::Tag(StringCondition::Contains(InclusionCondition::Exclude(v)))
        | BookValueCondition::Tag(StringCondition::Exact(InclusionCondition::Exclude(v))) => {
            criteria.tags_excluded = Some(v.clone());
        }
        BookValueCondition::Tag(StringCondition::IsEmpty) => {
            criteria.tags_null = Some(true);
        }
        BookValueCondition::Tag(StringCondition::IsNotEmpty) => {
            criteria.tags_null = Some(false);
        }
        BookValueCondition::Genre(StringCondition::Contains(InclusionCondition::Include(v)))
        | BookValueCondition::Genre(StringCondition::Exact(InclusionCondition::Include(v))) => {
            criteria.genres = Some(v.clone());
        }
        BookValueCondition::Genre(StringCondition::Contains(InclusionCondition::Exclude(v)))
        | BookValueCondition::Genre(StringCondition::Exact(InclusionCondition::Exclude(v))) => {
            criteria.genres_excluded = Some(v.clone());
        }
        BookValueCondition::Genre(StringCondition::IsEmpty) => {
            criteria.genres_null = Some(true);
        }
        BookValueCondition::Genre(StringCondition::IsNotEmpty) => {
            criteria.genres_null = Some(false);
        }
        BookValueCondition::Language(InclusionCondition::Include(v)) => {
            criteria.languages = Some(v.clone());
        }
        BookValueCondition::Language(InclusionCondition::Exclude(v)) => {
            criteria.languages_excluded = Some(v.clone());
        }
        BookValueCondition::Publisher(InclusionCondition::Include(v)) => {
            criteria.publishers = Some(v.clone());
        }
        BookValueCondition::Publisher(InclusionCondition::Exclude(v)) => {
            criteria.publishers_excluded = Some(v.clone());
        }
        BookValueCondition::AgeRating(InclusionCondition::Include(v)) => {
            criteria.age_ratings = Some(v.clone());
        }
        BookValueCondition::AgeRating(InclusionCondition::Exclude(v)) => {
            criteria.age_ratings_excluded = Some(v.clone());
        }
        BookValueCondition::ReadStatus(ReadStatusCondition::Include(v)) => {
            criteria.read_statuses = Some(v.clone());
        }
        BookValueCondition::ReadStatus(ReadStatusCondition::Exclude(v)) => {
            criteria.read_statuses_excluded = Some(v.clone());
        }
        BookValueCondition::MediaProfile(InclusionCondition::Include(v)) => {
            criteria.media_profiles = Some(v.clone());
        }
        BookValueCondition::MediaProfile(InclusionCondition::Exclude(v)) => {
            criteria.media_profiles_excluded = Some(v.clone());
        }
        BookValueCondition::MediaStatus(InclusionCondition::Include(v)) => {
            criteria.media_statuses = Some(v.clone());
        }
        BookValueCondition::MediaStatus(InclusionCondition::Exclude(v)) => {
            criteria.media_statuses_excluded = Some(v.clone());
        }
        BookValueCondition::Author(StringCondition::Contains(InclusionCondition::Include(v))) => {
            criteria.authors_contains = Some(v.clone());
        }
        BookValueCondition::Author(StringCondition::Exact(InclusionCondition::Include(v))) => {
            criteria.authors = Some(v.clone());
        }
        BookValueCondition::Author(StringCondition::Exact(InclusionCondition::Exclude(v))) => {
            criteria.authors_excluded = Some(v.clone());
        }
        BookValueCondition::Poster(InclusionCondition::Include(v)) => {
            if let Some(condition) = v.first() {
                apply_book_poster_condition(condition, criteria, false);
            }
        }
        BookValueCondition::Poster(InclusionCondition::Exclude(v)) => {
            if let Some(condition) = v.first() {
                apply_book_poster_condition(condition, criteria, true);
            }
        }
        BookValueCondition::NumberSort(NumberCondition::Exact(InclusionCondition::Include(v))) => {
            criteria.number_sorts = Some(parsed_numbers(v));
        }
        BookValueCondition::NumberSort(NumberCondition::Exact(InclusionCondition::Exclude(v))) => {
            criteria.number_sorts_excluded = Some(parsed_numbers(v));
        }
        BookValueCondition::NumberSort(NumberCondition::GreaterThan(v)) => {
            criteria.number_sort_gt = v.parse::<f64>().ok();
        }
        BookValueCondition::NumberSort(NumberCondition::LessThan(v)) => {
            criteria.number_sort_lt = v.parse::<f64>().ok();
        }
        BookValueCondition::ReleaseDate(DateCondition::After(v)) => {
            criteria.release_date_gt = Some(v.clone());
        }
        BookValueCondition::ReleaseDate(DateCondition::Before(v)) => {
            criteria.release_date_lt = Some(v.clone());
        }
        BookValueCondition::ReleaseDate(DateCondition::Exact(InclusionCondition::Include(v))) => {
            criteria.release_dates = Some(v.clone());
        }
        BookValueCondition::ReleaseDate(DateCondition::Exact(InclusionCondition::Exclude(v))) => {
            criteria.release_dates_excluded = Some(v.clone());
        }
        BookValueCondition::ReleaseDate(DateCondition::IsEmpty) => {
            criteria.release_dates_null = Some(true);
        }
        BookValueCondition::ReleaseDate(DateCondition::IsNotEmpty) => {
            criteria.release_dates_null = Some(false);
        }
        BookValueCondition::ReleaseDate(DateCondition::StartsWith(
            InclusionCondition::Include(v),
        )) => {
            criteria.release_date_begins_with = Some(v.clone());
        }
        BookValueCondition::ReleaseDate(DateCondition::StartsWith(
            InclusionCondition::Exclude(v),
        )) => {
            criteria.release_date_begins_with_excluded = Some(v.clone());
        }
        BookValueCondition::ReleaseDate(DateCondition::EndsWith(InclusionCondition::Include(
            v,
        ))) => {
            criteria.release_date_ends_with = Some(v.clone());
        }
        BookValueCondition::ReleaseDate(DateCondition::EndsWith(InclusionCondition::Exclude(
            v,
        ))) => {
            criteria.release_date_ends_with_excluded = Some(v.clone());
        }
        BookValueCondition::ReleaseDate(DateCondition::Contains(InclusionCondition::Exclude(
            v,
        ))) => {
            criteria.release_date_contains_excluded = Some(v.clone());
        }
        BookValueCondition::ReleaseDate(DateCondition::WithinLastDays(days)) => {
            criteria.release_date_in_last_days = Some(*days);
        }
        BookValueCondition::ReleaseDate(DateCondition::OutsideLastDays(days)) => {
            criteria.release_date_not_in_last_days = Some(*days);
        }
        // Unsupported variants silently skipped
        _ => {}
    }
}

#[async_trait]
impl DiscoveryListService for PersistedDiscoveryListAdapter {
    async fn list_series(
        &self,
        context: &DiscoveryQueryContext,
        query: SeriesBrowseQuery,
    ) -> Result<PageEnvelope<SeriesReadModel>, DiscoveryError> {
        let interfaces_context = to_interfaces_context(context);
        let sort_modes = series_sort_to_persisted(&query.sort);
        let criteria = series_filter_to_criteria(&query.filter);

        let persisted_query = PersistedSeriesBrowseQuery::from_filters(
            criteria,
            query.search,
            query.page,
            query.size,
            query.unpaged,
            sort_modes,
        );

        let page =
            load_persisted_series_page(&*self.persisted, &interfaces_context, persisted_query)
                .await
                .map_err(DiscoveryError::Persistence)?;

        Ok(PageEnvelope {
            content: page
                .content
                .iter()
                .map(persisted_series_to_read_model)
                .collect(),
            page: page.page,
            size: page.size,
            total_elements: page.total_elements,
            total_pages: page.total_pages,
        })
    }

    async fn list_books(
        &self,
        context: &DiscoveryQueryContext,
        query: BooksBrowseQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        let interfaces_context = to_interfaces_context(context);
        let sort_modes = book_sort_to_persisted(&query.sort);
        let criteria = book_filter_to_criteria(&query.filter);

        let persisted_query = PersistedBooksBrowseQuery::from_filters(
            criteria,
            query.search,
            query.page,
            query.size,
            query.unpaged,
            sort_modes,
        );

        load_persisted_books_page(&*self.persisted, &interfaces_context, persisted_query)
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_books_latest(
        &self,
        context: &DiscoveryQueryContext,
        query: BooksFeedQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        let interfaces_context = to_interfaces_context(context);

        let persisted_query = PersistedBooksBrowseQuery::from_filters(
            BooksFilterCriteria {
                library_ids: query.library_ids,
                ..BooksFilterCriteria::default()
            },
            None,
            query.page,
            query.size,
            query.unpaged,
            vec![PersistedBooksSortMode::LastModifiedDateDesc],
        );

        load_persisted_books_page(&*self.persisted, &interfaces_context, persisted_query)
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_series_alphabetical_groups(
        &self,
        context: &DiscoveryQueryContext,
        filter: SeriesFilter,
        search: Option<String>,
    ) -> Result<Vec<serde_json::Value>, DiscoveryError> {
        let interfaces_context = to_interfaces_context(context);
        let criteria = series_filter_to_criteria(&filter);

        load_persisted_alphabetical_groups(&*self.persisted, &interfaces_context, criteria, search)
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_genres(
        &self,
        _context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        self.persisted
            .load_persisted_genres(library_ids, collection_id)
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_tags(
        &self,
        _context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        self.persisted
            .load_persisted_tags(library_ids, collection_id)
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_languages(
        &self,
        _context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        self.persisted
            .load_persisted_languages(library_ids, collection_id)
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_publishers(
        &self,
        _context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        self.persisted
            .load_persisted_publishers(library_ids, collection_id)
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_age_ratings(
        &self,
        _context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        self.persisted
            .load_persisted_age_ratings(library_ids, collection_id)
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_sharing_labels(
        &self,
        _context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        self.persisted
            .load_persisted_sharing_labels(library_ids, collection_id)
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_series_tags(
        &self,
        _context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        self.persisted
            .load_persisted_series_tags(library_ids, collection_id)
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_series_release_dates(
        &self,
        _context: &DiscoveryQueryContext,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        self.persisted
            .load_persisted_series_release_dates(library_ids, collection_id)
            .await
            .map_err(DiscoveryError::Persistence)
    }

    async fn list_book_tags(
        &self,
        _context: &DiscoveryQueryContext,
        scope: Option<BookTagScope>,
        library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, DiscoveryError> {
        let persisted_scope = scope.map(|s| match s {
            BookTagScope::All => PersistedBookTagsScope::All,
            BookTagScope::Series(id) => PersistedBookTagsScope::Series(id),
            BookTagScope::Libraries(ids) => PersistedBookTagsScope::Libraries(ids),
            BookTagScope::ReadList(id) => PersistedBookTagsScope::ReadList(id),
        });
        self.persisted
            .load_persisted_book_tags(persisted_scope, library_ids)
            .await
            .map_err(DiscoveryError::Persistence)
    }
}
