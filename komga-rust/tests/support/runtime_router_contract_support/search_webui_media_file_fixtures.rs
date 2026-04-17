use komga_infrastructure::sqlite::connect_pool;
use lopdf::{Document as PdfDocument, Object, Stream, dictionary};

use super::RuntimeDbPaths;

fn write_single_page_pdf_fixture(path: &std::path::Path) {
    let mut document = PdfDocument::with_version("1.5");
    let pages_id = document.new_object_id();
    let page_id = document.new_object_id();
    let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
    let resources_id = document.add_object(dictionary! {});

    document.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => content_id,
            "Resources" => resources_id,
        }),
    );
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );

    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    document.compress();
    document
        .save(path)
        .expect("single-page pdf fixture should be saved");
}

pub async fn seed_router_pdf_book(
    paths: &RuntimeDbPaths,
    book_id: &str,
    series_id: &str,
    file_name: &str,
    title: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open for pdf book seed");

    let relative_path = format!("books/{file_name}");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(0_i64)
    .bind(file_name)
    .bind(&relative_path)
    .bind(series_id)
    .bind(4_096_i64)
    .bind(99_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("pdf book row should be inserted");

    sqlx::query(
        "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("application/pdf")
    .bind("READY")
    .bind(book_id)
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("pdf media row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("99")
    .bind(99.0_f64)
    .bind(title)
    .bind("2024-02-01")
    .bind(book_id)
    .execute(&pool)
    .await
    .expect("pdf book metadata row should be inserted");

    pool.close().await;

    let pdf_path = paths.config_dir.join(relative_path);
    if let Some(parent) = pdf_path.parent() {
        std::fs::create_dir_all(parent).expect("pdf parent directory should be created");
    }
    write_single_page_pdf_fixture(&pdf_path);
}
