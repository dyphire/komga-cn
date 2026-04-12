use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

mod browser;
mod fonts;
mod import;
mod media_access;
mod transient_books;

pub use browser::list_directory_entries;
pub use fonts::{list_font_families, load_font_family_css, load_font_file};
pub use import::FilesystemImportPort;
pub use media_access::*;
pub use transient_books::{
    TransientBookPage, analyze_transient_book, infer_transient_series_and_number,
    list_transient_book_entries, load_transient_book_file_metadata, load_transient_book_media,
    transient_book_content_type, transient_book_exists, transient_book_media_type,
    transient_book_page_content, validate_transient_scan_root,
};

pub(crate) fn remove_file_after_release(path: &Path) -> io::Result<bool> {
    let deadline = Instant::now() + Duration::from_millis(250);

    loop {
        match std::fs::remove_file(path) {
            Ok(()) => return Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error)
                if Instant::now() < deadline && is_transient_windows_share_violation(&error) =>
            {
                // SQLite and UnRAR can release their Windows file handles a moment after the
                // higher-level close/read call returns, so cleanup has to wait on the OS-visible
                // release instead of assuming the file is immediately removable.
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }
}

fn is_transient_windows_share_violation(error: &io::Error) -> bool {
    #[cfg(windows)]
    {
        matches!(error.raw_os_error(), Some(32 | 33))
    }

    #[cfg(not(windows))]
    {
        let _ = error;
        false
    }
}
