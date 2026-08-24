use komga_application::task_processing::TaskProcessingError;

use super::persistence::{
    load_library_hashing_flags as load_persisted_library_hashing_flags,
    load_library_maintenance_flags as load_persisted_library_maintenance_flags,
};
use crate::MediaLibraryJobContext;

pub struct LibraryHashingFlags {
    pub hash_files: bool,
    pub hash_pages: bool,
    pub hash_koreader: bool,
}

pub(crate) struct LibraryMaintenanceFlags {
    pub(crate) repair_extensions: bool,
}

pub async fn load_library_hashing_flags(
    runtime: &MediaLibraryJobContext,
    library_id: &str,
) -> Result<LibraryHashingFlags, TaskProcessingError> {
    let flags = load_persisted_library_hashing_flags(runtime.database().read_pool(), library_id)
        .await
        .map_err(TaskProcessingError::runtime)?;

    Ok(LibraryHashingFlags {
        hash_files: flags.hash_files,
        hash_pages: flags.hash_pages,
        hash_koreader: flags.hash_koreader,
    })
}

pub(crate) async fn load_library_maintenance_flags(
    runtime: &MediaLibraryJobContext,
    library_id: &str,
) -> Result<LibraryMaintenanceFlags, TaskProcessingError> {
    let flags =
        load_persisted_library_maintenance_flags(runtime.database().read_pool(), library_id)
            .await
            .map_err(TaskProcessingError::runtime)?;

    Ok(LibraryMaintenanceFlags {
        repair_extensions: flags.repair_extensions,
    })
}
