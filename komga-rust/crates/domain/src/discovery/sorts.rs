use super::errors::{DiscoveryError, NonNativeRequestShape};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeriesSort {
    MetadataTitleSort,
    CreatedDate,
    LastModifiedDate,
    BooksMetadataReleaseDate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookSort {
    MetadataTitle,
    CreatedDate,
    LastModifiedDate,
    MetadataReleaseDate,
}

pub fn classify_series_sorts(raw: &[String]) -> Result<Vec<SeriesSort>, DiscoveryError> {
    raw.iter()
        .map(|candidate| {
            let property = sort_property(candidate);
            match property {
                "metadata.titleSort" => Ok(SeriesSort::MetadataTitleSort),
                "createdDate" => Ok(SeriesSort::CreatedDate),
                "lastModifiedDate" => Ok(SeriesSort::LastModifiedDate),
                "booksMetadata.releaseDate" => Ok(SeriesSort::BooksMetadataReleaseDate),
                unsupported => Err(DiscoveryError::NonNativeRequestShape(
                    NonNativeRequestShape::UnsupportedSeriesSort(unsupported.to_string()),
                )),
            }
        })
        .collect()
}

pub fn classify_book_sorts(raw: &[String]) -> Result<Vec<BookSort>, DiscoveryError> {
    raw.iter()
        .map(|candidate| {
            let property = sort_property(candidate);
            match property {
                "metadata.title" => Ok(BookSort::MetadataTitle),
                "createdDate" => Ok(BookSort::CreatedDate),
                "lastModifiedDate" => Ok(BookSort::LastModifiedDate),
                "metadata.releaseDate" => Ok(BookSort::MetadataReleaseDate),
                unsupported => Err(DiscoveryError::NonNativeRequestShape(
                    NonNativeRequestShape::UnsupportedBookSort(unsupported.to_string()),
                )),
            }
        })
        .collect()
}

pub fn classify_direct_browse_books_list_sort(raw: &[String]) -> Result<(), DiscoveryError> {
    if raw.len() != 1 {
        return Err(DiscoveryError::NonNativeRequestShape(
            NonNativeRequestShape::UnsupportedBookSort(raw.first().cloned().unwrap_or_default()),
        ));
    }

    let sort = raw.first().cloned().unwrap_or_default();
    let (property, order) = split_sort_candidate(&sort);
    if property != "metadata.numberSort" {
        return Err(DiscoveryError::NonNativeRequestShape(
            NonNativeRequestShape::UnsupportedBookSort(sort),
        ));
    }

    if let Some(order) = order
        && !order.eq_ignore_ascii_case("asc")
    {
        return Err(DiscoveryError::NonNativeRequestShape(
            NonNativeRequestShape::UnsupportedBookSort(raw[0].clone()),
        ));
    }

    Ok(())
}

fn sort_property(candidate: &str) -> &str {
    candidate.split(',').next().unwrap_or(candidate).trim()
}

fn split_sort_candidate(candidate: &str) -> (&str, Option<&str>) {
    let mut parts = candidate.splitn(2, ',').map(str::trim);
    let property = parts.next().unwrap_or_default();
    let order = parts.next();
    (property, order)
}
