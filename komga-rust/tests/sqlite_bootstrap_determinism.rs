use komga_rust::application::discovery::{DiscoveryQueries, LibraryListQuery};
use komga_rust::domain::discovery::DiscoveryQueryContext;
use komga_rust::persistence::discovery::{BookRow, LibraryRow, SeriesRow, SqliteDiscoveryAdapter};
use komga_rust::persistence::sqlite::{fixtures, setup};

#[tokio::test]
async fn explicit_bootstrap_and_compat_seed_are_deterministic() {
    let pool = setup::open_in_memory_database()
        .await
        .expect("sqlite in-memory open should succeed");

    fixtures::insert_minimal_library(&pool, "lib-defaults", "Defaults Library")
        .await
        .expect("minimal library insert should succeed");
    fixtures::insert_minimal_series(&pool, "series-defaults", "lib-defaults", "Defaults Series")
        .await
        .expect("minimal series insert should succeed");
    fixtures::insert_minimal_book(
        &pool,
        "book-defaults",
        "series-defaults",
        "lib-defaults",
        "Defaults Book",
    )
        .await
        .expect("minimal book insert should succeed");

    fixtures::insert_library(&pool, LibraryRow::new("lib-seeded", "Seeded Library"))
        .await
        .expect("compat library seed should succeed");
    fixtures::insert_series(
        &pool,
        SeriesRow::new("series-seeded", "lib-seeded", "Seeded Series")
            .with_labels(["safe"])
            .with_genres(["drama"])
            .with_tags(["featured"])
            .with_authors(["author-1"]),
    )
    .await
    .expect("compat series seed should succeed");
    fixtures::insert_book(
        &pool,
        BookRow::new("book-seeded", "series-seeded", "lib-seeded", "Seeded Book")
            .with_tags(["featured"])
            .with_authors(["author-1"]),
    )
    .await
    .expect("compat book seed should succeed");

    let series_defaults = fixtures::series_defaults(&pool, "series-defaults")
        .await
        .expect("series defaults should be readable");
    assert_eq!(
        series_defaults,
        (
            "2026-01-01T00:00:00Z".to_string(),
            "2026-01-01T00:00:00Z".to_string(),
            "2024-01-02T03:04:05Z".to_string(),
            String::new(),
        )
    );

    let book_defaults = fixtures::book_defaults(&pool, "book-defaults")
        .await
        .expect("book defaults should be readable");
    assert_eq!(
        book_defaults,
        (
            "2024-01-02T03:04:05Z".to_string(),
            "2024-01-02T03:04:05Z".to_string(),
            "2024-01-02T08:04:05Z".to_string(),
            "UNKNOWN".to_string(),
            1,
            String::new(),
        )
    );

    let label_count = fixtures::count_series_label(&pool, "series-seeded", "safe")
        .await
        .expect("compat label seed should be readable");
    let tag_count = fixtures::count_book_tag(&pool, "book-seeded", "featured")
        .await
        .expect("compat tag seed should be readable");
    assert_eq!(label_count, 1);
    assert_eq!(tag_count, 1);

    let mut adapter = SqliteDiscoveryAdapter::new();
    adapter.insert_library(LibraryRow::new("lib-defaults", "Defaults Library"));
    adapter.insert_library(LibraryRow::new("lib-seeded", "Seeded Library"));
    let queries = DiscoveryQueries::new(adapter);
    let libraries = queries
        .list_libraries(&DiscoveryQueryContext::allow_all(), LibraryListQuery {})
        .await
        .expect("query execution should see explicit bootstrap and seed data");

    let library_ids = libraries
        .into_iter()
        .map(|library| library.id)
        .collect::<Vec<_>>();
    assert_eq!(
        library_ids,
        vec!["lib-defaults".to_string(), "lib-seeded".to_string()]
    );
}
