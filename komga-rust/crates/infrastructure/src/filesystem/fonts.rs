use std::fs;
use std::path::Path;

pub fn list_font_families(fonts_directory: &Path) -> Vec<String> {
    let mut families = fs::read_dir(fonts_directory)
        .ok()
        .into_iter()
        .flat_map(|items| items.filter_map(Result::ok))
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_dir() {
                return None;
            }
            Some(entry.file_name().to_string_lossy().to_string())
        })
        .collect::<Vec<_>>();
    families.sort();
    families
}

pub fn load_font_file(
    fonts_directory: &Path,
    font_family: &str,
    font_file: &str,
) -> Option<Vec<u8>> {
    fs::read(fonts_directory.join(font_family).join(font_file)).ok()
}

pub fn load_font_family_css(fonts_directory: &Path, font_family: &str) -> Option<String> {
    let family_dir = fonts_directory.join(font_family);
    let Ok(entries) = fs::read_dir(&family_dir) else {
        return None;
    };

    let mut blocks = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let file_name = match path.file_name().and_then(|value| value.to_str()) {
            Some(value) => value,
            None => continue,
        };
        let Some(format) = font_format(file_name) else {
            continue;
        };

        let lower = file_name.to_ascii_lowercase();
        let style = if lower.contains("italic") {
            "italic"
        } else {
            "normal"
        };
        let weight = if lower.contains("bold") {
            "bold"
        } else {
            "normal"
        };

        blocks.push(format!(
            "@font-face {{\n  font-family: '{}';\n  src: url('{}') format('{}');\n  font-weight: {};\n  font-style: {};\n}}",
            font_family, file_name, format, weight, style,
        ));
    }

    if blocks.is_empty() {
        return None;
    }

    Some(blocks.join("\n\n"))
}

fn font_extension(file_name: &str) -> Option<&str> {
    file_name
        .rsplit_once('.')
        .map(|(_, extension)| extension)
        .map(|extension| extension.to_ascii_lowercase())
        .filter(|extension| matches!(extension.as_str(), "woff" | "woff2" | "ttf" | "otf"))
        .map(|extension| {
            if extension == "woff2" {
                "woff2"
            } else if extension == "woff" {
                "woff"
            } else if extension == "ttf" {
                "ttf"
            } else {
                "otf"
            }
        })
}

fn font_format(file_name: &str) -> Option<&'static str> {
    match font_extension(file_name) {
        Some("ttf") => Some("truetype"),
        Some("otf") => Some("opentype"),
        Some("woff") => Some("woff"),
        Some("woff2") => Some("woff2"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{list_font_families, load_font_family_css, load_font_file};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_millis();
        std::env::temp_dir().join(format!("{prefix}-{millis}"))
    }

    #[test]
    fn list_font_families_returns_sorted_directories_only() {
        let root = unique_temp_dir("komga-fonts-list");
        fs::create_dir_all(root.join("Beta")).expect("beta dir should be created");
        fs::create_dir_all(root.join("Alpha")).expect("alpha dir should be created");
        fs::write(root.join("ignore.txt"), "not a family").expect("file should be created");

        let families = list_font_families(root.as_path());

        assert_eq!(families, vec!["Alpha".to_string(), "Beta".to_string()]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_font_file_reads_the_requested_font_bytes() {
        let root = unique_temp_dir("komga-fonts-file");
        let family_dir = root.join("Demo");
        fs::create_dir_all(&family_dir).expect("family dir should be created");
        fs::write(family_dir.join("Demo-Regular.ttf"), b"font-bytes")
            .expect("font file should be created");

        let bytes = load_font_file(root.as_path(), "Demo", "Demo-Regular.ttf");

        assert_eq!(bytes.as_deref(), Some(&b"font-bytes"[..]));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_font_family_css_builds_face_blocks_from_font_files() {
        let root = unique_temp_dir("komga-fonts-css");
        let family_dir = root.join("Demo Family");
        fs::create_dir_all(&family_dir).expect("family dir should be created");
        fs::write(family_dir.join("Demo-Regular.ttf"), b"font-bytes")
            .expect("regular font should be created");
        fs::write(family_dir.join("Demo-BoldItalic.woff2"), b"font-bytes")
            .expect("bold italic font should be created");
        fs::write(family_dir.join("notes.txt"), b"ignore")
            .expect("non-font file should be created");

        let css = load_font_family_css(root.as_path(), "Demo Family");

        let css = css.expect("css should be generated");
        assert!(css.contains("@font-face {\n  font-family: 'Demo Family';\n  src: url('Demo-Regular.ttf') format('truetype');\n  font-weight: normal;\n  font-style: normal;\n}"));
        assert!(css.contains("@font-face {\n  font-family: 'Demo Family';\n  src: url('Demo-BoldItalic.woff2') format('woff2');\n  font-weight: bold;\n  font-style: italic;\n}"));
        assert!(!css.contains("notes.txt"));

        let _ = fs::remove_dir_all(root);
    }
}
