use komga_compat_testkit::cases::HarnessConfig;
use std::collections::BTreeSet;

#[derive(Debug, PartialEq, Eq)]
struct NativeOwnedRequestContract {
    method: &'static str,
    path: &'static str,
    allowed_query_keys: &'static [&'static str],
    allowed_body_fields: &'static [&'static str],
    allowed_sorts: &'static [&'static str],
    allowed_condition_types: &'static [&'static str],
    fixed_sort: Option<&'static str>,
}

#[test]
fn native_owned_request_matrix_is_frozen() {
    let expected = vec![
        NativeOwnedRequestContract {
            method: "GET",
            path: "/api/v1/libraries",
            allowed_query_keys: &[],
            allowed_body_fields: &[],
            allowed_sorts: &[],
            allowed_condition_types: &[],
            fixed_sort: None,
        },
        NativeOwnedRequestContract {
            method: "POST",
            path: "/api/v1/series/list",
            allowed_query_keys: &["page", "size", "unpaged", "sort"],
            allowed_body_fields: &["fullTextSearch", "page", "size", "unpaged", "condition"],
            allowed_sorts: &[
                "metadata.titleSort",
                "createdDate",
                "lastModifiedDate",
                "booksMetadata.releaseDate",
            ],
            allowed_condition_types: &[
                "LibraryId",
                "AnyOfSeries",
                "AllOfSeries",
                "OneShot",
                "Deleted",
                "ReadStatus",
                "Genre",
                "Tag",
                "Language",
                "Publisher",
                "AgeRating",
                "ReleaseDate",
                "SharingLabel",
                "SeriesStatus",
                "Complete",
                "Author",
            ],
            fixed_sort: None,
        },
        NativeOwnedRequestContract {
            method: "POST",
            path: "/api/v1/books/list",
            allowed_query_keys: &["page", "size", "unpaged", "sort"],
            allowed_body_fields: &["fullTextSearch", "page", "size", "unpaged", "condition"],
            allowed_sorts: &[
                "metadata.title",
                "createdDate",
                "lastModifiedDate",
                "metadata.releaseDate",
            ],
            allowed_condition_types: &[
                "SeriesId",
                "LibraryId",
                "AnyOfBook",
                "AllOfBook",
                "OneShot",
                "Deleted",
                "ReadStatus",
                "Tag",
                "MediaProfile",
                "MediaStatus",
                "Author",
                "ReleaseDate",
            ],
            fixed_sort: None,
        },
        NativeOwnedRequestContract {
            method: "GET",
            path: "/api/v1/books/latest",
            allowed_query_keys: &["page", "size", "unpaged"],
            allowed_body_fields: &[],
            allowed_sorts: &[],
            allowed_condition_types: &[],
            fixed_sort: Some("lastModifiedDate,desc"),
        },
    ];

    assert_eq!(expected, frozen_native_owned_request_contract());

    let config = HarnessConfig::load_default().expect("default compat cases should load");
    for id in [
        "P2-DISCOVERY-LIBRARIES-OWNED",
        "P2-DISCOVERY-SERIES-LIST-OWNED",
        "P2-DISCOVERY-BOOKS-LIST-OWNED",
        "P2-DISCOVERY-BOOKS-LATEST-OWNED",
    ] {
        assert!(
            config.cases.iter().any(|it| it.id == id),
            "missing discovery owned compat case: {id}",
        );
    }
}

#[test]
fn unsupported_request_shapes_are_explicitly_non_native() {
    let expected = BTreeSet::from([
        "GET /api/v1/books/{id}/pages",
        "GET /api/v1/books/{id}/file",
        "GET /api/v1/books/{id}/thumbnail",
        "POST /api/v1/series/list sort=random",
        "POST /api/v1/books/list sort=readProgress.readDate",
        "GET /api/v1/readlists/*",
        "GET /api/v1/books/ondeck",
        "GET /api/v1/books/duplicates",
        "GET /api/v1/collections/*/series",
    ]);

    assert_eq!(expected, frozen_non_native_request_shapes());

    let config = HarnessConfig::load_default().expect("default compat cases should load");
    for id in [
        "P2-DISCOVERY-UNSUPPORTED-BOOK-PAGES",
        "P2-DISCOVERY-UNSUPPORTED-BOOK-FILE",
        "P2-DISCOVERY-UNSUPPORTED-BOOK-THUMBNAIL",
        "P2-DISCOVERY-UNSUPPORTED-SERIES-RANDOM-SORT",
        "P2-DISCOVERY-UNSUPPORTED-BOOK-READDATE-SORT",
        "P2-DISCOVERY-UNSUPPORTED-READLISTS",
        "P2-DISCOVERY-UNSUPPORTED-ONDECK",
        "P2-DISCOVERY-UNSUPPORTED-DUPLICATES",
        "P2-DISCOVERY-UNSUPPORTED-COLLECTIONS-GROUPED",
    ] {
        let case = config
            .cases
            .iter()
            .find(|it| it.id == id)
            .unwrap_or_else(|| panic!("missing non-native discovery compat case: {id}"));
        assert_eq!(
            case.headers
                .as_ref()
                .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership")),
            Some(&"shadow-java-writer".to_string()),
            "unsupported discovery case must carry explicit non-native marker: {id}",
        );
    }
}

fn frozen_native_owned_request_contract() -> Vec<NativeOwnedRequestContract> {
    vec![
        NativeOwnedRequestContract {
            method: "GET",
            path: "/api/v1/libraries",
            allowed_query_keys: &[],
            allowed_body_fields: &[],
            allowed_sorts: &[],
            allowed_condition_types: &[],
            fixed_sort: None,
        },
        NativeOwnedRequestContract {
            method: "POST",
            path: "/api/v1/series/list",
            allowed_query_keys: &["page", "size", "unpaged", "sort"],
            allowed_body_fields: &["fullTextSearch", "page", "size", "unpaged", "condition"],
            allowed_sorts: &[
                "metadata.titleSort",
                "createdDate",
                "lastModifiedDate",
                "booksMetadata.releaseDate",
            ],
            allowed_condition_types: &[
                "LibraryId",
                "AnyOfSeries",
                "AllOfSeries",
                "OneShot",
                "Deleted",
                "ReadStatus",
                "Genre",
                "Tag",
                "Language",
                "Publisher",
                "AgeRating",
                "ReleaseDate",
                "SharingLabel",
                "SeriesStatus",
                "Complete",
                "Author",
            ],
            fixed_sort: None,
        },
        NativeOwnedRequestContract {
            method: "POST",
            path: "/api/v1/books/list",
            allowed_query_keys: &["page", "size", "unpaged", "sort"],
            allowed_body_fields: &["fullTextSearch", "page", "size", "unpaged", "condition"],
            allowed_sorts: &[
                "metadata.title",
                "createdDate",
                "lastModifiedDate",
                "metadata.releaseDate",
            ],
            allowed_condition_types: &[
                "SeriesId",
                "LibraryId",
                "AnyOfBook",
                "AllOfBook",
                "OneShot",
                "Deleted",
                "ReadStatus",
                "Tag",
                "MediaProfile",
                "MediaStatus",
                "Author",
                "ReleaseDate",
            ],
            fixed_sort: None,
        },
        NativeOwnedRequestContract {
            method: "GET",
            path: "/api/v1/books/latest",
            allowed_query_keys: &["page", "size", "unpaged"],
            allowed_body_fields: &[],
            allowed_sorts: &[],
            allowed_condition_types: &[],
            fixed_sort: Some("lastModifiedDate,desc"),
        },
    ]
}

fn frozen_non_native_request_shapes() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "GET /api/v1/books/{id}/pages",
        "GET /api/v1/books/{id}/file",
        "GET /api/v1/books/{id}/thumbnail",
        "POST /api/v1/series/list sort=random",
        "POST /api/v1/books/list sort=readProgress.readDate",
        "GET /api/v1/readlists/*",
        "GET /api/v1/books/ondeck",
        "GET /api/v1/books/duplicates",
        "GET /api/v1/collections/*/series",
    ])
}
