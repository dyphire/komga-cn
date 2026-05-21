mod announcement_port;
mod claim_port;
mod client_settings_port;
mod filesystem_browse_port;
mod font_port;
mod history_port;
mod page_hash_port;
mod syncpoint_port;
mod transient_book_port;

pub use announcement_port::AnnouncementPort;
pub use claim_port::{ClaimInitialAdminUserResult, ClaimPort, CreatedClaimedUser};
pub use client_settings_port::ClientSettingsPort;
pub use filesystem_browse_port::FilesystemBrowsePort;
pub use font_port::FontPort;
pub use history_port::HistoryPort;
pub use page_hash_port::PageHashPort;
pub use syncpoint_port::SyncpointPort;
pub use transient_book_port::{
    TransientBookAnalysis, TransientBookFileMetadata, TransientBookPage, TransientBookPort,
};

/// Supertrait aggregating all operational sub-ports for backward compatibility.
pub trait OperationalSettingsPort:
    AnnouncementPort
    + ClaimPort
    + ClientSettingsPort
    + FilesystemBrowsePort
    + FontPort
    + HistoryPort
    + PageHashPort
    + SyncpointPort
    + TransientBookPort
{
}

impl<T> OperationalSettingsPort for T where
    T: AnnouncementPort
        + ClaimPort
        + ClientSettingsPort
        + FilesystemBrowsePort
        + FontPort
        + HistoryPort
        + PageHashPort
        + SyncpointPort
        + TransientBookPort
{
}
