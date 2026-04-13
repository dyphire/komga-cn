use komga_domain::discovery::{DiscoveryError, UnsupportedDiscoverySemantics};

pub(crate) fn classify_book_sorts(raw: &[String]) -> Result<(), DiscoveryError> {
    for value in raw {
        let field = value
            .split_once(',')
            .map(|(head, _)| head)
            .unwrap_or(value.as_str())
            .trim();

        match field {
            "metadata.title"
            | "title"
            | "createdDate"
            | "created"
            | "lastModifiedDate"
            | "lastModified"
            | "readProgress.lastModified"
            | "readProgress.readDate"
            | "metadata.releaseDate"
            | "seriesId"
            | "number"
            | "metadata.numberSort"
            | "series"
            | "" => {}
            _ => {
                return Err(DiscoveryError::UnsupportedSemantics(
                    UnsupportedDiscoverySemantics::UnsupportedBookSort(value.clone()),
                ));
            }
        }
    }

    Ok(())
}
