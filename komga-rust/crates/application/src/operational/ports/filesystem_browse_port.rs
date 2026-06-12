#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemBrowseRequest {
    pub path: String,
    pub show_files: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemDirectoryListing {
    pub parent: Option<String>,
    pub directories: Vec<FilesystemEntry>,
    pub files: Vec<FilesystemEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemEntry {
    pub entry_type: FilesystemEntryType,
    pub name: String,
    pub path: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilesystemEntryType {
    Directory,
    File,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilesystemBrowseError {
    BadRequest,
    Internal,
}

pub trait FilesystemBrowsePort: Send + Sync {
    fn browse(
        &self,
        request: FilesystemBrowseRequest,
    ) -> Result<FilesystemDirectoryListing, FilesystemBrowseError>;
}
