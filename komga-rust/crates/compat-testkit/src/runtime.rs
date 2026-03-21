use crate::cases::SetupStep;
use anyhow::{bail, Context};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, SET_COOKIE};
use std::collections::BTreeMap;

pub fn apply_setup_steps(
    client: &Client,
    base_url: &str,
    steps: &[SetupStep],
    vars: &mut BTreeMap<String, String>,
) -> anyhow::Result<()> {
    for step in steps {
        let mut request = match step.method.as_str() {
            "GET" => client.get(format!(
                "{}{}",
                base_url.trim_end_matches('/'),
                step.path.as_str()
            )),
            other => bail!("unsupported setup method in skeleton: {other}"),
        };

        if let Some(headers) = resolve_headers(&step.headers, vars)? {
            let mut header_map = HeaderMap::new();
            for (name, value) in headers {
                header_map.insert(
                    HeaderName::from_bytes(name.as_bytes())?,
                    HeaderValue::from_str(&value)?,
                );
            }
            request = request.headers(header_map);
        }

        let response = request
            .send()
            .with_context(|| format!("setup step '{}' failed to send", step.name))?;

        if !response.status().is_success() {
            bail!(
                "setup step '{}' returned HTTP {}",
                step.name,
                response.status().as_u16()
            );
        }

        if let Some(extract_headers) = &step.extract_headers {
            for (var_name, header_name) in extract_headers {
                let value = if header_name.eq_ignore_ascii_case("X-Auth-Token") {
                    if let Some(token) = extract_session_cookie_token(response.headers()) {
                        token
                    } else {
                        response
                            .headers()
                            .get(header_name)
                            .with_context(|| {
                                format!(
                                    "missing header '{header_name}' in setup step '{}'",
                                    step.name
                                )
                            })?
                            .to_str()
                            .with_context(|| {
                                format!(
                                    "header '{header_name}' is not valid UTF-8 in setup step '{}'",
                                    step.name
                                )
                            })?
                            .to_string()
                    }
                } else {
                    response
                        .headers()
                        .get(header_name)
                        .with_context(|| {
                            format!(
                                "missing header '{header_name}' in setup step '{}'",
                                step.name
                            )
                        })?
                        .to_str()
                        .with_context(|| {
                            format!(
                                "header '{header_name}' is not valid UTF-8 in setup step '{}'",
                                step.name
                            )
                        })?
                        .to_string()
                };
                vars.insert(var_name.clone(), value);
            }
        }
    }

    Ok(())
}

fn extract_session_cookie_token(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers.get_all(SET_COOKIE).iter().find_map(|value| {
        value.to_str().ok().and_then(|cookie| {
            cookie
                .split(';')
                .map(str::trim)
                .find_map(|part| part.strip_prefix("KOMGA-SESSION="))
                .map(str::to_string)
        })
    })
}

pub fn resolve_headers(
    headers: &Option<BTreeMap<String, String>>,
    vars: &BTreeMap<String, String>,
) -> anyhow::Result<Option<BTreeMap<String, String>>> {
    headers
        .as_ref()
        .map(|headers| {
            headers
                .iter()
                .map(|(name, value)| Ok((name.clone(), resolve_template(value, vars)?)))
                .collect::<anyhow::Result<BTreeMap<String, String>>>()
        })
        .transpose()
}

fn resolve_template(template: &str, vars: &BTreeMap<String, String>) -> anyhow::Result<String> {
    let mut resolved = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(start) = rest.find("${") {
        resolved.push_str(&rest[..start]);
        let rest_after_start = &rest[start + 2..];
        let end = rest_after_start
            .find('}')
            .with_context(|| format!("unterminated template variable in '{template}'"))?;
        let key = &rest_after_start[..end];
        let value = vars
            .get(key)
            .cloned()
            .or_else(|| std::env::var(key).ok())
            .with_context(|| format!("missing template variable '{key}'"))?;
        resolved.push_str(&value);
        rest = &rest_after_start[end + 1..];
    }

    resolved.push_str(rest);
    Ok(resolved)
}
