use super::*;

fn parse_bool(value: &str) -> Result<bool, ConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        other => Err(ConfigError::InvalidBoolean(other.to_string())),
    }
}

pub(crate) fn resolve_writer_ownership_policy_for_startup_slice(
    cli: &RuntimeCli,
    env: &BTreeMap<String, String>,
) -> Result<WriterOwnershipPolicy, ConfigError> {
    let isolation_root = cli
        .writer_isolation_root
        .clone()
        .or_else(|| env.get(WRITER_ISOLATION_ROOT_ENV).map(PathBuf::from));

    let allow_isolated_writes = if cli.allow_isolated_writes {
        true
    } else {
        env.get(ALLOW_ISOLATED_WRITES_ENV)
            .map(String::as_str)
            .map(parse_bool)
            .transpose()?
            .unwrap_or(false)
    };

    Ok(WriterOwnershipPolicy {
        isolation_root,
        allow_isolated_writes,
    })
}
