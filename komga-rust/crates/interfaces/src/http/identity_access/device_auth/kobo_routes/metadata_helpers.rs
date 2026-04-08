use super::*;

pub(super) fn kobo_description(summary: &str) -> Value {
    if summary.trim().is_empty() {
        Value::String(" ".to_string())
    } else {
        Value::String(summary.to_string())
    }
}

pub(super) fn kobo_language(language: &str) -> String {
    let language = language.trim();
    if language.is_empty() {
        "en".to_string()
    } else {
        language
            .chars()
            .take(2)
            .collect::<String>()
            .to_ascii_lowercase()
    }
}

pub(super) fn kobo_publication_date_value(value: &str) -> Option<Value> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else if value.len() == 10 && value.as_bytes().get(4) == Some(&b'-') {
        Some(Value::String(format!("{value}T00:00:00Z")))
    } else {
        Some(Value::String(value.to_string()))
    }
}

pub(super) async fn built_in_kepub_conversion_available(state: &OperationalState) -> bool {
    let persisted = state.settings_store.load_map().await.ok();
    let configured_path = persisted
        .as_ref()
        .and_then(|settings| settings.get("KEPUBIFY_PATH"))
        .and_then(|value| value.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let fallback_path = state.runtime.config_dir.as_ref().and_then(|config_dir| {
        config_kepubify_path(config_dir.join("application.yml").as_path())
            .or_else(|| config_kepubify_path(config_dir.join("application.yaml").as_path()))
            .or_else(|| {
                properties_kepubify_path(config_dir.join("application.properties").as_path())
            })
    });

    if let Some(path) = configured_path {
        if kepubify_path_is_available(path.as_str()) {
            return true;
        }
        return fallback_path
            .as_deref()
            .is_some_and(kepubify_path_is_available);
    }

    fallback_path
        .as_deref()
        .is_some_and(kepubify_path_is_available)
}

fn config_kepubify_path(path: &FsPath) -> Option<String> {
    std::fs::read_to_string(path).ok().and_then(|content| {
        content.lines().find_map(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with("kepubify-path:") {
                return None;
            }
            trimmed
                .split_once(':')
                .map(|(_, value)| {
                    value
                        .trim()
                        .trim_matches(|character| character == '"' || character == '\'')
                        .to_string()
                })
                .filter(|value| !value.is_empty())
        })
    })
}

fn properties_kepubify_path(path: &FsPath) -> Option<String> {
    std::fs::read_to_string(path).ok().and_then(|content| {
        content.lines().find_map(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with("komga.kobo.kepubify-path=") {
                return None;
            }
            trimmed
                .split_once('=')
                .map(|(_, value)| {
                    value
                        .trim()
                        .trim_matches(|character| character == '"' || character == '\'')
                        .to_string()
                })
                .filter(|value| !value.is_empty())
        })
    })
}

fn kepubify_path_is_available(path: &str) -> bool {
    let candidate = std::path::Path::new(path);
    if (candidate.is_absolute() || path.contains(std::path::MAIN_SEPARATOR))
        && let Ok(metadata) = std::fs::metadata(candidate)
    {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            return metadata.is_file() && metadata.permissions().mode() & 0o111 != 0;
        }
        #[cfg(not(unix))]
        {
            return metadata.is_file();
        }
    }

    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };

    std::env::split_paths(&paths)
        .map(|directory| directory.join(path))
        .any(|candidate| {
            std::fs::metadata(&candidate).ok().is_some_and(|metadata| {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
                }
                #[cfg(not(unix))]
                {
                    metadata.is_file()
                }
            })
        })
}
