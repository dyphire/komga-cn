use std::env;

/// Keep release-version selection in one crate so every build script embeds the same version string.
pub(crate) fn emit_version_env(fallback_version: &str) {
    println!("cargo:rerun-if-env-changed=VERSION_NEXT");
    println!(
        "cargo:rustc-env=VERSION={}",
        resolve_version(env::var("VERSION_NEXT").ok().as_deref(), fallback_version)
    );
}

fn resolve_version(version_next: Option<&str>, fallback_version: &str) -> String {
    version_next
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback_version)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::resolve_version;

    #[test]
    fn prefers_non_empty_release_version() {
        assert_eq!(resolve_version(Some("1.2.3"), "0.1.0"), "1.2.3");
    }

    #[test]
    fn falls_back_when_release_version_is_missing_or_blank() {
        assert_eq!(resolve_version(None, "0.1.0"), "0.1.0");
        assert_eq!(resolve_version(Some("   	"), "0.1.0"), "0.1.0");
    }
}
