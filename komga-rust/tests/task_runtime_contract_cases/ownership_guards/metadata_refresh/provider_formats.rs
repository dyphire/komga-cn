use super::*;

const COMICINFO_ONLY_EPUB_PACKAGE: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<package version="3.0" xmlns="http://www.idpf.org/2007/opf" unique-identifier="bookid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="bookid">book-1</dc:identifier>
  </metadata>
  <manifest>
    <item id="main" href="chapter.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="main"/>
  </spine>
</package>"##;

pub(in super::super) fn write_router_epub_with_package_document(
    paths: &RuntimeDbPaths,
    relative_book_path: &str,
    package_document: &str,
) {
    write_router_epub_with_package_document_and_entries(
        paths,
        relative_book_path,
        package_document,
        &[],
    );
}

pub(in super::super) fn write_router_epub_with_comicinfo(
    paths: &RuntimeDbPaths,
    relative_book_path: &str,
    comicinfo: &[u8],
) {
    write_router_epub_with_package_document_and_entries(
        paths,
        relative_book_path,
        COMICINFO_ONLY_EPUB_PACKAGE,
        &[("ComicInfo.xml", comicinfo)],
    );
}

pub(in super::super) fn write_router_epub_with_package_document_and_entries(
    paths: &RuntimeDbPaths,
    relative_book_path: &str,
    package_document: &str,
    extra_entries: &[(&str, &[u8])],
) {
    let epub_path = paths.config_dir.join(relative_book_path);
    if let Some(parent) = epub_path.parent() {
        std::fs::create_dir_all(parent).expect("epub metadata parent directory should be created");
    }

    let file = std::fs::File::create(&epub_path).expect("epub metadata fixture file should exist");
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o644);

    zip.start_file("mimetype", options)
        .expect("epub metadata mimetype entry should be created");
    use std::io::Write;
    zip.write_all(b"application/epub+zip")
        .expect("epub metadata mimetype should be written");

    zip.start_file("META-INF/container.xml", options)
        .expect("epub metadata container entry should be created");
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
    )
    .expect("epub metadata container should be written");

    zip.start_file("OEBPS/content.opf", options)
        .expect("epub metadata package entry should be created");
    zip.write_all(package_document.as_bytes())
        .expect("epub metadata package should be written");

    zip.start_file("OEBPS/chapter.xhtml", options)
        .expect("epub metadata chapter entry should be created");
    zip.write_all(
        br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>EPUB metadata fixture</p></body></html>"#,
    )
    .expect("epub metadata chapter should be written");

    for (entry_name, entry_bytes) in extra_entries {
        zip.start_file(*entry_name, options)
            .expect("epub metadata extra entry should be created");
        zip.write_all(entry_bytes)
            .expect("epub metadata extra entry should be written");
    }

    zip.finish()
        .expect("epub metadata fixture should finish successfully");
}

pub(in super::super) fn write_router_cbz_with_single_page(
    paths: &RuntimeDbPaths,
    relative_book_path: &str,
    page_file_name: &str,
    page_bytes: &[u8],
) {
    let archive_path = paths.config_dir.join(relative_book_path);
    if let Some(parent) = archive_path.parent() {
        std::fs::create_dir_all(parent).expect("cbz barcode parent directory should be created");
    }

    let file = std::fs::File::create(&archive_path).expect("cbz barcode fixture file should exist");
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o644);

    use std::io::Write;
    zip.start_file(page_file_name, options)
        .expect("cbz barcode page entry should be created");
    zip.write_all(page_bytes)
        .expect("cbz barcode page should be written");
    zip.finish()
        .expect("cbz barcode fixture should finish successfully");
}

fn render_ean13_png_bytes(digits: &str) -> Vec<u8> {
    const MODULE_WIDTH: u32 = 4;
    const BAR_HEIGHT: u32 = 140;
    const TOP_MARGIN: u32 = 10;
    const QUIET_ZONE: &str = "0000000000";
    const START_GUARD: &str = "101";
    const MIDDLE_GUARD: &str = "01010";
    const END_GUARD: &str = "101";
    const LEFT_ODD: [&str; 10] = [
        "0001101", "0011001", "0010011", "0111101", "0100011", "0110001", "0101111", "0111011",
        "0110111", "0001011",
    ];
    const LEFT_EVEN: [&str; 10] = [
        "0100111", "0110011", "0011011", "0100001", "0011101", "0111001", "0000101", "0010001",
        "0001001", "0010111",
    ];
    const RIGHT: [&str; 10] = [
        "1110010", "1100110", "1101100", "1000010", "1011100", "1001110", "1010000", "1000100",
        "1001000", "1110100",
    ];
    const PARITY: [&str; 10] = [
        "LLLLLL", "LLGLGG", "LLGGLG", "LLGGGL", "LGLLGG", "LGGLLG", "LGGGLL", "LGLGLG", "LGLGGL",
        "LGGLGL",
    ];

    assert_eq!(digits.len(), 13, "EAN-13 fixture must contain 13 digits");
    let digits = digits
        .chars()
        .map(|digit| digit.to_digit(10).expect("EAN-13 fixture must be numeric") as usize)
        .collect::<Vec<_>>();

    let mut bits = String::from(QUIET_ZONE);
    bits.push_str(START_GUARD);
    let parity = PARITY[digits[0]].as_bytes();
    for (index, digit) in digits[1..7].iter().enumerate() {
        let pattern = if parity[index] == b'L' {
            LEFT_ODD[*digit]
        } else {
            LEFT_EVEN[*digit]
        };
        bits.push_str(pattern);
    }
    bits.push_str(MIDDLE_GUARD);
    for digit in &digits[7..13] {
        bits.push_str(RIGHT[*digit]);
    }
    bits.push_str(END_GUARD);
    bits.push_str(QUIET_ZONE);

    let width = bits.len() as u32 * MODULE_WIDTH;
    let height = BAR_HEIGHT + TOP_MARGIN * 2;
    let mut image = image::GrayImage::from_pixel(width, height, image::Luma([255]));
    for (index, bit) in bits.bytes().enumerate() {
        if bit != b'1' {
            continue;
        }
        let start_x = index as u32 * MODULE_WIDTH;
        for x in start_x..start_x + MODULE_WIDTH {
            for y in TOP_MARGIN..TOP_MARGIN + BAR_HEIGHT {
                image.put_pixel(x, y, image::Luma([0]));
            }
        }
    }

    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageLuma8(image)
        .write_to(&mut output, image::ImageFormat::Png)
        .expect("EAN-13 PNG fixture should encode");
    output.into_inner()
}

#[tokio::test]
async fn runtime_refresh_book_metadata_applies_epub_provider_patch_when_title_capability_matches() {
    let ctx = TestFixture::new("runtime-refresh-book-metadata-applies-epub-provider-patch").await;

    write_router_epub_with_package_document(
        ctx.paths(),
        "books/book-1.epub",
        r##"<?xml version="1.0" encoding="UTF-8"?>
        <package version="3.0" xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf" unique-identifier="bookid">
          <metadata>
            <dc:identifier id="bookid">isbn:9780306406157</dc:identifier>
            <dc:title>EPUB Refresh Title</dc:title>
            <dc:description><![CDATA[<p>EPUB <b>Summary</b></p>]]></dc:description>
            <dc:date>2025-04-06T10:11:12Z</dc:date>
            <dc:creator id="creator-1">Alice Author</dc:creator>
            <meta refines="#creator-1" property="role" scheme="marc:relators">aut</meta>
            <dc:creator opf:role="trl">Bob Translator</dc:creator>
            <meta property="belongs-to-collection" id="series-collection">Series Collection</meta>
            <meta refines="#series-collection" property="group-position">4</meta>
          </metadata>
          <manifest>
            <item id="main" href="chapter.xhtml" media-type="application/xhtml+xml"/>
          </manifest>
          <spine>
            <itemref idref="main"/>
          </spine>
        </package>"##,
    );

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for EPUB metadata fixture setup");
    sqlx::query("DELETE FROM SIDECAR WHERE PARENT_URL = ?")
        .bind("books/book-1.epub")
        .execute(&pool)
        .await
        .expect("book sidecars should be cleared before EPUB metadata refresh test");
    isolate_book_metadata_imports(&pool, 0, 0, 1, 0).await;
    sqlx::query(
        "UPDATE BOOK_METADATA SET RELEASE_DATE = ?, RELEASE_DATE_LOCK = 1, ISBN = ?, ISBN_LOCK = 0, NUMBER = ?, NUMBER_SORT = ? WHERE BOOK_ID = ?",
    )
    .bind("2024-01-15")
    .bind("")
    .bind("1")
    .bind(1.0_f64)
    .bind("book-1")
    .execute(&pool)
    .await
    .expect("book metadata locks should be updated before EPUB metadata refresh test");
    sqlx::query("DELETE FROM BOOK_METADATA_AUTHOR WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("existing metadata authors should be cleared before EPUB metadata refresh test");
    pool.close().await;

    let tasks_pool = connect_test_pool(ctx.paths().tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for EPUB metadata task setup");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind("RefreshBookMetadata_book-1")
    .bind(80_i64)
    .bind("series-1")
    .bind("org.gotson.komga.application.tasks.Task$RefreshBookMetadata")
    .bind("RefreshBookMetadata")
    .bind(
        json!({
            "bookId": "book-1",
            "capabilities": ["TITLE"],
            "priority": 80,
            "groupId": "series-1",
            "uniqueId": "RefreshBookMetadata_book-1"
        })
        .to_string(),
    )
    .execute(&tasks_pool)
    .await
    .expect("EPUB metadata task row should be inserted");
    tasks_pool.close().await;

    let runtime = runtime_task_context(ctx.paths()).await;
    let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .process_available(&runtime.job())
        .await
        .expect("runtime should process EPUB RefreshBookMetadata tasks successfully");

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for EPUB metadata verification");
    let metadata = sqlx::query(
        "SELECT TITLE, SUMMARY, NUMBER, NUMBER_SORT, RELEASE_DATE, ISBN FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1",
    )
    .bind("book-1")
    .fetch_one(&verify_pool)
    .await
    .expect("book metadata row should be queryable after EPUB metadata refresh");
    let authors = sqlx::query(
        "SELECT NAME, ROLE FROM BOOK_METADATA_AUTHOR WHERE BOOK_ID = ? ORDER BY ROLE ASC, NAME ASC",
    )
    .bind("book-1")
    .fetch_all(&verify_pool)
    .await
    .expect("book metadata authors should be queryable after EPUB metadata refresh");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("TITLE"), "EPUB Refresh Title");
    assert_eq!(metadata.get::<String, _>("SUMMARY"), "EPUB Summary");
    assert_eq!(metadata.get::<String, _>("NUMBER"), "4");
    assert_eq!(metadata.get::<f64, _>("NUMBER_SORT"), 4.0_f64);
    assert_eq!(
        metadata.get::<String, _>("RELEASE_DATE"),
        "2024-01-15",
        "EPUB provider refresh must still respect existing releaseDate locks",
    );
    assert_eq!(metadata.get::<String, _>("ISBN"), "9780306406157");
    assert_eq!(
        authors
            .iter()
            .map(|row| (row.get::<String, _>("NAME"), row.get::<String, _>("ROLE")))
            .collect::<Vec<_>>(),
        vec![
            ("Bob Translator".to_string(), "translator".to_string()),
            ("Alice Author".to_string(), "writer".to_string()),
        ],
        "EPUB provider should map OPF creator roles and replace authors when provider capabilities match",
    );
}

#[tokio::test]
async fn runtime_refresh_book_metadata_applies_barcode_isbn_for_non_epub_books() {
    let ctx = TestFixture::new("runtime-refresh-book-metadata-applies-barcode-isbn").await;
    seed_router_cbz_book(
        ctx.paths(),
        "book-barcode-1",
        "series-1",
        "barcode-book.cbz",
        "Barcode Book",
    )
    .await;
    write_router_cbz_with_single_page(
        ctx.paths(),
        "books/barcode-book.cbz",
        "page-1.png",
        &render_ean13_png_bytes("9780306406157"),
    );

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for barcode metadata fixture setup");
    isolate_book_metadata_imports(&pool, 0, 0, 0, 1).await;
    sqlx::query("UPDATE BOOK_METADATA SET ISBN = ?, ISBN_LOCK = 0 WHERE BOOK_ID = ?")
        .bind("")
        .bind("book-barcode-1")
        .execute(&pool)
        .await
        .expect("book metadata isbn should be reset before barcode refresh test");
    pool.close().await;

    let tasks_pool = connect_test_pool(ctx.paths().tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for barcode metadata task setup");
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind("RefreshBookMetadata_book-barcode-1")
    .bind(80_i64)
    .bind("series-1")
    .bind("org.gotson.komga.application.tasks.Task$RefreshBookMetadata")
    .bind("RefreshBookMetadata")
    .bind(
        json!({
            "bookId": "book-barcode-1",
            "capabilities": ["ISBN"],
            "priority": 80,
            "groupId": "series-1",
            "uniqueId": "RefreshBookMetadata_book-barcode-1"
        })
        .to_string(),
    )
    .execute(&tasks_pool)
    .await
    .expect("barcode metadata task row should be inserted");
    tasks_pool.close().await;

    let runtime = runtime_task_context(ctx.paths()).await;
    let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
    scheduler
        .process_available(&runtime.job())
        .await
        .expect("runtime should process barcode RefreshBookMetadata tasks successfully");

    let verify_pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open for barcode metadata verification");
    let metadata = sqlx::query("SELECT ISBN FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1")
        .bind("book-barcode-1")
        .fetch_one(&verify_pool)
        .await
        .expect("book metadata row should be queryable after barcode metadata refresh");
    verify_pool.close().await;

    assert_eq!(metadata.get::<String, _>("ISBN"), "9780306406157");
}
