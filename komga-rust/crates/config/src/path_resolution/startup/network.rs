use std::collections::BTreeMap;
use std::net::SocketAddr;

use config::Config as LayeredConfig;

use super::paths::{preferred_string, read_string};
use crate::cli_args::{ADDR_ENV, RuntimeCli, SERVER_CONTEXT_PATH_ENV, SERVER_PORT_ENV};
use crate::error::ConfigError;

pub(crate) struct StartupNetworkConfig {
    pub(crate) bind_address: SocketAddr,
    pub(crate) server_context_path: String,
}

fn resolve_server_port(
    env: &BTreeMap<String, String>,
    layered: &LayeredConfig,
) -> Result<u16, ConfigError> {
    if let Some(raw) = env.get(SERVER_PORT_ENV) {
        return parse_port(raw);
    }

    match layered.get_int("server.port") {
        Ok(port) => {
            return u16::try_from(port).map_err(|_| ConfigError::InvalidPort(port.to_string()));
        }
        Err(config::ConfigError::NotFound(_)) => {}
        Err(_) => match layered.get_string("server.port") {
            Ok(port) => return parse_port(&port),
            Err(config::ConfigError::NotFound(_)) => {}
            Err(_) => return Err(ConfigError::InvalidPort("server.port".to_string())),
        },
    }

    Ok(25600)
}

pub(crate) fn resolve_bind_address_and_context_path(
    cli: &RuntimeCli,
    env: &BTreeMap<String, String>,
    layered: &LayeredConfig,
) -> Result<StartupNetworkConfig, ConfigError> {
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

    Ok(StartupNetworkConfig {
        bind_address,
        server_context_path,
    })
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use config::Config as LayeredConfig;

    use crate::error::ConfigError;

    use super::resolve_server_port;

    #[test]
    fn rejects_invalid_server_port_from_application_config() {
        let layered = LayeredConfig::builder()
            .set_override("server.port", "not-a-port")
            .expect("test config override should be accepted")
            .build()
            .expect("test config should build");

        let error = resolve_server_port(&BTreeMap::new(), &layered)
            .expect_err("invalid config server port should fail startup config resolution");

        assert!(matches!(error, ConfigError::InvalidPort(value) if value == "not-a-port"));
    }
}
