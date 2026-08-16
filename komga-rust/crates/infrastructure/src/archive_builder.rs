use anyhow::Context;
use std::io::{Cursor, Write};

use komga_application::media_assets::{ArchiveBuilderPort, ArchiveFileEntry};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

pub struct ZipArchiveBuilder;

impl ArchiveBuilderPort for ZipArchiveBuilder {
    fn build_archive(&self, entries: Vec<ArchiveFileEntry>) -> anyhow::Result<Vec<u8>> {
        build_zip_archive(entries)
    }
}

fn build_zip_archive(entries: Vec<ArchiveFileEntry>) -> anyhow::Result<Vec<u8>> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    writer.set_raw_zip64_extensible_data_sector(Vec::new().into_boxed_slice());

    for entry in entries {
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .large_file(true);
        writer
            .start_file(entry.file_name.as_str(), options)
            .map_err(|error| {
                anyhow::anyhow!(error).context(format!("start zip entry '{}'", entry.file_name))
            })?;
        writer.write_all(&entry.bytes).map_err(|error| {
            anyhow::anyhow!(error).context(format!("write zip entry '{}'", entry.file_name))
        })?;
    }

    writer
        .finish()
        .map(|cursor| cursor.into_inner())
        .context("finalize zip archive")
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use komga_application::media_assets::{ArchiveBuilderPort, ArchiveFileEntry};
    use zip::CompressionMethod;

    use super::ZipArchiveBuilder;

    #[test]
    fn zip_archive_builder_deflates_entries_and_emits_zip64_records() {
        let body = ZipArchiveBuilder
            .build_archive(vec![ArchiveFileEntry {
                file_name: "book-1.epub".to_string(),
                bytes: b"book-one".to_vec(),
            }])
            .expect("archive should build");

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
        assert_eq!(
            zip_entries(&body),
            vec![(
                "book-1.epub".to_string(),
                b"book-one".to_vec(),
                CompressionMethod::Deflated,
            )]
        );
    }

    fn zip_entries(body: &[u8]) -> Vec<(String, Vec<u8>, CompressionMethod)> {
        let cursor = std::io::Cursor::new(body);
        let mut archive = zip::ZipArchive::new(cursor).expect("archive should parse");

        (0..archive.len())
            .map(|index| {
                let mut entry = archive.by_index(index).expect("zip entry should open");
                let name = entry
                    .name()
                    .expect("zip entry name should decode")
                    .to_string();
                let compression = entry.compression();
                let mut bytes = Vec::new();
                entry
                    .read_to_end(&mut bytes)
                    .expect("zip entry bytes should read");
                (name, bytes, compression)
            })
            .collect()
    }
}
