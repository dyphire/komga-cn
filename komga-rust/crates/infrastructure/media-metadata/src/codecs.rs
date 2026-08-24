use komga_domain::media_assets::ThumbnailType;

pub fn parse_thumbnail_type(value: &str) -> ThumbnailType {
    ThumbnailType::parse(value).expect("persisted thumbnail type should use a known value")
}
