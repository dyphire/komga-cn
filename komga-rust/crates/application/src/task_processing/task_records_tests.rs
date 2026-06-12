use super::{
    book_analyze_task_record, book_metadata_refresh_task_records, series_analyze_task_records,
    series_metadata_refresh_task_records,
};

#[test]
fn book_analyze_record_uses_book_series_group() {
    let record = book_analyze_task_record("book-1", "series-1");

    assert_eq!(record.id, "AnalyzeBook_book-1");
    assert_eq!(record.simple_type, "AnalyzeBook");
    assert_eq!(record.priority, 6);
    assert_eq!(record.group.as_deref(), Some("series-1"));
}

#[test]
fn book_metadata_refresh_records_include_book_metadata_and_artwork() {
    let records = book_metadata_refresh_task_records("book-1", "series-1");

    assert_eq!(
        records
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "RefreshBookMetadata_book-1",
            "RefreshBookLocalArtwork_book-1",
        ],
    );
    assert_eq!(records[0].group.as_deref(), Some("series-1"));
    assert_eq!(records[1].group, None);
}

#[test]
fn series_metadata_refresh_records_include_books_and_series_artwork() {
    let records = series_metadata_refresh_task_records(
        vec!["book-1".to_string(), "book-2".to_string()],
        "series-1",
    );

    let ids = records
        .iter()
        .map(|record| record.id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        ids,
        vec![
            "RefreshBookMetadata_book-1",
            "RefreshBookLocalArtwork_book-1",
            "RefreshBookMetadata_book-2",
            "RefreshBookLocalArtwork_book-2",
            "RefreshSeriesLocalArtwork_series-1",
        ],
    );
}

#[test]
fn series_analyze_records_use_series_group() {
    let records =
        series_analyze_task_records(vec!["book-1".to_string(), "book-2".to_string()], "series-1");

    assert_eq!(records.len(), 2);
    assert!(
        records
            .iter()
            .all(|record| record.simple_type == "AnalyzeBook")
    );
    assert!(records.iter().all(|record| record.priority == 6));
    assert!(
        records
            .iter()
            .all(|record| record.group.as_deref() == Some("series-1"))
    );
}
