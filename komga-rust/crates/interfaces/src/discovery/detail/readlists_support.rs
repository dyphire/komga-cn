use crate::contracts::common::PageDto;
use crate::contracts::discovery::ReadListDto;
use komga_application::discovery::{ReadListReadModel, ReadlistMutationInput};
use komga_domain::discovery::PageEnvelope;
use serde_json::Value;

pub(super) fn merge_readlist_write_input(
    existing: &ReadListReadModel,
    payload: &Value,
) -> ReadlistMutationInput {
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(existing.name.as_str())
        .to_string();
    let summary = payload
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or(existing.summary.as_str())
        .to_string();
    let ordered = payload
        .get("ordered")
        .and_then(Value::as_bool)
        .unwrap_or(existing.ordered);
    let book_ids = payload
        .get("bookIds")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| existing.book_ids.clone());

    ReadlistMutationInput {
        name,
        summary,
        ordered,
        book_ids,
    }
}

pub(super) fn readlists_page_payload(
    page: PageEnvelope<ReadListReadModel>,
    paged: bool,
) -> anyhow::Result<PageDto<ReadListDto>> {
    let content = page
        .content
        .iter()
        .map(ReadListDto::from_read_model)
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(PageDto::from_parts(
        content,
        page.page,
        page.size,
        page.total_elements,
        page.total_pages,
        paged,
        true,
    ))
}

#[cfg(test)]
mod tests {
    use crate::contracts::discovery::ReadListRequestMatchDto;
    use komga_application::discovery::{
        ComicRackMatchBook, ComicRackMatchSeries, ComicRackReadListMatchError,
        ComicRackReadListMatchGroup, ComicRackReadListMatchResult, ComicRackReadListRequestBook,
        ComicRackReadListRequestMatch,
    };

    #[test]
    fn comicrack_match_payload_serializes_matches_and_error_codes() {
        let payload = serde_json::to_value(
            ReadListRequestMatchDto::from_result(&ComicRackReadListMatchResult {
                name: "ReadList 1".to_string(),
                error: Some(ComicRackReadListMatchError::DuplicateName),
                requests: vec![ComicRackReadListRequestMatch {
                    request: ComicRackReadListRequestBook {
                        series_candidates: vec!["Series 1".to_string()],
                        number: "1".to_string(),
                    },
                    matches: vec![ComicRackReadListMatchGroup {
                        series: ComicRackMatchSeries {
                            series_id: "series-1".to_string(),
                            title: "Series 1".to_string(),
                            release_date: Some("2024-01-15".to_string()),
                        },
                        books: vec![ComicRackMatchBook {
                            book_id: "book-1".to_string(),
                            number: "1".to_string(),
                            title: "Book 1".to_string(),
                        }],
                    }],
                }],
            })
            .expect("comicrack match should map"),
        )
        .expect("comicrack match should serialize");

        assert_eq!(
            payload.get("errorCode").and_then(|it| it.as_str()),
            Some("")
        );
        assert_eq!(
            payload
                .get("readListMatch")
                .and_then(|it| it.get("errorCode"))
                .and_then(|it| it.as_str()),
            Some("ERR_1009"),
        );
        assert_eq!(
            payload
                .get("requests")
                .and_then(|it| it.as_array())
                .map(Vec::len),
            Some(1),
        );
    }
}
