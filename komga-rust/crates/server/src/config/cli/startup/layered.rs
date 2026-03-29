use super::*;

pub(crate) fn build_layered_config(
    config_dir: &Path,
    env: &BTreeMap<String, String>,
) -> Result<LayeredConfig, ConfigError> {
    let env_source = env
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<HashMap<_, _>>();

    LayeredConfig::builder()
        .set_default("server.port", 25600)
        .map_err(|error| ConfigError::InvalidConfigSource(error.to_string()))?
        .set_default("server.servlet.context-path", "")
        .map_err(|error| ConfigError::InvalidConfigSource(error.to_string()))?
        .set_default("komga.config-dir", config_dir.to_string_lossy().to_string())
        .map_err(|error| ConfigError::InvalidConfigSource(error.to_string()))?
        .add_source(
            ConfigFile::from(config_dir.join("application.yml"))
                .format(FileFormat::Yaml)
                .required(false),
        )
        .add_source(
            ConfigFile::from(config_dir.join("application.yaml"))
                .format(FileFormat::Yaml)
                .required(false),
        )
        .add_source(
            ConfigFile::from(config_dir.join("application.properties"))
                .format(FileFormat::Ini)
                .required(false),
        )
        .add_source(
            Environment::default()
                .separator("_")
                .try_parsing(true)
                .source(Some(env_source)),
        )
        .build()
        .map_err(|error| ConfigError::InvalidConfigSource(error.to_string()))
}
