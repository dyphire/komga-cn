use komga_application::media_assets::{BookMediaRecord, BookPageRecord};

use super::test_backend;

pub(crate) fn resolve_book_page_bytes(
    media: &BookMediaRecord,
    page: &BookPageRecord,
    page_number: u64,
) -> Option<Vec<u8>> {
    test_backend::resolve_book_page_bytes_for_tests(media, page, page_number)
}

pub(crate) fn load_archive_page_rows(media: &BookMediaRecord) -> Option<Vec<BookPageRecord>> {
    test_backend::load_archive_page_rows_for_tests(media)
}

pub(crate) fn load_generated_pdf_page_rows(media: &BookMediaRecord) -> Vec<BookPageRecord> {
    test_backend::load_generated_pdf_page_rows_for_tests(media)
}

pub(crate) fn read_pdf_page_as_single_page_pdf(
    media: &BookMediaRecord,
    page_number: u64,
) -> Option<Vec<u8>> {
    test_backend::read_pdf_page_as_single_page_pdf_for_tests(media, page_number)
}
