use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryListingDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub directories: Vec<FilesystemEntryDto>,
    pub files: Vec<FilesystemEntryDto>,
}

#[derive(Debug, Serialize)]
pub struct FilesystemEntryDto {
    #[serde(rename = "type")]
    pub entry_type: FilesystemEntryTypeDto,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub enum FilesystemEntryTypeDto {
    #[serde(rename = "directory")]
    Directory,
    #[serde(rename = "file")]
    File,
}
