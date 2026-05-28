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
        let redirect_uri = read_object_string(
            registration,
            &["redirect-uri", "redirectUri", "redirect_uri"],
        );
        let client_authentication_method = read_object_string(
            registration,
            &[
                "client-authentication-method",
                "clientAuthenticationMethod",
                "client_authentication_method",
            ],
        );
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

        let authorization_uri = read_object_string(
            provider,
            &["authorization-uri", "authorizationUri", "authorization_uri"],
        );

        let token_uri = read_object_string(provider, &["token-uri", "tokenUri", "token_uri"]);
        let issuer_uri = read_object_string(provider, &["issuer-uri", "issuerUri", "issuer_uri"]);
        let jwk_set_uri = read_object_string(
            provider,
            &[
                "jwk-set-uri",
                "jwkSetUri",
                "jwk_set_uri",
                "jwks-uri",
                "jwksUri",
                "jwks_uri",
            ],
        );
        if !oauth2_client_has_enough_endpoints(
            &scopes,
            authorization_uri.as_ref(),
            token_uri.as_ref(),
            issuer_uri.as_ref(),
        ) {
            continue;
        }
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
            issuer_uri,
            jwk_set_uri,
            redirect_uri,
            client_authentication_method,
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
        let redirect_uri = env
            .get(&format!(
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_{registration_key}_REDIRECT_URI"
            ))
            .cloned();
        let client_authentication_method = env
            .get(&format!(
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_{registration_key}_CLIENT_AUTHENTICATION_METHOD"
            ))
            .cloned();
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

        let authorization_uri = env
            .get(&format!(
                "SPRING_SECURITY_OAUTH2_CLIENT_PROVIDER_{provider_key}_AUTHORIZATION_URI"
            ))
            .cloned();

        let token_uri = env
            .get(&format!(
                "SPRING_SECURITY_OAUTH2_CLIENT_PROVIDER_{provider_key}_TOKEN_URI"
            ))
            .cloned();
        let issuer_uri = env
            .get(&format!(
                "SPRING_SECURITY_OAUTH2_CLIENT_PROVIDER_{provider_key}_ISSUER_URI"
            ))
            .cloned();
        let jwk_set_uri = env
            .get(&format!(
                "SPRING_SECURITY_OAUTH2_CLIENT_PROVIDER_{provider_key}_JWK_SET_URI"
            ))
            .cloned();
        if !oauth2_client_has_enough_endpoints(
            &scopes,
            authorization_uri.as_ref(),
            token_uri.as_ref(),
            issuer_uri.as_ref(),
        ) {
            continue;
        }
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
            issuer_uri,
            jwk_set_uri,
            redirect_uri,
            client_authentication_method,
            scopes,
        });
    }

    clients
}

fn oauth2_client_has_enough_endpoints(
    scopes: &[String],
    authorization_uri: Option<&String>,
    token_uri: Option<&String>,
    issuer_uri: Option<&String>,
) -> bool {
    if issuer_uri.is_some() && oauth2_client_uses_oidc(scopes) {
        return true;
    }

    authorization_uri.is_some() && token_uri.is_some()
}

fn oauth2_client_uses_oidc(scopes: &[String]) -> bool {
    scopes
        .iter()
        .any(|scope| scope.eq_ignore_ascii_case("openid"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_accepts_oidc_issuer_without_explicit_oauth2_endpoints() {
        let clients = resolve_oauth2_clients_from_env(&BTreeMap::from([
            (
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_OIDC_CLIENT_ID".to_string(),
                "oidc-client-id".to_string(),
            ),
            (
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_OIDC_CLIENT_SECRET".to_string(),
                "oidc-client-secret".to_string(),
            ),
            (
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_OIDC_SCOPE".to_string(),
                "openid email".to_string(),
            ),
            (
                "SPRING_SECURITY_OAUTH2_CLIENT_PROVIDER_OIDC_ISSUER_URI".to_string(),
                "https://issuer.example".to_string(),
            ),
        ]));

        assert_eq!(clients.len(), 1);
        let client = &clients[0];
        assert_eq!(client.registration_id, "oidc");
        assert_eq!(client.issuer_uri.as_deref(), Some("https://issuer.example"));
        assert_eq!(client.authorization_uri, None);
        assert_eq!(client.token_uri, None);
    }

    #[test]
    fn env_preserves_login_critical_spring_oauth2_fields() {
        let clients = resolve_oauth2_clients_from_env(&BTreeMap::from([
            (
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_OIDC_CLIENT_ID".to_string(),
                "oidc-client-id".to_string(),
            ),
            (
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_OIDC_CLIENT_SECRET".to_string(),
                "oidc-client-secret".to_string(),
            ),
            (
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_OIDC_SCOPE".to_string(),
                "openid,email".to_string(),
            ),
            (
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_OIDC_REDIRECT_URI".to_string(),
                "{baseUrl}/oauth/callback/{registrationId}".to_string(),
            ),
            (
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_OIDC_CLIENT_AUTHENTICATION_METHOD"
                    .to_string(),
                "client_secret_post".to_string(),
            ),
            (
                "SPRING_SECURITY_OAUTH2_CLIENT_PROVIDER_OIDC_AUTHORIZATION_URI".to_string(),
                "https://issuer.example/oauth/authorize".to_string(),
            ),
            (
                "SPRING_SECURITY_OAUTH2_CLIENT_PROVIDER_OIDC_TOKEN_URI".to_string(),
                "https://issuer.example/oauth/token".to_string(),
            ),
            (
                "SPRING_SECURITY_OAUTH2_CLIENT_PROVIDER_OIDC_ISSUER_URI".to_string(),
                "https://issuer.example".to_string(),
            ),
            (
                "SPRING_SECURITY_OAUTH2_CLIENT_PROVIDER_OIDC_JWK_SET_URI".to_string(),
                "https://issuer.example/oauth/jwks".to_string(),
            ),
        ]));

        assert_eq!(clients.len(), 1);
        let client = &clients[0];
        assert_eq!(
            client.redirect_uri.as_deref(),
            Some("{baseUrl}/oauth/callback/{registrationId}")
        );
        assert_eq!(
            client.client_authentication_method.as_deref(),
            Some("client_secret_post")
        );
        assert_eq!(client.issuer_uri.as_deref(), Some("https://issuer.example"));
        assert_eq!(
            client.jwk_set_uri.as_deref(),
            Some("https://issuer.example/oauth/jwks")
        );
        assert_eq!(client.scopes, vec!["openid", "email"]);
    }

    #[test]
    fn env_keeps_oauth2_clients_on_authorization_and_token_uri_contract() {
        let clients = resolve_oauth2_clients_from_env(&BTreeMap::from([
            (
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_GENERIC_CLIENT_ID".to_string(),
                "generic-client-id".to_string(),
            ),
            (
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_GENERIC_CLIENT_SECRET".to_string(),
                "generic-client-secret".to_string(),
            ),
            (
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_GENERIC_SCOPE".to_string(),
                "profile email".to_string(),
            ),
            (
                "SPRING_SECURITY_OAUTH2_CLIENT_PROVIDER_GENERIC_ISSUER_URI".to_string(),
                "https://issuer.example".to_string(),
            ),
        ]));

        assert!(clients.is_empty());
    }

    #[test]
    fn layered_config_preserves_spring_oauth2_provider_and_registration_fields() {
        let layered = LayeredConfig::builder()
            .set_override(
                "spring.security.oauth2.client.registration.oidc.client-id",
                "oidc-client-id",
            )
            .expect("layered client id override should apply")
            .set_override(
                "spring.security.oauth2.client.registration.oidc.client-secret",
                "oidc-client-secret",
            )
            .expect("layered client secret override should apply")
            .set_override(
                "spring.security.oauth2.client.registration.oidc.scope",
                "openid,email",
            )
            .expect("layered scope override should apply")
            .set_override(
                "spring.security.oauth2.client.registration.oidc.redirect-uri",
                "{baseUrl}/login/oauth2/code/{registrationId}",
            )
            .expect("layered redirect uri override should apply")
            .set_override(
                "spring.security.oauth2.client.registration.oidc.client-authentication-method",
                "client_secret_post",
            )
            .expect("layered client auth method override should apply")
            .set_override(
                "spring.security.oauth2.client.provider.oidc.authorization-uri",
                "https://issuer.example/oauth/authorize",
            )
            .expect("layered authorization uri override should apply")
            .set_override(
                "spring.security.oauth2.client.provider.oidc.token-uri",
                "https://issuer.example/oauth/token",
            )
            .expect("layered token uri override should apply")
            .set_override(
                "spring.security.oauth2.client.provider.oidc.issuer-uri",
                "https://issuer.example",
            )
            .expect("layered issuer uri override should apply")
            .set_override(
                "spring.security.oauth2.client.provider.oidc.jwk-set-uri",
                "https://issuer.example/oauth/jwks",
            )
            .expect("layered jwk set uri override should apply")
            .build()
            .expect("layered config should build");

        let clients = resolve_oauth2_clients_from_layered(&layered);

        assert_eq!(clients.len(), 1);
        let client = &clients[0];
        assert_eq!(client.registration_id, "oidc");
        assert_eq!(
            client.redirect_uri.as_deref(),
            Some("{baseUrl}/login/oauth2/code/{registrationId}")
        );
        assert_eq!(
            client.client_authentication_method.as_deref(),
            Some("client_secret_post")
        );
        assert_eq!(client.issuer_uri.as_deref(), Some("https://issuer.example"));
        assert_eq!(
            client.jwk_set_uri.as_deref(),
            Some("https://issuer.example/oauth/jwks")
        );
    }
}
