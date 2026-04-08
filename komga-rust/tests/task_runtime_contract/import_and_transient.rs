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
    use std::io::Write;

    let file = std::fs::File::create(path).expect("zip-as-epub fixture should be created");
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o644);
    zip.start_file("page-1.png", options)
        .expect("zip-as-epub page entry should be created");
    zip.write_all(b"not-an-image")
        .expect("zip-as-epub page bytes should be written");
    zip.finish()
        .expect("zip-as-epub fixture should finish successfully");
}

#[path = "import_and_transient/import.rs"]
mod import;
#[path = "import_and_transient/transient.rs"]
mod transient;
