use crate::state::PersistedDiscoveryService;

use super::*;

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|candidate| candidate == value) {
        values.push(value.to_string());
    }
}

pub async fn load_persisted_library_ids(
    backend: &dyn PersistedDiscoveryService,
    database_file: &FsPath,
) -> Result<Vec<String>, String> {
    backend
        .load_persisted_library_ids(database_file.to_path_buf())
        .await
}

pub async fn remap_requested_library_ids_for_persisted(
    backend: &dyn PersistedDiscoveryService,
    database_file: &FsPath,
    requested: Option<&Vec<String>>,
) -> Option<Vec<String>> {
    let requested = requested?;

    if requested.is_empty() || !database_file.exists() {
        return None;
    }

    let persisted_ids = match load_persisted_library_ids(backend, database_file).await {
        Ok(ids) => ids,
        Err(_) => return None,
    };

    if persisted_ids.is_empty() {
        return None;
    }

    let mut normalized = Vec::new();
    for value in requested {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }

        if persisted_ids.iter().any(|candidate| candidate == trimmed) {
            push_unique(&mut normalized, trimmed);
            continue;
        }

        let Some(index) = trimmed.parse::<usize>().ok() else {
            continue;
        };
        if index == 0 {
            continue;
        }

        let Some(mapped) = persisted_ids.get(index - 1) else {
            continue;
        };
        push_unique(&mut normalized, mapped);
    }

    (!normalized.is_empty()).then_some(normalized)
}

pub async fn load_collection_memberships(
    backend: &dyn PersistedDiscoveryService,
    database_file: &FsPath,
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    backend
        .load_collection_memberships(database_file.to_path_buf())
        .await
}

pub async fn load_collection_ordering(
    backend: &dyn PersistedDiscoveryService,
    database_file: &FsPath,
    collection_id: &str,
) -> Result<HashMap<String, i64>, String> {
    backend
        .load_collection_ordering(database_file.to_path_buf(), collection_id.to_string())
        .await
}

pub async fn load_readlist_memberships(
    backend: &dyn PersistedDiscoveryService,
    database_file: &FsPath,
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    backend
        .load_readlist_memberships(database_file.to_path_buf())
        .await
}
