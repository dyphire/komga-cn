use crate::media_assets::{EpubNavigationContentPort, EpubNavigationExtension};

use super::kobo_metadata_pre_paginated;

#[test]
fn kobo_metadata_pre_paginated_uses_epub_navigation_content_boundary() {
    let content = FixedLayoutContent;

    assert!(matches!(
        kobo_metadata_pre_paginated(&content, Some(&[1, 2, 3])),
        Ok(true)
    ));
    assert!(matches!(
        kobo_metadata_pre_paginated(&content, None),
        Ok(false)
    ));
}

#[test]
fn kobo_metadata_pre_paginated_propagates_navigation_decode_errors() {
    let content = FailingContent;

    let error = kobo_metadata_pre_paginated(&content, Some(&[1, 2, 3]))
        .expect_err("invalid persisted EPUB navigation blobs must not become reflowable metadata");

    assert!(
        error.to_string().contains("decode failed"),
        "unexpected pre-paginated decode error: {error}"
    );
}

struct FixedLayoutContent;

impl EpubNavigationContentPort for FixedLayoutContent {
    fn decode_epub_navigation_extension(
        &self,
        _blob: &[u8],
    ) -> anyhow::Result<EpubNavigationExtension> {
        Ok(EpubNavigationExtension {
            is_fixed_layout: true,
            ..EpubNavigationExtension::default()
        })
    }
}

struct FailingContent;

impl EpubNavigationContentPort for FailingContent {
    fn decode_epub_navigation_extension(
        &self,
        _blob: &[u8],
    ) -> anyhow::Result<EpubNavigationExtension> {
        Err(anyhow::anyhow!("decode failed"))
    }
}
