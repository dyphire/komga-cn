use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

pub mod browser;
pub mod fonts;
pub mod import;
pub mod media_access;
pub mod transient_books;

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
