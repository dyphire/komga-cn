use super::*;

fn assert_spring_bad_request(payload: &Value, message: &str, path: &str) {
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Bad Request".to_string()))
    );
    assert_eq!(
        payload.get("message"),
        Some(&Value::String(message.to_string()))
    );
    assert_eq!(payload.get("status"), Some(&Value::from(400)));
    assert_eq!(payload.get("path"), Some(&Value::String(path.to_string())));
    assert!(
        payload.get("timestamp").and_then(Value::as_u64).is_some(),
        "expected numeric timestamp in spring-style error payload: {payload:?}"
    );
}

fn assert_json_error(payload: &Value, message: &str) {
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(message.to_string()))
    );
}

fn transient_scan_payload(path: &std::path::Path) -> String {
    json!({
        "path": path.to_string_lossy().to_string(),
    })
    .to_string()
}

fn transient_id_from_scan_payload(payload: &Value, context: &str) -> String {
    payload
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{context} should yield an id"))
        .to_string()
}

fn unique_transient_dir(case: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "komga-transient-route-{case}-{}-{nanos}",
        std::process::id()
    ))
}

fn write_zip_as_epub(path: &std::path::Path) {
    write_zip_with_entries(path, &[("page-1.png", b"not-an-image")]);
}

fn write_zip_with_entries(path: &std::path::Path, entries: &[(&str, &[u8])]) {
    use std::io::Write;

    let file = std::fs::File::create(path).expect("zip fixture should be created");
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o644);

    for (entry_name, entry_bytes) in entries {
        zip.start_file(*entry_name, options)
            .expect("zip fixture entry should be created");
        zip.write_all(entry_bytes)
            .expect("zip fixture entry bytes should be written");
    }

    zip.finish()
        .expect("zip fixture should finish successfully");
}

fn write_epub_with_package(
    path: &std::path::Path,
    package_document: &str,
    resources: &[(&str, &[u8])],
) {
    use std::io::Write;

    let file = std::fs::File::create(path).expect("custom epub fixture should be created");
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o644);

    zip.start_file("mimetype", options)
        .expect("custom epub mimetype entry should be created");
    zip.write_all(b"application/epub+zip")
        .expect("custom epub mimetype should be written");

    zip.start_file("META-INF/container.xml", options)
        .expect("custom epub container entry should be created");
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
    )
    .expect("custom epub container should be written");

    zip.start_file("OEBPS/content.opf", options)
        .expect("custom epub package entry should be created");
    zip.write_all(package_document.as_bytes())
        .expect("custom epub package should be written");

    for (resource_name, resource_bytes) in resources {
        zip.start_file(resource_name, options)
            .expect("custom epub resource entry should be created");
        zip.write_all(resource_bytes)
            .expect("custom epub resource payload should be written");
    }

    zip.finish()
        .expect("custom epub fixture should finish successfully");
}

fn write_cbz_with_entries(path: &std::path::Path, entries: &[(&str, &[u8])]) {
    write_zip_with_entries(path, entries);
}

mod import;
mod transient;
