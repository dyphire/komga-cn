use super::media_queries::{
    load_library_hashing_flags as load_persisted_library_hashing_flags,
    load_library_maintenance_flags as load_persisted_library_maintenance_flags,
};
use super::*;

pub(in crate::task_queue) struct LibraryHashingFlags {
    pub(in crate::task_queue) hash_files: bool,
    pub(in crate::task_queue) hash_pages: bool,
    pub(in crate::task_queue) hash_koreader: bool,
}

pub(in crate::task_queue) struct LibraryMaintenanceFlags {
    pub(in crate::task_queue) repair_extensions: bool,
}

pub(in crate::task_queue) async fn load_library_hashing_flags(
    runtime: &JobRuntime<'_>,
    library_id: &str,
) -> Result<LibraryHashingFlags, TaskExecutionError> {
    let flags = load_persisted_library_hashing_flags(runtime.database().read_pool(), library_id)
        .await
        .map_err(TaskExecutionError::runtime)?;

    Ok(LibraryHashingFlags {
        hash_files: flags.hash_files,
        hash_pages: flags.hash_pages,
        hash_koreader: flags.hash_koreader,
    })
}

pub(in crate::task_queue) async fn load_library_maintenance_flags(
    runtime: &JobRuntime<'_>,
    library_id: &str,
) -> Result<LibraryMaintenanceFlags, TaskExecutionError> {
    let flags =
        load_persisted_library_maintenance_flags(runtime.database().read_pool(), library_id)
            .await
            .map_err(TaskExecutionError::runtime)?;

    Ok(LibraryMaintenanceFlags {
        repair_extensions: flags.repair_extensions,
    })
}
