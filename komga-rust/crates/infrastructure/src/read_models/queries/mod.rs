pub(super) mod books;
pub(super) mod books_media;
pub(super) mod libraries;
pub(super) mod series;

use komga_domain::discovery::DiscoveryError;

pub(super) fn map_sqlx_error(error: sqlx::Error) -> DiscoveryError {
    DiscoveryError::Persistence(error.to_string())
}
