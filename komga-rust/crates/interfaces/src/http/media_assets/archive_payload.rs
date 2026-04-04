use std::io::{Cursor, Write};

use zip::CompressionMethod;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

pub(super) fn build_stored_zip_archive(entries: Vec<(String, Vec<u8>)>) -> Result<Vec<u8>, String> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    writer.set_zip64_comment(Some(""));

    for (file_name, bytes) in entries {
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .large_file(true);
        writer
            .start_file(file_name.as_str(), options)
            .map_err(|error| format!("start zip entry '{file_name}': {error}"))?;
        writer
            .write_all(&bytes)
            .map_err(|error| format!("write zip entry '{file_name}': {error}"))?;
    }

    writer
        .finish()
        .map(|cursor| cursor.into_inner())
        .map_err(|error| format!("finalize zip archive: {error}"))
}

pub(super) fn readlist_archive_entry_name(index: usize, file_name: &str) -> String {
    let visible_name = file_name
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(file_name);
    format!("{} - {}", index + 1, visible_name)
}

#[cfg(test)]
mod tests {
    use super::{build_stored_zip_archive, readlist_archive_entry_name};
    use zip::CompressionMethod;

    #[test]
    fn readlist_archive_entry_name_adds_one_based_prefix() {
        assert_eq!(readlist_archive_entry_name(0, "issue.cbz"), "1 - issue.cbz");
        assert_eq!(
            readlist_archive_entry_name(4, "issue-5.cbz"),
            "5 - issue-5.cbz"
        );
        assert_eq!(
            readlist_archive_entry_name(1, "books/issue-2.cbz"),
            "2 - issue-2.cbz"
        );
    }

    #[test]
    fn build_stored_zip_archive_writes_deflated_entries() {
        let body =
            build_stored_zip_archive(vec![("issue.cbz".to_string(), b"hello world".to_vec())])
                .expect("zip archive should build");
        let cursor = std::io::Cursor::new(body);
        let mut zip = zip::ZipArchive::new(cursor).expect("zip archive should parse");
        let entry = zip.by_index(0).expect("zip entry should open");

        assert_eq!(entry.compression(), CompressionMethod::Deflated);
    }

    #[test]
    fn build_stored_zip_archive_emits_zip64_records() {
        let body =
            build_stored_zip_archive(vec![("issue.cbz".to_string(), b"hello world".to_vec())])
                .expect("zip archive should build");

        assert!(
            body.windows(4)
                .any(|window| window == [0x50, 0x4b, 0x06, 0x06]),
            "zip64 end of central directory signature should be present"
        );
        assert!(
            body.windows(4)
                .any(|window| window == [0x50, 0x4b, 0x06, 0x07]),
            "zip64 end of central directory locator signature should be present"
        );
    }
}
