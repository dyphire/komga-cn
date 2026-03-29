use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

use super::task_records::scan_library_task_record;
use super::{
    CreateLibraryResult, LibraryCatalogMutationError, LibraryCatalogMutationPort, LibraryChangeSet,
    LibraryRecord,
};

pub struct CreateLibraryService<P> {
    port: P,
}

impl<P> CreateLibraryService<P>
where
    P: LibraryCatalogMutationPort,
{
    pub fn new(port: P) -> Self {
        Self { port }
    }

    pub async fn create_library(
        &self,
        changes: LibraryChangeSet,
    ) -> Result<CreateLibraryResult, LibraryCatalogMutationError> {
        let mut library = LibraryRecord::default_record(generated_library_id());
        library.apply_changes(changes);
        ensure_name_and_root(&library)?;
        self.port
            .validate_library(&library)
            .await
            .map_err(LibraryCatalogMutationError::Validation)?;
        self.port
            .create_library(&library)
            .await
            .map_err(LibraryCatalogMutationError::persistence)?;

        Ok(CreateLibraryResult {
            task_records: vec![scan_library_task_record(&library.id, false)],
            library,
        })
    }
}

fn ensure_name_and_root(library: &LibraryRecord) -> Result<(), LibraryCatalogMutationError> {
    if library.name.trim().is_empty() || library.root.trim().is_empty() {
        return Err(LibraryCatalogMutationError::Validation(
            "library create payload must provide non-empty name and root".to_string(),
        ));
    }

    Ok(())
}

fn generated_library_id() -> String {
    format!("library-{}", random_hex_token(12))
}

fn random_hex_token(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        let _ = file.read_exact(&mut bytes);
    } else {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = ((seed >> ((index % 8) * 8)) as u8) ^ (index as u8).wrapping_mul(17);
        }
    }

    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
