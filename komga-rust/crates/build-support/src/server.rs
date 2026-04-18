use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::version::emit_version_env;

pub fn configure_server_build(manifest_dir: &Path, fallback_version: &str) {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=BUILD_TIME");
    println!("cargo:rerun-if-env-changed=GIT_BRANCH");
    println!("cargo:rerun-if-env-changed=GIT_COMMIT_ID");
    println!("cargo:rerun-if-env-changed=GIT_COMMIT_TIME");

    let repo_root = resolve_repo_root(manifest_dir);
    emit_git_rerun_directives(repo_root.as_deref());

    emit_version_env(fallback_version);
    println!("cargo:rustc-env=BUILD_TIME={}", resolve_build_time());

    if let Some(git_branch) = resolve_git_branch(repo_root.as_deref()) {
        println!("cargo:rustc-env=GIT_BRANCH={git_branch}");
    }

    if let Some(git_commit_id) = resolve_git_commit_id(repo_root.as_deref()) {
        println!("cargo:rustc-env=GIT_COMMIT_ID={git_commit_id}");
    }

    if let Some(git_commit_time) = resolve_git_commit_time(repo_root.as_deref()) {
        println!("cargo:rustc-env=GIT_COMMIT_TIME={git_commit_time}");
    }
}

fn resolve_build_time() -> String {
    env_value("BUILD_TIME").unwrap_or_else(|| {
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .expect("current UTC time should format as RFC3339")
    })
}

fn resolve_git_branch(repo_root: Option<&Path>) -> Option<String> {
    env_value("GIT_BRANCH").or_else(|| git_output(repo_root?, &["branch", "--show-current"]))
}

fn resolve_git_commit_id(repo_root: Option<&Path>) -> Option<String> {
    env_value("GIT_COMMIT_ID").or_else(|| git_output(repo_root?, &["rev-parse", "HEAD"]))
}

fn resolve_git_commit_time(repo_root: Option<&Path>) -> Option<String> {
    env_value("GIT_COMMIT_TIME")
        .or_else(|| git_output(repo_root?, &["show", "-s", "--format=%cI", "HEAD"]))
}

fn resolve_repo_root(manifest_dir: &Path) -> Option<PathBuf> {
    git_output(manifest_dir, &["rev-parse", "--show-toplevel"])
        .map(PathBuf::from)
        .or_else(|| manifest_dir.join("../../..").canonicalize().ok())
}

fn emit_git_rerun_directives(repo_root: Option<&Path>) {
    let Some(repo_root) = repo_root else {
        return;
    };
    let Some(git_dir) = resolve_git_dir(repo_root) else {
        return;
    };

    let head_path = git_dir.join("HEAD");
    println!("cargo:rerun-if-changed={}", head_path.display());

    if let Some(reference) = read_head_reference(&head_path) {
        println!(
            "cargo:rerun-if-changed={}",
            git_dir.join(reference).display()
        );
    }

    println!(
        "cargo:rerun-if-changed={}",
        git_dir.join("packed-refs").display()
    );
}

fn resolve_git_dir(repo_root: &Path) -> Option<PathBuf> {
    let git_entry = repo_root.join(".git");
    let metadata = fs::metadata(&git_entry).ok()?;

    if metadata.is_dir() {
        return Some(git_entry);
    }

    let gitdir = fs::read_to_string(&git_entry).ok()?;
    let gitdir = gitdir.strip_prefix("gitdir:")?.trim();
    let gitdir = PathBuf::from(gitdir);
    Some(if gitdir.is_absolute() {
        gitdir
    } else {
        repo_root.join(gitdir)
    })
}

fn read_head_reference(head_path: &Path) -> Option<PathBuf> {
    let head = fs::read_to_string(head_path).ok()?;
    let reference = head.strip_prefix("ref:")?.trim();
    Some(PathBuf::from(reference))
}

fn git_output(current_dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(current_dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8(output.stdout).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn env_value(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
