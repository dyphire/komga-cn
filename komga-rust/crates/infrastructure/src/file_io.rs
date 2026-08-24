use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

const FILE_RELEASE_RETRY_DEADLINE: Duration = Duration::from_secs(30);
const FILE_RELEASE_RETRY_INTERVAL: Duration = Duration::from_millis(10);

pub fn remove_file_after_release(path: &Path) -> io::Result<bool> {
    let deadline = Instant::now() + FILE_RELEASE_RETRY_DEADLINE;

    loop {
        match std::fs::remove_file(path) {
            Ok(()) => return Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error)
                if Instant::now() < deadline && is_transient_windows_release_error(&error) =>
            {
                // SQLite WAL teardown on Windows can outlive the higher-level close call by a
                // noticeable margin on loaded CI runners. Depending on the exact teardown phase,
                // `remove_file` may surface either a classic sharing violation or a generic
                // `AccessDenied`, so cleanup has to wait for OS-level handle release instead of
                // assuming the file becomes removable immediately.
                std::thread::sleep(FILE_RELEASE_RETRY_INTERVAL);
            }
            Err(error) => return Err(error),
        }
    }
}

fn is_transient_windows_release_error(error: &io::Error) -> bool {
    #[cfg(windows)]
    {
        matches!(error.raw_os_error(), Some(5 | 32 | 33))
    }

    #[cfg(not(windows))]
    {
        let _ = error;
        false
    }
}
