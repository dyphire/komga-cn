use super::*;

fn resolve_server_port(
    env: &BTreeMap<String, String>,
    layered: &LayeredConfig,
) -> Result<u16, ConfigError> {
    if let Some(raw) = env.get(SERVER_PORT_ENV) {
        return parse_port(raw);
    }

    if let Ok(port) = layered.get_int("server.port") {
        return u16::try_from(port).map_err(|_| ConfigError::InvalidPort(port.to_string()));
    }

    Ok(25600)
}

pub(crate) fn resolve_bind_address_and_context_path(
    cli: &RuntimeCli,
    env: &BTreeMap<String, String>,
    layered: &LayeredConfig,
) -> Result<(SocketAddr, String), ConfigError> {
    let bind_address = match preferred_string(
        cli.address.as_deref(),
        env.get(ADDR_ENV).map(String::as_str),
    ) {
        Some(raw) => raw.parse().map_err(ConfigError::InvalidAddress)?,
        None => SocketAddr::from(([127, 0, 0, 1], resolve_server_port(env, layered)?)),
    };

    let server_context_path =
        preferred_string(None, env.get(SERVER_CONTEXT_PATH_ENV).map(String::as_str))
            .map(str::to_string)
            .or_else(|| {
                read_string(
                    layered,
                    &["server.servlet.context-path", "server.servlet.context.path"],
                )
            })
            .unwrap_or_default();
    if !is_valid_startup_context_path(&server_context_path) {
        return Err(ConfigError::InvalidContextPath(server_context_path));
    }

    Ok((bind_address, server_context_path))
}

fn parse_port(raw: &str) -> Result<u16, ConfigError> {
    raw.trim()
        .parse::<u16>()
        .map_err(|_| ConfigError::InvalidPort(raw.to_string()))
}

pub(crate) fn is_valid_startup_context_path(value: &str) -> bool {
    if value.is_empty() {
        return true;
    }

    if !value.starts_with('/') || value.ends_with('/') {
        return false;
    }

    value
        .chars()
        .all(|ch| ch == '/' || ch == '-' || ch == '_' || ch.is_ascii_alphanumeric())
}
