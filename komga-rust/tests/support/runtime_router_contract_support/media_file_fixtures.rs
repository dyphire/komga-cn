use std::fs::File;
use std::io::Write;

use komga_rust::infrastructure::sqlite::connect_pool;
use lopdf::{Document as PdfDocument, Object, Stream, dictionary};
use zip::CompressionMethod;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use super::RuntimeDbPaths;

pub async fn update_router_book_name(paths: &RuntimeDbPaths, book_id: &str, name: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open for book rename");

    sqlx::query("UPDATE BOOK SET NAME = ? WHERE ID = ?")
        .bind(name)
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("book name should be updated");

    pool.close().await;
}

pub fn write_router_epub_resource(
    paths: &RuntimeDbPaths,
    relative_book_path: &str,
    resource_name: &str,
    resource_bytes: &[u8],
) {
    let epub_path = paths.config_dir.join(relative_book_path);
    if let Some(parent) = epub_path.parent() {
        std::fs::create_dir_all(parent).expect("epub parent directory should be created");
    }

    let file = File::create(&epub_path).expect("epub fixture file should be created");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);

    zip.start_file("mimetype", options)
        .expect("mimetype entry should be created");
    zip.write_all(b"application/epub+zip")
        .expect("mimetype payload should be written");

    zip.start_file("META-INF/container.xml", options)
        .expect("container entry should be created");
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
    )
    .expect("container payload should be written");

    zip.start_file("OEBPS/content.opf", options)
        .expect("package entry should be created");
    let media_type = if resource_name.ends_with(".html") || resource_name.ends_with(".xhtml") {
        "application/xhtml+xml"
    } else if resource_name.ends_with(".css") {
        "text/css"
    } else if resource_name.ends_with(".svg") {
        "image/svg+xml"
    } else if resource_name.ends_with(".png") {
        "image/png"
    } else if resource_name.ends_with(".jpg") || resource_name.ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "application/octet-stream"
    };
    let package = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><package version=\"3.0\" xmlns=\"http://www.idpf.org/2007/opf\" unique-identifier=\"bookid\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:identifier id=\"bookid\">book-1</dc:identifier><dc:title>Fixture Book</dc:title><dc:language>en</dc:language></metadata><manifest><item id=\"main\" href=\"{}\" media-type=\"{}\"/></manifest><spine><itemref idref=\"main\"/></spine></package>",
        resource_name, media_type,
    );
    zip.write_all(package.as_bytes())
        .expect("package payload should be written");

    zip.start_file(resource_name, options)
        .expect("resource entry should be created");
    zip.write_all(resource_bytes)
        .expect("resource payload should be written");

    zip.finish()
        .expect("epub fixture should finish successfully");
}

pub fn write_single_page_pdf_fixture(path: &std::path::Path) {
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

pub fn fixture_png_bytes() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8,
        0xCF, 0xC0, 0xF0, 0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00,
        0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}

pub fn multipart_image_upload_body(
    field_name: &str,
    file_name: &str,
    media_type: &str,
    selected: bool,
    bytes: &[u8],
) -> (String, Vec<u8>) {
    let boundary = "komga-rust-thumbnail-boundary";
    let mut body = Vec::new();
    write!(
        &mut body,
        "--{boundary}\r\nContent-Disposition: form-data; name=\"{field_name}\"; filename=\"{file_name}\"\r\nContent-Type: {media_type}\r\n\r\n"
    )
    .expect("multipart file prelude should be written");
    body.extend_from_slice(bytes);
    write!(
        &mut body,
        "\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"selected\"\r\n\r\n{}\r\n--{boundary}--\r\n",
        if selected { "true" } else { "false" }
    )
    .expect("multipart selected field should be written");

    (format!("multipart/form-data; boundary={boundary}"), body)
}
