use std::path::PathBuf;

use crate::filesystem::media_analysis::{self, MediaAnalysisProfile, MediaFileAnalyzer};
pub(in crate::task_queue) use crate::filesystem::media_analysis::{
    expected_extension_for_media_type, is_rar_media_type, is_supported_page_image_file_name,
    media_type_from_entry_name,
};

pub(in crate::task_queue) type BookMediaAnalysis = media_analysis::MediaFileAnalysis;

pub(in crate::task_queue) fn analyze_book_media_file(
    file_path: &PathBuf,
    analyze_dimensions: bool,
) -> Result<BookMediaAnalysis, String> {
    MediaFileAnalyzer.analyze(
        file_path.as_path(),
        MediaAnalysisProfile::PersistedBook {
            include_dimensions: analyze_dimensions,
        },
    )
}
