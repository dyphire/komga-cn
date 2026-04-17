use super::*;

pub(crate) fn resolve_oauth2_clients_for_startup_slice(
    layered: &LayeredConfig,
    env: &BTreeMap<String, String>,
) -> Vec<OAuth2ClientConfig> {
    let mut clients_by_registration_id = resolve_oauth2_clients_from_layered(layered)
        .into_iter()
        .map(|client| (client.registration_id.clone(), client))
        .collect::<BTreeMap<_, _>>();

    for client in resolve_oauth2_clients_from_env(env) {
        clients_by_registration_id.insert(client.registration_id.clone(), client);
    }

    clients_by_registration_id.into_values().collect()
}

fn resolve_oauth2_clients_from_layered(layered: &LayeredConfig) -> Vec<OAuth2ClientConfig> {
    let Ok(root) = layered.clone().try_deserialize::<serde_json::Value>() else {
        return vec![];
    };

    let Some(registrations) = root
        .pointer("/spring/security/oauth2/client/registration")
        .and_then(serde_json::Value::as_object)
    else {
        return vec![];
    };

    let providers = root
        .pointer("/spring/security/oauth2/client/provider")
        .and_then(serde_json::Value::as_object);

    let mut clients = Vec::with_capacity(registrations.len());
    for (registration_id, registration_value) in registrations {
        let Some(registration) = registration_value.as_object() else {
            continue;
        };

        let Some(client_id) =
            read_object_string(registration, &["client-id", "clientId", "client_id"])
        else {
            continue;
        };
        let Some(client_secret) = read_object_string(
            registration,
            &["client-secret", "clientSecret", "client_secret"],
        ) else {
            continue;
        };
        let client_name =
            read_object_string(registration, &["client-name", "clientName", "client_name"])
                .unwrap_or_else(|| registration_id.to_string());
        let scopes = read_object_string_list(registration, &["scope"]);

        let provider_id = read_object_string(registration, &["provider"])
            .unwrap_or_else(|| registration_id.to_string());
        let Some(provider) = providers
            .and_then(|all| all.get(&provider_id))
            .or_else(|| providers.and_then(|all| all.get(registration_id)))
            .and_then(serde_json::Value::as_object)
        else {
            continue;
        };

        let Some(authorization_uri) = read_object_string(
            provider,
            &["authorization-uri", "authorizationUri", "authorization_uri"],
        ) else {
            continue;
        };

        let Some(token_uri) = read_object_string(provider, &["token-uri", "tokenUri", "token_uri"])
        else {
            continue;
        };
        let user_info_uri = read_object_string(
            provider,
            &[
                "user-info-uri",
                "userInfoUri",
                "user_info_uri",
                "userinfo-uri",
                "userinfoUri",
                "userinfo_uri",
            ],
        );

        clients.push(OAuth2ClientConfig {
            registration_id: registration_id.to_string(),
            client_name,
            client_id,
            client_secret,
            authorization_uri,
            token_uri,
            user_info_uri,
            scopes,
        });
    }

    clients
}

fn resolve_oauth2_clients_from_env(env: &BTreeMap<String, String>) -> Vec<OAuth2ClientConfig> {
    let registration_ids = env
        .keys()
        .filter_map(|key| {
            key.strip_prefix("SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_")
                .and_then(|value| value.strip_suffix("_CLIENT_ID"))
                .map(|value| value.to_ascii_lowercase())
        })
        .collect::<BTreeSet<_>>();

    let mut clients = Vec::with_capacity(registration_ids.len());
    for registration_id in registration_ids {
        let registration_key = registration_id.to_ascii_uppercase();
        let Some(client_id) = env
            .get(&format!(
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_{registration_key}_CLIENT_ID"
            ))
            .cloned()
        else {
            continue;
        };
        let Some(client_secret) = env
            .get(&format!(
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_{registration_key}_CLIENT_SECRET"
            ))
            .cloned()
        else {
            continue;
        };

        let client_name = env
            .get(&format!(
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_{registration_key}_CLIENT_NAME"
            ))
            .cloned()
            .unwrap_or_else(|| registration_id.clone());
        let scopes = env
            .get(&format!(
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_{registration_key}_SCOPE"
            ))
            .map(|value| parse_scope_value(value))
            .unwrap_or_default();

        let provider_id = env
            .get(&format!(
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_{registration_key}_PROVIDER"
            ))
            .map(|value| value.to_ascii_lowercase())
            .unwrap_or_else(|| registration_id.clone());
        let provider_key = provider_id.to_ascii_uppercase();

        let Some(authorization_uri) = env
            .get(&format!(
                "SPRING_SECURITY_OAUTH2_CLIENT_PROVIDER_{provider_key}_AUTHORIZATION_URI"
            ))
            .cloned()
        else {
            continue;
        };

        let Some(token_uri) = env
            .get(&format!(
                "SPRING_SECURITY_OAUTH2_CLIENT_PROVIDER_{provider_key}_TOKEN_URI"
            ))
            .cloned()
        else {
            continue;
        };
        let user_info_uri = env
            .get(&format!(
                "SPRING_SECURITY_OAUTH2_CLIENT_PROVIDER_{provider_key}_USER_INFO_URI"
            ))
            .cloned();

        clients.push(OAuth2ClientConfig {
            registration_id,
            client_name,
            client_id,
            client_secret,
            authorization_uri,
            token_uri,
            user_info_uri,
            scopes,
        });
    }

    clients
}

fn read_object_string(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    keys.iter().find_map(|key| {
        object
            .get(*key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    })
}

fn read_object_string_list(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Vec<String> {
    keys.iter()
        .find_map(|key| object.get(*key))
        .map(parse_scope_json_value)
        .unwrap_or_default()
}

fn parse_scope_json_value(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::String(value) => parse_scope_value(value),
        serde_json::Value::Array(values) => values
            .iter()
            .filter_map(serde_json::Value::as_str)
            .flat_map(parse_scope_value)
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_scope_value(value: &str) -> Vec<String> {
    value
        .split([',', ' '])
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
        .map(str::to_string)
        .collect()
}
