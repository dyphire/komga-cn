#[derive(Clone, Debug)]
pub(crate) struct BuildMetadata {
    pub(crate) version: String,
    pub(crate) build_time: String,
    pub(crate) git_branch: Option<String>,
    pub(crate) git_commit_id: Option<String>,
    pub(crate) git_commit_time: Option<String>,
}

pub(crate) fn current_build_metadata() -> BuildMetadata {
    BuildMetadata {
        version: env!("VERSION").to_string(),
        build_time: env!("BUILD_TIME").to_string(),
        git_branch: option_env!("GIT_BRANCH")
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        git_commit_id: option_env!("GIT_COMMIT_ID")
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        git_commit_time: option_env!("GIT_COMMIT_TIME")
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    }
}
