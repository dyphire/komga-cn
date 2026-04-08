use komga_domain::discovery::{BookSort, DiscoveryError, UnsupportedDiscoverySemantics};

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
                "createdDate" | "created" => Ok(BookSort::CreatedDate),
                "lastModifiedDate" | "lastModified" => Ok(BookSort::LastModifiedDate),
                "metadata.releaseDate" => Ok(BookSort::MetadataReleaseDate),
                "seriesId" => Ok(BookSort::SeriesId),
                "number" | "metadata.numberSort" | "series" => Ok(BookSort::Number),
                "" => Ok(BookSort::MetadataTitle),
                _ => Err(DiscoveryError::UnsupportedSemantics(
                    UnsupportedDiscoverySemantics::UnsupportedBookSort(value.clone()),
                )),
            }
        })
        .collect()
}
