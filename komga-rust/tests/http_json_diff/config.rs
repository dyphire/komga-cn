use super::*;

#[test]
fn p0_cases_configuration_loads() {
    let config = HarnessConfig::load_default().expect("default compat cases should load");
    let case_ids: Vec<&str> = config.cases.iter().map(|it| it.id.as_str()).collect();
    let required_case_ids = [
        "KOMGA-P0-LIB-01-ADMIN",
        "KOMGA-P0-LIB-01-USER",
        "KOMGA-P0-LIB-01-LIMITED",
        "P2-DISCOVERY-LIBRARIES-OWNED",
        "P2-DISCOVERY-SERIES-LIST-OWNED",
        "P2-DISCOVERY-BOOKS-LIST-OWNED",
        "P2-DISCOVERY-BOOKS-LATEST-OWNED",
        "P3-DETAIL-SERIES-DETAIL-OWNED",
        "P3-DETAIL-BOOK-DETAIL-OWNED",
        "P2-DISCOVERY-UNSUPPORTED-BOOK-PAGES",
        "P2-DISCOVERY-UNSUPPORTED-BOOK-FILE",
        "P2-DISCOVERY-UNSUPPORTED-BOOK-THUMBNAIL",
        "P2-DISCOVERY-UNSUPPORTED-SERIES-RANDOM-SORT",
        "P2-DISCOVERY-UNSUPPORTED-BOOK-READDATE-SORT",
        "P2-DISCOVERY-UNSUPPORTED-ONDECK",
        "P2-DISCOVERY-UNSUPPORTED-DUPLICATES",
        "P2-DISCOVERY-UNSUPPORTED-COLLECTIONS-GROUPED",
        "P9-READLISTS-LIST-BROWSE-DEFAULT-OWNED",
        "P9-READLISTS-LIST-BROWSE-PAGE-SIZE-OWNED",
        "P9-READLISTS-LIST-BROWSE-REPEATED-LIBRARY-ID-OWNED",
        "P9-READLISTS-LIST-BROWSE-REPEATED-LIBRARY-ID-PAGE-SIZE-OWNED",
        "P9-READLISTS-LIST-BROWSE-SIZE-ZERO-OWNED",
        "P9-READLISTS-LIST-BROWSE-NEGATIVE-SEARCH",
        "P9-READLISTS-LIST-BROWSE-NEGATIVE-UNPAGED-TRUE",
        "P9-READLISTS-LIST-BROWSE-NEGATIVE-SORT",
        "P9-READLISTS-LIST-BROWSE-NEGATIVE-TACHIYOMI",
        "P10-READLISTS-SEARCH-DEFAULT-OWNED",
        "P10-READLISTS-SEARCH-PAGE-SIZE-OWNED",
        "P10-READLISTS-SEARCH-REPEATED-LIBRARY-ID-OWNED",
        "P10-READLISTS-SEARCH-REPEATED-LIBRARY-ID-PAGE-SIZE-OWNED",
        "P10-READLISTS-SEARCH-SIZE-ZERO-OWNED",
        "P10-READLISTS-SEARCH-REPEATED-LIBRARY-ID-SIZE-ZERO-OWNED",
        "P10-READLISTS-SEARCH-NO-RESULTS-OWNED",
        "P10-READLISTS-SEARCH-NEGATIVE-BLANK",
        "P10-READLISTS-SEARCH-NEGATIVE-WHITESPACE",
        "P10-READLISTS-SEARCH-NEGATIVE-SORT",
        "P10-READLISTS-SEARCH-NEGATIVE-UNPAGED-TRUE",
        "P10-READLISTS-SEARCH-NEGATIVE-DUPLICATE-PAGE",
        "P10-READLISTS-SEARCH-NEGATIVE-DUPLICATE-SIZE",
        "P10-READLISTS-SEARCH-NEGATIVE-UNSUPPORTED-EXTRA",
        "P1-AUTH-APIKEY-UPPER",
        "P1-AUTH-APIKEY-LOWER",
        "P1-AUTH-APIKEY-INVALID",
        "P0-OPDS-V1-SERIES",
        "P1-BK-READ-PROGRESS-DELETE",
        "P1-BK-READ-PROGRESS-404",
        "P1-BK-PROGRESSION-VALID",
        "P1-BK-PROGRESSION-INVALID",
        "P1-SEARCH-QUERY",
        "P1-SEARCH-ORDERING",
        "P1-SEARCH-OWNERSHIP-SHADOW",
    ];

    for id in required_case_ids {
        assert!(
            case_ids.contains(&id),
            "missing required compatibility case id: {id}"
        );
        assert_eq!(
            config.cases.iter().filter(|it| it.id == id).count(),
            1,
            "case id should appear exactly once: {id}"
        );
        assert_eq!(
            PathBuf::from(&config.output_dir).join(format!("{id}.json")),
            PathBuf::from("target/compat-diff").join(format!("{id}.json")),
            "diff evidence path contract changed for {id}"
        );
    }

    let library_case = config
        .cases
        .iter()
        .find(|it| it.id == "KOMGA-P0-LIB-01")
        .expect("library case should exist");
    let library_admin_case = config
        .cases
        .iter()
        .find(|it| it.id == "KOMGA-P0-LIB-01-ADMIN")
        .expect("library admin case should exist");
    let library_user_case = config
        .cases
        .iter()
        .find(|it| it.id == "KOMGA-P0-LIB-01-USER")
        .expect("library user case should exist");
    let library_limited_case = config
        .cases
        .iter()
        .find(|it| it.id == "KOMGA-P0-LIB-01-LIMITED")
        .expect("library limited case should exist");
    let latest_case = config
        .cases
        .iter()
        .find(|it| it.id == "KOMGA-P0-BOOKS-LATEST-01")
        .expect("books latest case should exist");
    let set_cookie_case = config
        .cases
        .iter()
        .find(|it| it.id == "P0-AUTH-SETCOOKIE")
        .expect("set-cookie case should exist");
    let remember_me_case = config
        .cases
        .iter()
        .find(|it| it.id == "P0-AUTH-REMEMBERME")
        .expect("remember-me case should exist");
    let pages_case = config
        .cases
        .iter()
        .find(|it| it.id == "KOMGA-P0-BK-PAGES-01")
        .expect("book pages case should exist");
    let thumbnail_case = config
        .cases
        .iter()
        .find(|it| it.id == "KOMGA-P0-BK-THUMBNAIL-01")
        .expect("book thumbnail case should exist");
    let book_thumbnail_case = config
        .cases
        .iter()
        .find(|it| it.id == "KOMGA-P0-BK-THUMBNAIL-BOOK-01")
        .expect("book cover thumbnail case should exist");
    let read_progress_case = config
        .cases
        .iter()
        .find(|it| it.id == "KOMGA-P0-BK-READ-PROGRESS-01")
        .expect("book read-progress case should exist");
    let api_key_upper_case = config
        .cases
        .iter()
        .find(|it| it.id == "P1-AUTH-APIKEY-UPPER")
        .expect("api-key upper-case header case should exist");
    let api_key_lower_case = config
        .cases
        .iter()
        .find(|it| it.id == "P1-AUTH-APIKEY-LOWER")
        .expect("api-key lower-case header case should exist");
    let api_key_invalid_case = config
        .cases
        .iter()
        .find(|it| it.id == "P1-AUTH-APIKEY-INVALID")
        .expect("api-key invalid case should exist");
    let read_progress_delete_case = config
        .cases
        .iter()
        .find(|it| it.id == "P1-BK-READ-PROGRESS-DELETE")
        .expect("read-progress delete case should exist");
    let read_progress_missing_case = config
        .cases
        .iter()
        .find(|it| it.id == "P1-BK-READ-PROGRESS-404")
        .expect("read-progress 404 case should exist");
    let progression_valid_case = config
        .cases
        .iter()
        .find(|it| it.id == "P1-BK-PROGRESSION-VALID")
        .expect("book progression valid case should exist");
    let progression_invalid_case = config
        .cases
        .iter()
        .find(|it| it.id == "P1-BK-PROGRESSION-INVALID")
        .expect("book progression invalid case should exist");
    let search_query_case = config
        .cases
        .iter()
        .find(|it| it.id == "P1-SEARCH-QUERY")
        .expect("search query case should exist");
    let search_ordering_case = config
        .cases
        .iter()
        .find(|it| it.id == "P1-SEARCH-ORDERING")
        .expect("search ordering case should exist");
    let search_ownership_case = config
        .cases
        .iter()
        .find(|it| it.id == "P1-SEARCH-OWNERSHIP-SHADOW")
        .expect("search ownership case should exist");
    let discovery_supported_series_case = config
        .cases
        .iter()
        .find(|it| it.id == "P2-DISCOVERY-SERIES-LIST-OWNED")
        .expect("discovery supported series list case should exist");
    let discovery_supported_books_case = config
        .cases
        .iter()
        .find(|it| it.id == "P2-DISCOVERY-BOOKS-LIST-OWNED")
        .expect("discovery supported books list case should exist");
    let detail_owned_series_case = config
        .cases
        .iter()
        .find(|it| it.id == "P3-DETAIL-SERIES-DETAIL-OWNED")
        .expect("detail owned series case should exist");
    let discovery_unsupported_books_readdate_sort = config
        .cases
        .iter()
        .find(|it| it.id == "P2-DISCOVERY-UNSUPPORTED-BOOK-READDATE-SORT")
        .expect("unsupported books read-date sort case should exist");
    let catalog_case = config
        .cases
        .iter()
        .find(|it| it.id == "P0-OPDS-V2-CATALOG-UNAUTH")
        .expect("opds catalog case should exist");
    let auth_case = config
        .cases
        .iter()
        .find(|it| it.id == "P0-OPDS-V2-AUTH-DOCUMENT")
        .expect("opds auth document case should exist");
    let opds_v1_series_case = config
        .cases
        .iter()
        .find(|it| it.id == "P0-OPDS-V1-SERIES")
        .expect("opds v1 series case should exist");
    let setup = library_case
        .setup
        .as_ref()
        .expect("library case should define setup");
    let login = &setup[0];
    let set_cookie_setup = set_cookie_case
        .setup
        .as_ref()
        .expect("set-cookie case should define setup");
    let set_cookie_login = &set_cookie_setup[0];

    assert_eq!(config.output_dir, "target/compat-diff");
    assert!(config.header_allowlist.contains(&"set-cookie".to_string()));
    assert!(config
        .header_allowlist
        .contains(&"x-auth-token".to_string()));
    assert!(config
        .header_allowlist
        .contains(&"www-authenticate".to_string()));
    assert!(case_ids.contains(&"KOMGA-P0-LIB-01"));
    assert!(case_ids.contains(&"KOMGA-P0-SERIES-01"));
    assert!(case_ids.contains(&"KOMGA-P0-BOOKS-LIST-01"));
    assert!(case_ids.contains(&"KOMGA-P0-BOOKS-LATEST-01"));
    assert!(case_ids.contains(&"KOMGA-P0-BK-PAGES-01"));
    assert!(case_ids.contains(&"KOMGA-P0-BK-THUMBNAIL-01"));
    assert!(case_ids.contains(&"KOMGA-P0-BK-THUMBNAIL-BOOK-01"));
    assert!(case_ids.contains(&"P0-AUTH-SETCOOKIE"));
    assert!(case_ids.contains(&"P0-AUTH-REMEMBERME"));
    assert_eq!(setup.len(), 1);
    assert_eq!(login.name, "login");
    assert_eq!(login.method, "GET");
    assert_eq!(login.path, "/api/v2/users/me");
    assert_eq!(
        login
            .headers
            .as_ref()
            .and_then(|headers| headers.get("Authorization")),
        Some(&"Basic ${KOMGA_COMPAT_BASIC_AUTH}".to_string())
    );
    assert_eq!(
        login
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Auth-Token")),
        Some(&"".to_string())
    );
    assert_eq!(
        login
            .extract_headers
            .as_ref()
            .and_then(|headers| headers.get("SESSION_TOKEN")),
        Some(&"X-Auth-Token".to_string())
    );
    assert_eq!(
        library_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Auth-Token")),
        Some(&"${SESSION_TOKEN}".to_string())
    );
    assert_eq!(library_admin_case.method, "GET");
    assert_eq!(library_admin_case.path, "/api/v1/libraries");
    assert_eq!(library_admin_case.comparison, ComparisonMode::Json);
    assert_eq!(library_admin_case.setup.as_ref().map(Vec::len), Some(1));
    assert_eq!(
        library_admin_case
            .setup
            .as_ref()
            .and_then(|steps| steps[0].headers.as_ref())
            .and_then(|headers| headers.get("Authorization")),
        Some(&"Basic ${KOMGA_COMPAT_BASIC_AUTH_ADMIN}".to_string())
    );
    assert_eq!(library_user_case.method, "GET");
    assert_eq!(library_user_case.path, "/api/v1/libraries");
    assert_eq!(library_user_case.comparison, ComparisonMode::Json);
    assert_eq!(library_user_case.setup.as_ref().map(Vec::len), Some(1));
    assert_eq!(
        library_user_case
            .setup
            .as_ref()
            .and_then(|steps| steps[0].headers.as_ref())
            .and_then(|headers| headers.get("Authorization")),
        Some(&"Basic ${KOMGA_COMPAT_BASIC_AUTH_USER}".to_string())
    );
    assert_eq!(library_limited_case.method, "GET");
    assert_eq!(library_limited_case.path, "/api/v1/libraries");
    assert_eq!(library_limited_case.comparison, ComparisonMode::Json);
    assert_eq!(library_limited_case.setup.as_ref().map(Vec::len), Some(1));
    assert_eq!(
        library_limited_case
            .setup
            .as_ref()
            .and_then(|steps| steps[0].headers.as_ref())
            .and_then(|headers| headers.get("Authorization")),
        Some(&"Basic ${KOMGA_COMPAT_BASIC_AUTH_LIMITED}".to_string())
    );
    assert_eq!(latest_case.method, "GET");
    assert_eq!(latest_case.path, "/api/v1/books/latest?unpaged=true");
    assert_eq!(latest_case.comparison, ComparisonMode::Json);
    assert_eq!(
        latest_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Auth-Token")),
        Some(&"${SESSION_TOKEN}".to_string())
    );
    assert_eq!(
        set_cookie_login
            .headers
            .as_ref()
            .and_then(|headers| headers.get("Authorization")),
        Some(&"Basic ${KOMGA_COMPAT_BASIC_AUTH}".to_string())
    );
    assert_eq!(remember_me_case.method, "GET");
    assert_eq!(remember_me_case.path, "/api/v2/users/me?remember-me=true");
    assert_eq!(remember_me_case.comparison, ComparisonMode::Json);
    assert_eq!(
        remember_me_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("Authorization")),
        Some(&"Basic ${KOMGA_COMPAT_BASIC_AUTH}".to_string())
    );
    assert_eq!(pages_case.method, "GET");
    assert_eq!(pages_case.path, "/api/v1/books/book-1/pages");
    assert_eq!(pages_case.comparison, ComparisonMode::Json);
    assert_eq!(
        pages_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Auth-Token")),
        Some(&"${SESSION_TOKEN}".to_string())
    );
    assert_eq!(thumbnail_case.method, "GET");
    assert_eq!(
        thumbnail_case.path,
        "/api/v1/books/book-1/pages/1/thumbnail"
    );
    assert_eq!(thumbnail_case.comparison, ComparisonMode::BinaryMetadata);
    assert_eq!(
        thumbnail_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Auth-Token")),
        Some(&"${SESSION_TOKEN}".to_string())
    );
    assert_eq!(book_thumbnail_case.method, "GET");
    assert_eq!(book_thumbnail_case.path, "/api/v1/books/book-1/thumbnail");
    assert_eq!(
        book_thumbnail_case.comparison,
        ComparisonMode::BinaryMetadata
    );
    assert_eq!(
        book_thumbnail_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Auth-Token")),
        Some(&"${SESSION_TOKEN}".to_string())
    );
    assert_eq!(read_progress_case.method, "PATCH");
    assert_eq!(
        read_progress_case.path,
        "/api/v1/books/book-1/read-progress"
    );
    assert_eq!(read_progress_case.comparison, ComparisonMode::Json);
    assert_eq!(
        read_progress_case.body.as_deref(),
        Some(r#"{"completed":true}"#)
    );
    assert_eq!(
        read_progress_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("Content-Type")),
        Some(&"application/json".to_string())
    );
    assert_eq!(
        read_progress_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Auth-Token")),
        Some(&"${SESSION_TOKEN}".to_string())
    );
    assert_eq!(api_key_upper_case.method, "GET");
    assert_eq!(api_key_upper_case.path, "/api/v2/users/me");
    assert_eq!(api_key_upper_case.comparison, ComparisonMode::Json);
    assert_eq!(
        api_key_upper_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-API-Key")),
        Some(&"${KOMGA_COMPAT_API_KEY}".to_string())
    );
    assert!(api_key_upper_case.setup.is_none());
    assert_eq!(api_key_lower_case.method, "GET");
    assert_eq!(api_key_lower_case.path, "/api/v2/users/me");
    assert_eq!(api_key_lower_case.comparison, ComparisonMode::Json);
    assert_eq!(
        api_key_lower_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("x-api-key")),
        Some(&"${KOMGA_COMPAT_API_KEY}".to_string())
    );
    assert!(api_key_lower_case.setup.is_none());
    assert_eq!(api_key_invalid_case.method, "GET");
    assert_eq!(api_key_invalid_case.path, "/api/v2/users/me");
    assert_eq!(api_key_invalid_case.comparison, ComparisonMode::Json);
    assert_eq!(
        api_key_invalid_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("x-api-key")),
        Some(&"${KOMGA_COMPAT_API_KEY_INVALID}".to_string())
    );
    assert!(api_key_invalid_case.setup.is_none());
    assert_eq!(read_progress_delete_case.method, "DELETE");
    assert_eq!(
        read_progress_delete_case.path,
        "/api/v1/books/book-1/read-progress"
    );
    assert_eq!(read_progress_delete_case.comparison, ComparisonMode::Json);
    assert_eq!(
        read_progress_delete_case.setup.as_ref().map(Vec::len),
        Some(1)
    );
    assert_eq!(read_progress_missing_case.method, "DELETE");
    assert_eq!(
        read_progress_missing_case.path,
        "/api/v1/books/book-missing/read-progress"
    );
    assert_eq!(read_progress_missing_case.comparison, ComparisonMode::Json);
    assert_eq!(
        read_progress_missing_case.setup.as_ref().map(Vec::len),
        Some(1)
    );
    assert_eq!(progression_valid_case.method, "PATCH");
    assert_eq!(
        progression_valid_case.path,
        "/api/v1/books/book-1/progression"
    );
    assert_eq!(progression_valid_case.comparison, ComparisonMode::Json);
    assert_eq!(progression_valid_case.setup.as_ref().map(Vec::len), Some(1));
    assert_eq!(
        progression_valid_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("Content-Type")),
        Some(&"application/json".to_string())
    );
    assert_eq!(
        progression_valid_case.body.as_deref(),
        Some(
            r#"{"modified":"2024-01-01T00:00:00Z","device":"compat-client","locator":{"href":"OEBPS/chapter-1.xhtml","type":"application/xhtml+xml","locations":{"progression":0.3}}}"#
        )
    );
    assert_eq!(progression_invalid_case.method, "PATCH");
    assert_eq!(
        progression_invalid_case.path,
        "/api/v1/books/book-1/progression"
    );
    assert_eq!(progression_invalid_case.comparison, ComparisonMode::Json);
    assert_eq!(
        progression_invalid_case.setup.as_ref().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        progression_invalid_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("Content-Type")),
        Some(&"application/json".to_string())
    );
    assert_eq!(
        progression_invalid_case.body.as_deref(),
        Some(
            r#"{"modified":"2024-01-01T00:00:00Z","device":"compat-client","locator":{"href":"OEBPS/chapter-1.xhtml","type":"application/xhtml+xml","locations":{}}}"#
        )
    );
    assert_eq!(search_query_case.method, "POST");
    assert_eq!(search_query_case.path, "/api/v1/series/list");
    assert_eq!(search_query_case.comparison, ComparisonMode::Json);
    assert_eq!(search_query_case.setup.as_ref().map(Vec::len), Some(1));
    assert_eq!(
        search_query_case.body.as_deref(),
        Some(r#"{"fullTextSearch":"series"}"#)
    );
    assert_eq!(search_ordering_case.method, "POST");
    assert_eq!(
        search_ordering_case.path,
        "/api/v1/series/list?page=0&size=20&sort=metadata.titleSort,asc"
    );
    assert_eq!(search_ordering_case.comparison, ComparisonMode::Json);
    assert_eq!(search_ordering_case.setup.as_ref().map(Vec::len), Some(1));
    assert_eq!(
        search_ordering_case.body.as_deref(),
        Some(r#"{"fullTextSearch":"series"}"#)
    );
    assert_eq!(search_ownership_case.method, "POST");
    assert_eq!(search_ownership_case.path, "/api/v1/series/list");
    assert_eq!(search_ownership_case.comparison, ComparisonMode::Json);
    assert_eq!(search_ownership_case.setup.as_ref().map(Vec::len), Some(1));
    assert_eq!(
        search_ownership_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership")),
        Some(&"shadow-java-writer".to_string())
    );
    let search_ownership_allowlist = search_ownership_case.header_allowlist();
    assert!(
        search_ownership_allowlist.contains("x-komga-compat-search-ownership"),
        "search ownership marker header should be diff-allowlisted at case level"
    );
    assert_eq!(
        search_ownership_case.body.as_deref(),
        Some(r#"{"fullTextSearch":"series","ownership":"shadow"}"#)
    );
    assert_eq!(discovery_supported_series_case.method, "POST");
    assert_eq!(
        discovery_supported_series_case.path,
        "/api/v1/series/list?page=0&size=20&sort=metadata.titleSort,asc"
    );
    assert_eq!(
        discovery_supported_series_case.body.as_deref(),
        Some(
            r#"{"fullTextSearch":"series","condition":{"type":"LibraryId","operator":"is","value":"library-1"}}"#
        )
    );
    assert_eq!(discovery_supported_books_case.method, "POST");
    assert_eq!(
        discovery_supported_books_case.path,
        "/api/v1/books/list?page=0&size=20&sort=metadata.title,asc"
    );
    assert_eq!(
        discovery_supported_books_case.body.as_deref(),
        Some(
            r#"{"fullTextSearch":"book","condition":{"type":"LibraryId","operator":"is","value":"library-1"}}"#
        )
    );
    assert_eq!(detail_owned_series_case.method, "GET");
    assert_eq!(detail_owned_series_case.path, "/api/v1/series/series-1");
    assert_eq!(
        detail_owned_series_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership")),
        None
    );
    assert_eq!(discovery_unsupported_books_readdate_sort.method, "POST");
    assert_eq!(
        discovery_unsupported_books_readdate_sort.path,
        "/api/v1/books/list?page=0&size=20&sort=readProgress.readDate,desc"
    );
    assert_eq!(
        discovery_unsupported_books_readdate_sort
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership")),
        Some(&"shadow-java-writer".to_string())
    );
    assert_eq!(catalog_case.method, "GET");
    assert_eq!(catalog_case.path, "/opds/v2/catalog");
    assert_eq!(catalog_case.comparison, ComparisonMode::Json);
    assert!(catalog_case.headers.is_none());
    assert_eq!(auth_case.method, "GET");
    assert_eq!(auth_case.path, "/opds/v2/auth");
    assert_eq!(auth_case.comparison, ComparisonMode::Json);
    assert!(auth_case.headers.is_none());
    assert_eq!(opds_v1_series_case.method, "GET");
    assert_eq!(opds_v1_series_case.path, "/opds/v1.2/series");
    assert_eq!(opds_v1_series_case.comparison, ComparisonMode::Xml);
    assert_eq!(opds_v1_series_case.setup.as_ref().map(Vec::len), Some(1));
    assert_eq!(
        opds_v1_series_case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Auth-Token")),
        Some(&"${SESSION_TOKEN}".to_string())
    );
    assert_eq!(
        opds_v1_series_case
            .setup
            .as_ref()
            .and_then(|steps| steps[0].headers.as_ref())
            .and_then(|headers| headers.get("Authorization")),
        Some(&"Basic ${KOMGA_COMPAT_BASIC_AUTH_USER}".to_string())
    );
}

#[test]
fn phase3_detail_case_inventory_loads() {
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
        "P3-DETAIL-EXCLUDED-BOOK-PAGES",
        "P3-DETAIL-EXCLUDED-BOOK-FILE",
        "P3-DETAIL-EXCLUDED-BOOK-THUMBNAIL",
        "P3-DETAIL-EXCLUDED-BOOK-MANIFEST",
        "P3-DETAIL-EXCLUDED-BOOK-RESOURCE",
        "P3-DETAIL-EXCLUDED-BOOK-POSITIONS",
        "P3-DETAIL-EXCLUDED-SERIES-THUMBNAIL",
        "P3-DETAIL-EXCLUDED-SERIES-FILE",
        "P3-DETAIL-EXCLUDED-READLIST-CONTEXT-BOOKS",
        "P3-DETAIL-EXCLUDED-READLIST-CONTEXT-PREVIOUS",
        "P3-DETAIL-EXCLUDED-READLIST-CONTEXT-NEXT",
        "P3-DETAIL-EXCLUDED-ONESHOT-ROUTE-CLOSURE",
        "P3-DETAIL-EXCLUDED-READ-PROGRESS-PATCH",
        "P3-DETAIL-EXCLUDED-READ-PROGRESS-DELETE",
        "P3-DETAIL-EXCLUDED-PROGRESSION-PUT",
        "P3-DETAIL-EXCLUDED-READLIST-PATCH",
        "P3-DETAIL-EXCLUDED-READLIST-DELETE",
        "P3-DETAIL-EXCLUDED-COLLECTION-PATCH",
        "P3-DETAIL-EXCLUDED-COLLECTION-DELETE",
        "P3-DETAIL-EXCLUDED-BOOKS-LIST-READDATE-SORT",
        "P3-DETAIL-EXCLUDED-BOOKS-LIST-READSTATUS-FILTER",
    ] {
        let case = config
            .cases
            .iter()
            .find(|it| it.id == id)
            .unwrap_or_else(|| panic!("missing phase3 detail compat case: {id}"));

        assert_eq!(
            config.cases.iter().filter(|it| it.id == id).count(),
            1,
            "phase3 detail case id must be unique: {id}",
        );

        if id.contains("-EXCLUDED-") {
            assert_eq!(
                case.headers
                    .as_ref()
                    .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership")),
                Some(&"shadow-java-writer".to_string()),
                "excluded phase3 detail case must carry shadow marker: {id}",
            );
        }
    }
}

#[test]
pub(super) fn phase6_readlist_detail_case_inventory_loads() {
    let config = HarnessConfig::load_default().expect("default compat cases should load");

    for id in [
        "P6-ONESHOT-READLIST-DETAIL-OWNED",
        "P5-ONESHOT-BOOKS-LIST-SERIESID-ONLY-OWNED",
        "P9-READLISTS-LIST-BROWSE-NEGATIVE-SEARCH",
        "P3-DETAIL-EXCLUDED-READLIST-CONTEXT-BOOKS",
        "P3-DETAIL-EXCLUDED-READLIST-CONTEXT-PREVIOUS",
        "P3-DETAIL-EXCLUDED-READLIST-CONTEXT-NEXT",
        "P3-DETAIL-EXCLUDED-ONESHOT-ROUTE-CLOSURE",
        "P5-ONESHOT-EXCLUDED-BOOKS-LIST-WIDENED-PAGED",
        "P3-DETAIL-EXCLUDED-BOOKS-LIST-READDATE-SORT",
        "P3-DETAIL-EXCLUDED-BOOKS-LIST-READSTATUS-FILTER",
        "P3-DETAIL-EXCLUDED-BOOK-PAGES",
        "P3-DETAIL-EXCLUDED-BOOK-FILE",
        "P3-DETAIL-EXCLUDED-BOOK-THUMBNAIL",
        "P3-DETAIL-EXCLUDED-BOOK-MANIFEST",
        "P3-DETAIL-EXCLUDED-BOOK-RESOURCE",
        "P3-DETAIL-EXCLUDED-BOOK-POSITIONS",
        "P3-DETAIL-EXCLUDED-READ-PROGRESS-PATCH",
        "P3-DETAIL-EXCLUDED-READ-PROGRESS-DELETE",
        "P3-DETAIL-EXCLUDED-PROGRESSION-PUT",
        "P3-DETAIL-EXCLUDED-READLIST-PATCH",
        "P3-DETAIL-EXCLUDED-READLIST-DELETE",
        "P3-DETAIL-EXCLUDED-COLLECTION-PATCH",
        "P3-DETAIL-EXCLUDED-COLLECTION-DELETE",
        "P5-ONESHOT-EXCLUDED-SSE-LIVE-REFRESH",
    ] {
        let case = config
            .cases
            .iter()
            .find(|it| it.id == id)
            .unwrap_or_else(|| panic!("missing phase6 readlist-detail compat case: {id}"));

        assert_eq!(
            config.cases.iter().filter(|it| it.id == id).count(),
            1,
            "phase6 readlist-detail case id must be unique: {id}",
        );

        if id.contains("-EXCLUDED-") || id == "P9-READLISTS-LIST-BROWSE-NEGATIVE-SEARCH" {
            assert_eq!(
                case.headers
                    .as_ref()
                    .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership")),
                Some(&"shadow-java-writer".to_string()),
                "phase6 adjacent non-native case must carry shadow marker: {id}",
            );
        }
    }

    let owned = config
        .cases
        .iter()
        .find(|it| it.id == "P6-ONESHOT-READLIST-DETAIL-OWNED")
        .expect("phase6 readlist detail owned case should exist");
    assert_eq!(owned.method, "GET");
    assert_eq!(owned.path, "/api/v1/readlists/readlist-1");
    assert_eq!(owned.body, None);
    assert_eq!(
        owned
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership")),
        None,
    );
}

#[test]
pub(super) fn phase7_series_oneshot_case_inventory_loads() {
    let config = HarnessConfig::load_default().expect("default compat cases should load");

    for id in [
        "P7-ONESHOT-SERIES-DETAIL-EXACT-OWNED",
        "P3-DETAIL-SERIES-DETAIL-OWNED",
        "P3-DETAIL-EXCLUDED-ONESHOT-ROUTE-CLOSURE",
        "P9-READLISTS-LIST-BROWSE-NEGATIVE-SEARCH",
        "P3-DETAIL-EXCLUDED-READLIST-CONTEXT-BOOKS",
        "P3-DETAIL-EXCLUDED-READLIST-CONTEXT-PREVIOUS",
        "P3-DETAIL-EXCLUDED-READLIST-CONTEXT-NEXT",
        "P5-ONESHOT-EXCLUDED-BOOKS-LIST-WIDENED-PAGED",
        "P3-DETAIL-EXCLUDED-BOOKS-LIST-READDATE-SORT",
        "P3-DETAIL-EXCLUDED-BOOKS-LIST-READSTATUS-FILTER",
        "P3-DETAIL-EXCLUDED-BOOK-PAGES",
        "P3-DETAIL-EXCLUDED-BOOK-FILE",
        "P3-DETAIL-EXCLUDED-BOOK-THUMBNAIL",
        "P3-DETAIL-EXCLUDED-BOOK-MANIFEST",
        "P3-DETAIL-EXCLUDED-BOOK-RESOURCE",
        "P3-DETAIL-EXCLUDED-BOOK-POSITIONS",
        "P3-DETAIL-EXCLUDED-READ-PROGRESS-PATCH",
        "P3-DETAIL-EXCLUDED-READ-PROGRESS-DELETE",
        "P3-DETAIL-EXCLUDED-PROGRESSION-PUT",
        "P3-DETAIL-EXCLUDED-READLIST-PATCH",
        "P3-DETAIL-EXCLUDED-READLIST-DELETE",
        "P3-DETAIL-EXCLUDED-COLLECTION-PATCH",
        "P3-DETAIL-EXCLUDED-COLLECTION-DELETE",
        "P5-ONESHOT-EXCLUDED-SSE-LIVE-REFRESH",
    ] {
        let case = config
            .cases
            .iter()
            .find(|it| it.id == id)
            .unwrap_or_else(|| panic!("missing phase7 series oneshot compat case: {id}"));

        assert_eq!(
            config.cases.iter().filter(|it| it.id == id).count(),
            1,
            "phase7 series oneshot case id must be unique: {id}",
        );

        if id.contains("-EXCLUDED-") || id == "P9-READLISTS-LIST-BROWSE-NEGATIVE-SEARCH" {
            assert_eq!(
                case.headers
                    .as_ref()
                    .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership")),
                Some(&"shadow-java-writer".to_string()),
                "phase7 adjacent non-native case must carry shadow marker: {id}",
            );
        }
    }

    let owned = config
        .cases
        .iter()
        .find(|it| it.id == "P7-ONESHOT-SERIES-DETAIL-EXACT-OWNED")
        .expect("phase7 exact oneshot route owned case should exist");
    assert_eq!(owned.method, "GET");
    assert_eq!(owned.path, "/api/v1/series/series-1?oneshot=true");
    assert_eq!(owned.body, None);
    assert_eq!(
        owned
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership")),
        None,
    );
}

#[test]
pub(super) fn phase8_readlist_books_family_case_inventory_loads() {
    let config = HarnessConfig::load_default().expect("default compat cases should load");
    let owned_case_ids: BTreeSet<&str> = phase8_readlist_books_family_owned_case_ids()
        .iter()
        .copied()
        .collect();
    let negative_case_ids: BTreeSet<&str> = phase8_readlist_books_family_negative_case_ids()
        .iter()
        .copied()
        .collect();
    let all_case_ids: BTreeSet<&str> = phase8_readlist_books_family_all_case_ids()
        .iter()
        .copied()
        .collect();

    assert!(
        owned_case_ids.is_disjoint(&negative_case_ids),
        "phase8 owned and negative compat inventory buckets must stay disjoint",
    );
    assert_eq!(
        owned_case_ids
            .union(&negative_case_ids)
            .copied()
            .collect::<BTreeSet<_>>(),
        all_case_ids,
        "phase8 owned + negative compat inventory buckets must explain the full case inventory",
    );

    for id in phase8_readlist_books_family_all_case_ids() {
        let case = config
            .cases
            .iter()
            .find(|it| it.id == *id)
            .unwrap_or_else(|| panic!("missing phase8 readlist-books compat case: {id}"));

        assert_eq!(
            config.cases.iter().filter(|it| it.id == *id).count(),
            1,
            "phase8 readlist-books case id must be unique: {id}",
        );
        assert_eq!(
            case.method, "GET",
            "phase8 readlist-books cases stay GET-only: {id}"
        );
        assert_eq!(
            case.body, None,
            "phase8 readlist-books cases must stay body-less: {id}"
        );
        assert!(
            case.setup.is_none(),
            "phase8 readlist-books compat cases must stay self-contained without setup blocks: {id}",
        );
        assert_eq!(
            PathBuf::from(&config.output_dir).join(format!("{id}.json")),
            PathBuf::from("target/compat-diff").join(format!("{id}.json")),
            "phase8 readlist-books diff evidence path contract changed for {id}",
        );
        assert_eq!(
            case.headers
                .as_ref()
                .and_then(|headers| headers.get("X-Auth-Token")),
            Some(&"${SESSION_TOKEN}".to_string()),
            "phase8 readlist-books compat cases must use explicit session-token wiring: {id}",
        );

        let ownership_header = case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership"));

        if owned_case_ids.contains(id) || *id == "P8-READLIST-BOOKS-DEPENDENCY-UNPAGED-PREOWNED" {
            assert_eq!(
                ownership_header, None,
                "phase8 owned/dependency case must not carry a shadow marker: {id}",
            );
        } else {
            assert_eq!(
                ownership_header,
                Some(&"shadow-java-writer".to_string()),
                "phase8 negative case must carry shadow marker: {id}",
            );
        }
    }

    let default_paged = config
        .cases
        .iter()
        .find(|it| it.id == "P8-READLIST-BOOKS-PAGED-DEFAULT-OWNED")
        .expect("phase8 default paged case should exist");
    let explicit_paged = config
        .cases
        .iter()
        .find(|it| it.id == "P8-READLIST-BOOKS-PAGED-EXPLICIT-OWNED")
        .expect("phase8 explicit paged case should exist");
    let library_filter = config
        .cases
        .iter()
        .find(|it| it.id == "P8-READLIST-BOOKS-LIBRARY-FILTER-OWNED")
        .expect("phase8 library filter case should exist");
    let read_status = config
        .cases
        .iter()
        .find(|it| it.id == "P8-READLIST-BOOKS-READ-STATUS-FILTER-OWNED")
        .expect("phase8 read status case should exist");
    let media_status = config
        .cases
        .iter()
        .find(|it| it.id == "P8-READLIST-BOOKS-MEDIA-STATUS-FILTER-OWNED")
        .expect("phase8 media status case should exist");
    let repeated_tag = config
        .cases
        .iter()
        .find(|it| it.id == "P8-READLIST-BOOKS-REPEATED-TAG-FILTER-OWNED")
        .expect("phase8 repeated tag case should exist");
    let repeated_author = config
        .cases
        .iter()
        .find(|it| it.id == "P8-READLIST-BOOKS-REPEATED-AUTHOR-FILTER-OWNED")
        .expect("phase8 repeated author case should exist");
    let deleted_true = config
        .cases
        .iter()
        .find(|it| it.id == "P8-READLIST-BOOKS-DELETED-TRUE-FILTER-OWNED")
        .expect("phase8 deleted=true case should exist");
    let deleted_false = config
        .cases
        .iter()
        .find(|it| it.id == "P8-READLIST-BOOKS-DELETED-FALSE-FILTER-OWNED")
        .expect("phase8 deleted=false case should exist");
    let combined_filters = config
        .cases
        .iter()
        .find(|it| it.id == "P8-READLIST-BOOKS-COMBINED-FILTERS-OWNED")
        .expect("phase8 combined filters case should exist");
    let combined_repeated_filters = config
        .cases
        .iter()
        .find(|it| it.id == "P8-READLIST-BOOKS-COMBINED-REPEATED-FILTERS-OWNED")
        .expect("phase8 combined repeated filters case should exist");
    let unpaged_false = config
        .cases
        .iter()
        .find(|it| it.id == "P8-READLIST-BOOKS-UNPAGED-FALSE-OWNED")
        .expect("phase8 unpaged=false case should exist");
    let dependency_unpaged = config
        .cases
        .iter()
        .find(|it| it.id == "P8-READLIST-BOOKS-DEPENDENCY-UNPAGED-PREOWNED")
        .expect("phase8 dependency unpaged case should exist");
    let widened_unpaged = config
        .cases
        .iter()
        .find(|it| it.id == "P8-READLIST-BOOKS-DEPENDENCY-UNPAGED-WIDENED-SHADOW")
        .expect("phase8 widened unpaged case should exist");
    let readlists = config
        .cases
        .iter()
        .find(|it| it.id == "P8-READLIST-BOOKS-EXCLUDED-READLISTS-LIST-FAMILY")
        .expect("phase8 readlists exclusion case should exist");
    let tachiyomi = config
        .cases
        .iter()
        .find(|it| it.id == "P8-READLIST-BOOKS-EXCLUDED-TACHIYOMI")
        .expect("phase8 tachiyomi exclusion case should exist");

    assert_eq!(default_paged.path, "/api/v1/readlists/readlist-2/books");
    assert_eq!(
        explicit_paged.path,
        "/api/v1/readlists/readlist-2/books?page=1&size=1"
    );
    assert_eq!(
        library_filter.path,
        "/api/v1/readlists/readlist-2/books?page=0&size=20&library_id=1"
    );
    assert_eq!(
        read_status.path,
        "/api/v1/readlists/readlist-2/books?page=0&size=20&read_status=READ"
    );
    assert_eq!(
        media_status.path,
        "/api/v1/readlists/readlist-2/books?page=0&size=20&media_status=UNSUPPORTED"
    );
    assert_eq!(
        repeated_tag.path,
        "/api/v1/readlists/readlist-2/books?page=0&size=20&tag=safe&tag=missing"
    );
    assert_eq!(repeated_author.path, "/api/v1/readlists/readlist-2/books?page=0&size=20&author=alice,writer&author=charlie,writer");
    assert_eq!(
        deleted_true.path,
        "/api/v1/readlists/readlist-2/books?page=0&size=20&deleted=true"
    );
    assert_eq!(
        deleted_false.path,
        "/api/v1/readlists/readlist-2/books?page=0&size=20&deleted=false"
    );
    assert_eq!(combined_filters.path, "/api/v1/readlists/readlist-2/books?page=0&size=20&read_status=READ&media_status=READY&tag=safe&author=alice,writer&deleted=false");
    assert_eq!(combined_repeated_filters.path, "/api/v1/readlists/readlist-2/books?page=0&size=20&read_status=READ&media_status=READY&tag=safe&tag=missing&author=alice,writer&author=charlie,writer&deleted=false");
    assert_eq!(
        unpaged_false.path,
        "/api/v1/readlists/readlist-2/books?unpaged=false"
    );
    assert_eq!(
        dependency_unpaged.path,
        "/api/v1/readlists/readlist-2/books?unpaged=true"
    );
    assert_eq!(
        widened_unpaged.path,
        "/api/v1/readlists/readlist-2/books?unpaged=true&library_id=1"
    );
    assert_eq!(
        readlists.path,
        "/api/v1/readlists?search=alpha&sort=name,asc"
    );
    assert_eq!(
        tachiyomi.path,
        "/api/v1/readlists/readlist-2/read-progress/tachiyomi"
    );
}

pub(super) fn readlists_list_browse_case_inventory_loads() {
    let config = HarnessConfig::load_default().expect("default compat cases should load");
    let owned_case_ids: BTreeSet<&str> = phase9_readlists_list_browse_owned_case_ids()
        .iter()
        .copied()
        .collect();
    let negative_case_ids: BTreeSet<&str> = phase9_readlists_list_browse_negative_case_ids()
        .iter()
        .copied()
        .collect();
    let all_case_ids: BTreeSet<&str> = phase9_readlists_list_browse_all_case_ids()
        .iter()
        .copied()
        .collect();

    assert!(
        owned_case_ids.is_disjoint(&negative_case_ids),
        "phase9 owned and negative compat inventory buckets must stay disjoint",
    );
    assert_eq!(
        owned_case_ids
            .union(&negative_case_ids)
            .copied()
            .collect::<BTreeSet<_>>(),
        all_case_ids,
        "phase9 owned + negative compat inventory buckets must explain the full case inventory",
    );

    for id in phase9_readlists_list_browse_all_case_ids() {
        let case = config
            .cases
            .iter()
            .find(|it| it.id == *id)
            .unwrap_or_else(|| panic!("missing phase9 readlists-list browse compat case: {id}"));

        assert_eq!(
            config.cases.iter().filter(|it| it.id == *id).count(),
            1,
            "phase9 readlists-list browse case id must be unique: {id}",
        );
        assert_eq!(
            case.method, "GET",
            "phase9 readlists-list browse cases stay GET-only: {id}",
        );
        assert_eq!(
            case.body, None,
            "phase9 readlists-list browse cases must stay body-less: {id}",
        );
        assert!(
            case.setup.is_none(),
            "phase9 readlists-list browse compat cases must stay self-contained without setup blocks: {id}",
        );
        assert_eq!(
            PathBuf::from(&config.output_dir).join(format!("{id}.json")),
            PathBuf::from("target/compat-diff").join(format!("{id}.json")),
            "phase9 readlists-list browse diff evidence path contract changed for {id}",
        );
        assert_eq!(
            case.headers
                .as_ref()
                .and_then(|headers| headers.get("X-Auth-Token")),
            Some(&"${SESSION_TOKEN}".to_string()),
            "phase9 readlists-list browse compat cases must use explicit session-token wiring: {id}",
        );

        let ownership_header = case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership"));

        if owned_case_ids.contains(id) {
            assert_eq!(
                ownership_header, None,
                "phase9 owned browse case must not carry a shadow marker: {id}",
            );
        } else {
            assert_eq!(
                ownership_header,
                Some(&"shadow-java-writer".to_string()),
                "phase9 negative browse case must carry a shadow marker: {id}",
            );
        }
    }

    let default_browse = config
        .cases
        .iter()
        .find(|it| it.id == "P9-READLISTS-LIST-BROWSE-DEFAULT-OWNED")
        .expect("phase9 default browse case should exist");
    let page_size = config
        .cases
        .iter()
        .find(|it| it.id == "P9-READLISTS-LIST-BROWSE-PAGE-SIZE-OWNED")
        .expect("phase9 page/size browse case should exist");
    let repeated_library = config
        .cases
        .iter()
        .find(|it| it.id == "P9-READLISTS-LIST-BROWSE-REPEATED-LIBRARY-ID-OWNED")
        .expect("phase9 repeated library browse case should exist");
    let repeated_library_page_size = config
        .cases
        .iter()
        .find(|it| it.id == "P9-READLISTS-LIST-BROWSE-REPEATED-LIBRARY-ID-PAGE-SIZE-OWNED")
        .expect("phase9 repeated library + page/size browse case should exist");
    let size_zero = config
        .cases
        .iter()
        .find(|it| it.id == "P9-READLISTS-LIST-BROWSE-SIZE-ZERO-OWNED")
        .expect("phase9 size=0 browse case should exist");
    let search = config
        .cases
        .iter()
        .find(|it| it.id == "P9-READLISTS-LIST-BROWSE-NEGATIVE-SEARCH")
        .expect("phase9 search browse exclusion case should exist");
    let unpaged_true = config
        .cases
        .iter()
        .find(|it| it.id == "P9-READLISTS-LIST-BROWSE-NEGATIVE-UNPAGED-TRUE")
        .expect("phase9 unpaged=true browse exclusion case should exist");
    let sort = config
        .cases
        .iter()
        .find(|it| it.id == "P9-READLISTS-LIST-BROWSE-NEGATIVE-SORT")
        .expect("phase9 sort browse exclusion case should exist");
    let tachiyomi = config
        .cases
        .iter()
        .find(|it| it.id == "P9-READLISTS-LIST-BROWSE-NEGATIVE-TACHIYOMI")
        .expect("phase9 tachiyomi browse exclusion case should exist");

    assert_eq!(default_browse.path, "/api/v1/readlists");
    assert_eq!(page_size.path, "/api/v1/readlists?page=1&size=1");
    assert_eq!(
        repeated_library.path,
        "/api/v1/readlists?library_id=1&library_id=2"
    );
    assert_eq!(
        repeated_library_page_size.path,
        "/api/v1/readlists?library_id=1&library_id=2&page=1&size=1"
    );
    assert_eq!(size_zero.path, "/api/v1/readlists?size=0");
    assert_eq!(search.path, "/api/v1/readlists?search=");
    assert_eq!(unpaged_true.path, "/api/v1/readlists?unpaged=true");
    assert_eq!(sort.path, "/api/v1/readlists?sort=name,desc");
    assert_eq!(
        tachiyomi.path,
        "/api/v1/readlists/readlist-2/read-progress/tachiyomi"
    );
}

#[test]
pub(super) fn phase10_readlists_search_case_inventory_loads() {
    let config = HarnessConfig::load_default().expect("default compat cases should load");
    let owned_case_ids: BTreeSet<&str> = phase10_readlists_search_owned_case_ids()
        .iter()
        .copied()
        .collect();
    let negative_case_ids: BTreeSet<&str> = phase10_readlists_search_negative_case_ids()
        .iter()
        .copied()
        .collect();
    let all_case_ids: BTreeSet<&str> = phase10_readlists_search_all_case_ids()
        .iter()
        .copied()
        .collect();

    assert!(
        owned_case_ids.is_disjoint(&negative_case_ids),
        "phase10 owned and negative compat inventory buckets must stay disjoint",
    );
    assert_eq!(
        owned_case_ids
            .union(&negative_case_ids)
            .copied()
            .collect::<BTreeSet<_>>(),
        all_case_ids,
        "phase10 owned + negative compat inventory buckets must explain the full case inventory",
    );

    for id in phase10_readlists_search_all_case_ids() {
        let case = config
            .cases
            .iter()
            .find(|it| it.id == *id)
            .unwrap_or_else(|| panic!("missing phase10 readlists search compat case: {id}"));

        assert_eq!(
            config.cases.iter().filter(|it| it.id == *id).count(),
            1,
            "phase10 readlists search case id must be unique: {id}",
        );
        assert_eq!(
            case.method, "GET",
            "phase10 readlists search cases stay GET-only: {id}",
        );
        assert_eq!(
            case.body, None,
            "phase10 readlists search cases must stay body-less: {id}",
        );
        assert!(
            case.setup.is_none(),
            "phase10 readlists search compat cases must stay self-contained without setup blocks: {id}",
        );
        assert_eq!(
            PathBuf::from(&config.output_dir).join(format!("{id}.json")),
            PathBuf::from("target/compat-diff").join(format!("{id}.json")),
            "phase10 readlists search diff evidence path contract changed for {id}",
        );
        assert_eq!(
            case.headers
                .as_ref()
                .and_then(|headers| headers.get("X-Auth-Token")),
            Some(&"${SESSION_TOKEN}".to_string()),
            "phase10 readlists search compat cases must use explicit session-token wiring: {id}",
        );

        let ownership_header = case
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership"));

        if owned_case_ids.contains(id) {
            assert_eq!(
                ownership_header, None,
                "phase10 owned search case must not carry a shadow marker: {id}",
            );
        } else {
            assert_eq!(
                ownership_header,
                Some(&"shadow-java-writer".to_string()),
                "phase10 negative search case must carry a shadow marker: {id}",
            );
        }
    }

    let default_search = config
        .cases
        .iter()
        .find(|it| it.id == "P10-READLISTS-SEARCH-DEFAULT-OWNED")
        .expect("phase10 default search case should exist");
    let page_size = config
        .cases
        .iter()
        .find(|it| it.id == "P10-READLISTS-SEARCH-PAGE-SIZE-OWNED")
        .expect("phase10 page/size search case should exist");
    let repeated_library = config
        .cases
        .iter()
        .find(|it| it.id == "P10-READLISTS-SEARCH-REPEATED-LIBRARY-ID-OWNED")
        .expect("phase10 repeated library search case should exist");
    let repeated_library_page_size = config
        .cases
        .iter()
        .find(|it| it.id == "P10-READLISTS-SEARCH-REPEATED-LIBRARY-ID-PAGE-SIZE-OWNED")
        .expect("phase10 repeated library + page/size search case should exist");
    let size_zero = config
        .cases
        .iter()
        .find(|it| it.id == "P10-READLISTS-SEARCH-SIZE-ZERO-OWNED")
        .expect("phase10 size=0 search case should exist");
    let repeated_library_size_zero = config
        .cases
        .iter()
        .find(|it| it.id == "P10-READLISTS-SEARCH-REPEATED-LIBRARY-ID-SIZE-ZERO-OWNED")
        .expect("phase10 repeated library size=0 search case should exist");
    let no_results = config
        .cases
        .iter()
        .find(|it| it.id == "P10-READLISTS-SEARCH-NO-RESULTS-OWNED")
        .expect("phase10 no-results search case should exist");
    let blank = config
        .cases
        .iter()
        .find(|it| it.id == "P10-READLISTS-SEARCH-NEGATIVE-BLANK")
        .expect("phase10 blank search exclusion case should exist");
    let whitespace = config
        .cases
        .iter()
        .find(|it| it.id == "P10-READLISTS-SEARCH-NEGATIVE-WHITESPACE")
        .expect("phase10 whitespace search exclusion case should exist");
    let sort = config
        .cases
        .iter()
        .find(|it| it.id == "P10-READLISTS-SEARCH-NEGATIVE-SORT")
        .expect("phase10 sort exclusion case should exist");
    let unpaged_true = config
        .cases
        .iter()
        .find(|it| it.id == "P10-READLISTS-SEARCH-NEGATIVE-UNPAGED-TRUE")
        .expect("phase10 unpaged=true exclusion case should exist");
    let duplicate_page = config
        .cases
        .iter()
        .find(|it| it.id == "P10-READLISTS-SEARCH-NEGATIVE-DUPLICATE-PAGE")
        .expect("phase10 duplicate page exclusion case should exist");
    let duplicate_size = config
        .cases
        .iter()
        .find(|it| it.id == "P10-READLISTS-SEARCH-NEGATIVE-DUPLICATE-SIZE")
        .expect("phase10 duplicate size exclusion case should exist");
    let extra = config
        .cases
        .iter()
        .find(|it| it.id == "P10-READLISTS-SEARCH-NEGATIVE-UNSUPPORTED-EXTRA")
        .expect("phase10 unsupported extra exclusion case should exist");

    assert_eq!(default_search.path, "/api/v1/readlists?search=alpha");
    assert_eq!(
        page_size.path,
        "/api/v1/readlists?search=alpha&page=1&size=1"
    );
    assert_eq!(
        repeated_library.path,
        "/api/v1/readlists?search=alpha&library_id=1&library_id=2"
    );
    assert_eq!(
        repeated_library_page_size.path,
        "/api/v1/readlists?search=alpha&library_id=1&library_id=2&page=1&size=1"
    );
    assert_eq!(size_zero.path, "/api/v1/readlists?search=alpha&size=0");
    assert_eq!(
        repeated_library_size_zero.path,
        "/api/v1/readlists?search=alpha&library_id=1&library_id=2&size=0"
    );
    assert_eq!(no_results.path, "/api/v1/readlists?search=zzzz-no-match");
    assert_eq!(blank.path, "/api/v1/readlists?search=");
    assert_eq!(whitespace.path, "/api/v1/readlists?search=%20%20");
    assert_eq!(sort.path, "/api/v1/readlists?search=alpha&sort=name,asc");
    assert_eq!(
        unpaged_true.path,
        "/api/v1/readlists?search=alpha&unpaged=true"
    );
    assert_eq!(
        duplicate_page.path,
        "/api/v1/readlists?search=alpha&page=0&page=1"
    );
    assert_eq!(
        duplicate_size.path,
        "/api/v1/readlists?search=alpha&size=20&size=1"
    );
    assert_eq!(extra.path, "/api/v1/readlists?search=alpha&foo=bar");
}

#[test]
fn live_http_json_diff_includes_library_role_cases() {
    let case_ids = live_http_json_case_ids();

    assert!(case_ids.contains(&"KOMGA-P0-LIB-01-ADMIN"));
    assert!(case_ids.contains(&"KOMGA-P0-LIB-01-USER"));
    assert!(case_ids.contains(&"KOMGA-P0-LIB-01-LIMITED"));
}

#[test]
fn live_http_json_diff_includes_api_key_parity_cases() {
    let case_ids = live_http_json_case_ids();

    assert!(case_ids.contains(&"P1-AUTH-APIKEY-UPPER"));
    assert!(case_ids.contains(&"P1-AUTH-APIKEY-LOWER"));
    assert!(case_ids.contains(&"P1-AUTH-APIKEY-INVALID"));
}

#[test]
fn seeded_localdb_smoke_includes_t10_read_progress_and_progression_cases() {
    let config = seeded_localdb_smoke_harness_config();
    let case_ids: Vec<&str> = config.cases.iter().map(|it| it.id.as_str()).collect();

    assert!(case_ids.contains(&"KOMGA-P0-BK-READ-PROGRESS-01"));
    assert!(case_ids.contains(&"P1-BK-READ-PROGRESS-DELETE"));
    assert!(case_ids.contains(&"P1-BK-READ-PROGRESS-404"));
    assert!(case_ids.contains(&"P1-BK-PROGRESSION-VALID"));
    assert!(case_ids.contains(&"P1-BK-PROGRESSION-INVALID"));
}
