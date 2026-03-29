use std::path::PathBuf;

#[derive(Debug)]
pub enum ConfigError {
    InvalidAddress(std::net::AddrParseError),
    InvalidPort(String),
    InvalidMode(String),
    InvalidRuntimeProfile(String),
    InvalidPlatformProfile(String),
    InvalidBoolean(String),
    InvalidContextPath(String),
    InvalidConfigSource(String),
    DirectoryCreate {
        path: PathBuf,
        source: std::io::Error,
    },
    InvalidTempDirectory(PathBuf),
    MissingWebUiAssetsLayout {
        candidates: Vec<PathBuf>,
    },
    MixedWriterStorageOwnership {
        details: String,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAddress(error) => write!(f, "invalid KOMGA_RUST_ADDR: {error}"),
            Self::InvalidPort(value) => write!(f, "invalid SERVER_PORT: {value}"),
            Self::InvalidMode(value) => write!(f, "invalid KOMGA_RUST_MODE: {value}"),
            Self::InvalidRuntimeProfile(value) => {
                write!(f, "invalid KOMGA_RUST_RUNTIME_PROFILE: {value}")
            }
            Self::InvalidPlatformProfile(value) => {
                write!(f, "invalid KOMGA_RUST_PLATFORM_PROFILE: {value}")
            }
            Self::InvalidBoolean(value) => {
                write!(f, "invalid isolated write boolean value: {value}")
            }
            Self::InvalidContextPath(_) => write!(
                f,
                "invalid SERVER_SERVLET_CONTEXT_PATH: must be empty or start with '/' and not end with '/'",
            ),
            Self::InvalidConfigSource(value) => {
                write!(f, "invalid runtime startup config source: {value}")
            }
            Self::DirectoryCreate { path, source } => {
                write!(
                    f,
                    "failed to create runtime directory '{}': {source}",
                    path.display()
                )
            }
            Self::InvalidTempDirectory(path) => write!(
                f,
                "invalid temp directory '{}': directory does not exist or is not a directory",
                path.display(),
            ),
            Self::MissingWebUiAssetsLayout { candidates } => write!(
                f,
                "missing WebUI runtime assets layout: expected 'index.html' in one of [{}]",
                candidates
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            Self::MixedWriterStorageOwnership { details } => write!(
                f,
                "unsafe mixed-writer storage ownership detected: {details}. keep a single writer for database.sqlite, tasks.sqlite, and search directory",
            ),
        }
    }
}

impl std::error::Error for ConfigError {}
