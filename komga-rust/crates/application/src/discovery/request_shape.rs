use komga_domain::discovery::{
    BookSort, DiscoveryError, SeriesSort, UnsupportedDiscoverySemantics,
};

pub(crate) fn classify_series_sorts(raw: &[String]) -> Result<Vec<SeriesSort>, DiscoveryError> {
    raw.iter()
        .map(|value| {
            let field = value
                .split_once(',')
                .map(|(head, _)| head)
                .unwrap_or(value.as_str())
                .trim();

            match field {
                "metadata.titleSort" | "titleSort" => Ok(SeriesSort::MetadataTitleSort),
                "createdDate" => Ok(SeriesSort::CreatedDate),
                "lastModifiedDate" => Ok(SeriesSort::LastModifiedDate),
                "booksMetadata.releaseDate" => Ok(SeriesSort::BooksMetadataReleaseDate),
                "" => Ok(SeriesSort::MetadataTitleSort),
                _ => Err(DiscoveryError::UnsupportedSemantics(
                    UnsupportedDiscoverySemantics::UnsupportedSeriesSort(value.clone()),
                )),
            }
        })
        .collect()
}

pub(crate) fn classify_book_sorts(raw: &[String]) -> Result<Vec<BookSort>, DiscoveryError> {
    raw.iter()
        .map(|value| {
            let field = value
                .split_once(',')
                .map(|(head, _)| head)
                .unwrap_or(value.as_str())
                .trim();

            match field {
                "metadata.title" | "title" => Ok(BookSort::MetadataTitle),
                "createdDate" => Ok(BookSort::CreatedDate),
                "lastModifiedDate" => Ok(BookSort::LastModifiedDate),
                "metadata.releaseDate" => Ok(BookSort::MetadataReleaseDate),
                "" => Ok(BookSort::MetadataTitle),
                _ => Err(DiscoveryError::UnsupportedSemantics(
                    UnsupportedDiscoverySemantics::UnsupportedBookSort(value.clone()),
                )),
            }
        })
        .collect()
}
