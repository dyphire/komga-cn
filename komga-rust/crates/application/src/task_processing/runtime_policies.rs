#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CleanupEmptySetsPolicy {
    pub delete_empty_collections: bool,
    pub delete_empty_read_lists: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThumbnailRegenerationPolicy {
    pub generated_thumbnail_max_edge: u32,
}

impl Default for ThumbnailRegenerationPolicy {
    fn default() -> Self {
        Self {
            generated_thumbnail_max_edge: 300,
        }
    }
}
