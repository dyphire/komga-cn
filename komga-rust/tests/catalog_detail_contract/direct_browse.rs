use super::*;

#[test]
fn in_scope_direct_browse_shapes_are_frozen() {
    let expected = BTreeSet::from([
        "GET /api/v1/series/{seriesId}",
        "GET /api/v1/series/{seriesId}/collections",
        "POST /api/v1/books/list?page={page}&size={size}&sort=metadata.numberSort,asc body=AllOfBook([SeriesId])",
        "POST /api/v1/books/list?unpaged=true&sort=metadata.numberSort,asc body=SeriesId",
        "GET /api/v1/books/{bookId}",
        "GET /api/v1/books/{bookId}/previous",
        "GET /api/v1/books/{bookId}/next",
        "GET /api/v1/books/{bookId}/readlists",
    ]);

    assert_eq!(expected, frozen_in_scope_direct_browse_shapes());

    let config = HarnessConfig::load_default().expect("default compat cases should load");
    for id in [
        "P3-DETAIL-SERIES-DETAIL-OWNED",
        "P3-DETAIL-SERIES-COLLECTIONS-OWNED",
        "P3-DETAIL-BOOKS-LIST-PAGED-SERIES-OWNED",
        "P3-DETAIL-BOOKS-LIST-UNPAGED-SIBLINGS-OWNED",
        "P3-DETAIL-BOOK-DETAIL-OWNED",
        "P3-DETAIL-BOOK-PREVIOUS-OWNED",
        "P3-DETAIL-BOOK-NEXT-OWNED",
        "P3-DETAIL-BOOK-READLISTS-OWNED",
    ] {
        assert!(
            config.cases.iter().any(|it| it.id == id),
            "missing detail owned compat case: {id}",
        );
    }
}

#[test]
fn browse_series_books_list_shape_is_frozen() {
    let expected = BTreeSet::from([
        "POST /api/v1/books/list?page={page}&size={size}&sort=metadata.numberSort,asc body=AllOfBook([SeriesId])",
        "POST /api/v1/books/list?unpaged=true&sort=metadata.numberSort,asc body=SeriesId",
    ]);

    let actual = frozen_in_scope_direct_browse_shapes()
        .into_iter()
        .filter(|shape| shape.starts_with("POST /api/v1/books/list"))
        .collect::<BTreeSet<_>>();

    assert_eq!(expected, actual);

    let config = HarnessConfig::load_default().expect("default compat cases should load");
    for id in [
        "P3-DETAIL-BOOKS-LIST-PAGED-SERIES-OWNED",
        "P3-DETAIL-BOOKS-LIST-UNPAGED-SIBLINGS-OWNED",
    ] {
        assert!(
            config.cases.iter().any(|it| it.id == id),
            "missing detail books/list owned compat case: {id}",
        );
    }
}
