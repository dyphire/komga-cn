use std::collections::BTreeMap;

use komga_application::operational::{
    ClientGlobalSetting, ClientGlobalSettings, ClientUserSetting, ClientUserSettings,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientSettingDto {
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_unauthorized: Option<bool>,
}

impl From<&ClientGlobalSetting> for ClientSettingDto {
    fn from(setting: &ClientGlobalSetting) -> Self {
        Self {
            value: setting.value.clone(),
            allow_unauthorized: Some(setting.allow_unauthorized),
        }
    }
}

impl From<&ClientUserSetting> for ClientSettingDto {
    fn from(setting: &ClientUserSetting) -> Self {
        Self {
            value: setting.value.clone(),
            allow_unauthorized: None,
        }
    }
}

pub fn client_settings_global_dto(
    settings: &ClientGlobalSettings,
) -> BTreeMap<String, ClientSettingDto> {
    settings
        .iter()
        .map(|(key, setting)| (key.clone(), setting.into()))
        .collect()
}

pub fn client_settings_user_dto(
    settings: &ClientUserSettings,
) -> BTreeMap<String, ClientSettingDto> {
    settings
        .iter()
        .map(|(key, setting)| (key.clone(), setting.into()))
        .collect()
}
