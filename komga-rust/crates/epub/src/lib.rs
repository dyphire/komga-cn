mod analysis;
mod mobi;
mod navigation;
mod parse;

pub const EPUB_MEDIA_TYPE: &str = "application/epub+zip";

pub use analysis::{
    EpubAnalysis, EpubAnalysisError, EpubAnalysisFile, EpubAnalysisPage, analyze_epub_file,
};
pub use mobi::{
    ADAPTER_VERSION, MOBI_MEDIA_TYPE, MobiError, MobiUnsupportedReason, NormalizedPublication,
    PublicationChapter, PublicationMetadata, PublicationResource, normalize_mobi,
};
pub use navigation::{EpubNavigation, EpubNavigationLink, decode_epub_navigation_extension};
pub use parse::{
    EpubManifestItem, EpubParseError, EpubSpineItem, normalize_epub_resource_href,
    normalize_epub_zip_path, parse_epub_fixed_layout, parse_epub_fixed_layout_with_heuristic,
    parse_epub_manifest_items, parse_epub_metadata_cover_id, parse_epub_rootfile_path,
    parse_epub_spine_itemrefs, parse_epub_spine_items,
};
