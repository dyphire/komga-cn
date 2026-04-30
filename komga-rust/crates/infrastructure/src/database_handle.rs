use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use crate::sqlite::{connect_read_pool, connect_write_pool};

#[derive(Clone, Debug)]
pub struct DatabaseHandle {
    database_file: PathBuf,
    read_pool: SqlitePool,
    write_pool: SqlitePool,
}

impl DatabaseHandle {
    pub async fn file_backed(database_file: PathBuf) -> Result<Self, sqlx::Error> {
        let read_pool = connect_read_pool(&database_file).await?;
        let write_pool = connect_write_pool(&database_file).await?;
        Ok(Self {
            database_file,
            read_pool,
            write_pool,
        })
    }

    pub fn new(database_file: PathBuf, read_pool: SqlitePool, write_pool: SqlitePool) -> Self {
        Self {
            database_file,
            read_pool,
            write_pool,
        }
    }

    pub fn single_pool(database_file: PathBuf, pool: SqlitePool) -> Self {
        Self {
            database_file,
            read_pool: pool.clone(),
            write_pool: pool,
        }
    }

    pub fn database_file(&self) -> &Path {
        self.database_file.as_path()
    }

    pub fn read_pool(&self) -> &SqlitePool {
        &self.read_pool
    }

    pub fn write_pool(&self) -> &SqlitePool {
        &self.write_pool
    }
}
