#[derive(Debug)]
pub enum ConfigError {
    InvalidAddress(std::net::AddrParseError),
    InvalidMode(String),
    InvalidCompatProfile(String),
    InvalidPlatformProfile(String),
    InvalidBoolean(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAddress(error) => write!(f, "invalid KOMGA_RUST_ADDR: {error}"),
            Self::InvalidMode(value) => write!(f, "invalid KOMGA_RUST_MODE: {value}"),
            Self::InvalidCompatProfile(value) => {
                write!(f, "invalid KOMGA_RUST_COMPAT_PROFILE: {value}")
            }
            Self::InvalidPlatformProfile(value) => {
                write!(f, "invalid KOMGA_RUST_PLATFORM_PROFILE: {value}")
            }
            Self::InvalidBoolean(value) => {
                write!(f, "invalid shadow write boolean value: {value}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}
