#[derive(Clone, PartialEq, Eq)]
struct FontCharacteristics {
    style: &'static str,
    weight: &'static str,
}

struct FontFileGroup {
    characteristics: FontCharacteristics,
    files: Vec<String>,
}

pub fn is_supported_font_file(file_name: &str) -> bool {
    font_extension(file_name).is_some()
}

pub fn font_media_type(file_name: &str) -> Option<&'static str> {
    match font_extension(file_name) {
        Some("woff") => Some("font/woff"),
        Some("woff2") => Some("font/woff2"),
        Some("ttf") => Some("font/ttf"),
        Some("otf") => Some("font/otf"),
        _ => None,
    }
}

pub fn build_font_family_css(
    font_family: &str,
    font_files: impl IntoIterator<Item = String>,
) -> Option<String> {
    let mut font_files = font_files
        .into_iter()
        .filter(|file_name| font_format(file_name).is_some())
        .collect::<Vec<_>>();
    if font_files.is_empty() {
        return None;
    }

    font_files.sort_by_key(|file_name| file_name.to_ascii_lowercase());

    let mut groups = Vec::<FontFileGroup>::new();
    for file_name in font_files {
        let characteristics = font_characteristics(&file_name);
        if let Some(group) = groups
            .iter_mut()
            .find(|group| group.characteristics == characteristics)
        {
            group.files.push(file_name);
        } else {
            groups.push(FontFileGroup {
                characteristics,
                files: vec![file_name],
            });
        }
    }

    Some(
        groups
            .into_iter()
            .map(|group| build_font_face_block(font_family, &group.characteristics, &group.files))
            .collect::<Vec<_>>()
            .join("\n"),
    )
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

fn font_characteristics(file_name: &str) -> FontCharacteristics {
    let lower = file_name.to_ascii_lowercase();
    FontCharacteristics {
        style: if lower.contains("italic") {
            "italic"
        } else {
            "normal"
        },
        weight: if lower.contains("bold") {
            "bold"
        } else {
            "normal"
        },
    }
}

fn build_font_face_block(
    font_family: &str,
    characteristics: &FontCharacteristics,
    files: &[String],
) -> String {
    let src = files
        .iter()
        .map(|file_name| {
            format!(
                "url('{}') format('{}')",
                file_name,
                font_format(file_name).expect("font format should exist for grouped files")
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    format!(
        "@font-face {{\n    font-family: '{}';\n    src: {};\n    font-weight: {};\n    font-style: {};\n}}\n",
        font_family, src, characteristics.weight, characteristics.style,
    )
}

#[cfg(test)]
mod tests {
    use super::{build_font_family_css, font_media_type, is_supported_font_file};

    #[test]
    fn build_font_family_css_groups_sorted_supported_fonts() {
        let css = build_font_family_css(
            "Demo Family",
            vec![
                "notes.txt".to_string(),
                "Demo-Regular.ttf".to_string(),
                "Demo-BoldItalic.woff2".to_string(),
                "Demo-BoldItalic.woff".to_string(),
            ],
        )
        .expect("supported fonts should produce CSS");

        assert_eq!(
            css,
            "@font-face {\n    font-family: 'Demo Family';\n    src: url('Demo-BoldItalic.woff') format('woff'),url('Demo-BoldItalic.woff2') format('woff2');\n    font-weight: bold;\n    font-style: italic;\n}\n\n@font-face {\n    font-family: 'Demo Family';\n    src: url('Demo-Regular.ttf') format('truetype');\n    font-weight: normal;\n    font-style: normal;\n}\n"
        );
    }

    #[test]
    fn supported_font_helpers_reject_unknown_extensions() {
        assert!(is_supported_font_file("Demo-Regular.ttf"));
        assert_eq!(font_media_type("Demo-Bold.woff2"), Some("font/woff2"));
        assert!(!is_supported_font_file("notes.txt"));
        assert_eq!(font_media_type("notes.txt"), None);
        assert_eq!(
            build_font_family_css("Demo", vec!["notes.txt".to_string()]),
            None
        );
    }
}
