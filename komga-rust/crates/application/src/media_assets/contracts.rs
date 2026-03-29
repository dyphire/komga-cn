use std::future::Future;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaAssetQuery {
    pub book_id: String,
    pub page_number: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaAssetResource {
    pub content_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaAssetsError {
    pub message: String,
}

pub trait MediaAssetReadModelPort {
    fn resolve_asset(
        &self,
        query: &MediaAssetQuery,
    ) -> impl Future<Output = Result<Option<MediaAssetResource>, MediaAssetsError>>;
}

pub struct MediaAssetsUseCases<R> {
    read_model: R,
}

impl<R> MediaAssetsUseCases<R>
where
    R: MediaAssetReadModelPort,
{
    pub fn new(read_model: R) -> Self {
        Self { read_model }
    }

    pub async fn resolve_asset(
        &self,
        query: &MediaAssetQuery,
    ) -> Result<Option<MediaAssetResource>, MediaAssetsError> {
        self.read_model.resolve_asset(query).await
    }
}
