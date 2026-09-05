use std::path::Path;

use crate::error::ConfigError;

pub(crate) fn ensure_runtime_directories(
    config_dir: &Path,
    log_file: &Path,
    database_file: &Path,
    riir_db_file: &Path,
    tasks_db_file: &Path,
    lucene_data_directory: &Path,
    fonts_data_directory: &Path,
) -> Result<(), ConfigError> {
    create_dir(config_dir)?;

    if let Some(parent) = log_file.parent() {
        create_dir(parent)?;
    }
    if let Some(parent) = database_file.parent() {
        create_dir(parent)?;
    }
    if let Some(parent) = riir_db_file.parent() {
        create_dir(parent)?;
    }
    if let Some(parent) = tasks_db_file.parent() {
        create_dir(parent)?;
    }
    create_dir(lucene_data_directory)?;
    create_dir(fonts_data_directory)?;

    Ok(())
}

fn create_dir(path: &Path) -> Result<(), ConfigError> {
    std::fs::create_dir_all(path).map_err(|source| ConfigError::DirectoryCreate {
        path: path.to_path_buf(),
        source,
    })
}

pub(crate) fn validate_temp_directory() -> Result<(), ConfigError> {
    let temp_dir = std::env::temp_dir();
    match std::fs::metadata(&temp_dir) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        _ => Err(ConfigError::InvalidTempDirectory(temp_dir)),
    }
}
