use anyhow::{Context, Result};
use komga_application::identity_access::{
    AuthUser, AuthUserAgeRestriction, PersistedApiKey, PersistedAuthenticationActivity,
    user_response_role_names,
};
use serde::Serialize;

use super::common::KotlinUtcDateTime;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDto {
    pub id: String,
    pub email: String,
    pub roles: Vec<&'static str>,
    pub shared_all_libraries: bool,
    pub shared_libraries_ids: Vec<String>,
    pub labels_allow: Vec<String>,
    pub labels_exclude: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age_restriction: Option<AgeRestrictionDto>,
}

impl UserDto {
    pub fn from_user(user: &AuthUser) -> Self {
        Self {
            id: user.id.clone(),
            email: user.email.clone(),
            roles: user_response_role_names(user),
            shared_all_libraries: user.shared_all_libraries,
            shared_libraries_ids: user.shared_library_ids.clone(),
            labels_allow: user.labels_allow.clone(),
            labels_exclude: user.labels_exclude.clone(),
            age_restriction: user
                .age_restriction
                .as_ref()
                .map(AgeRestrictionDto::from_model),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgeRestrictionDto {
    pub age: i64,
    pub restriction: String,
}

impl AgeRestrictionDto {
    fn from_model(value: &AuthUserAgeRestriction) -> Self {
        Self {
            age: value.age,
            restriction: value.restriction.persisted_name().to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimStatusDto {
    pub is_claimed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyDto {
    pub id: String,
    pub user_id: String,
    pub key: String,
    pub comment: String,
    pub created_date: Option<KotlinUtcDateTime>,
    pub last_modified_date: Option<KotlinUtcDateTime>,
}

impl ApiKeyDto {
    pub fn from_persisted(api_key: &PersistedApiKey, redacted: bool) -> Result<Self> {
        Ok(Self {
            id: api_key.id.clone(),
            user_id: api_key.user_id.clone(),
            key: if redacted {
                "******".to_string()
            } else {
                api_key.key.clone()
            },
            comment: api_key.comment.clone(),
            created_date: parse_optional_datetime(
                "apiKey.createdDate",
                api_key.created_date.as_deref(),
            )?,
            last_modified_date: parse_optional_datetime(
                "apiKey.lastModifiedDate",
                api_key.last_modified_date.as_deref(),
            )?,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticationActivityDto {
    pub user_id: Option<String>,
    pub email: Option<String>,
    pub api_key_id: Option<String>,
    pub api_key_comment: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub success: bool,
    pub error: Option<String>,
    pub date_time: KotlinUtcDateTime,
    pub source: Option<String>,
}

impl AuthenticationActivityDto {
    pub fn from_persisted(activity: &PersistedAuthenticationActivity) -> Result<Self> {
        Ok(Self {
            user_id: activity.user_id.clone(),
            email: activity.email.clone(),
            api_key_id: activity.api_key_id.clone(),
            api_key_comment: activity.api_key_comment.clone(),
            ip: activity.ip.clone(),
            user_agent: activity.user_agent.clone(),
            success: activity.success,
            error: activity.error.clone(),
            date_time: KotlinUtcDateTime::parse(&activity.date_time).with_context(|| {
                format!("authenticationActivity.dateTime: {}", activity.date_time)
            })?,
            source: activity.source.clone(),
        })
    }
}

fn parse_optional_datetime(field: &str, value: Option<&str>) -> Result<Option<KotlinUtcDateTime>> {
    value
        .map(|value| KotlinUtcDateTime::parse(value).with_context(|| format!("{field}: {value}")))
        .transpose()
}
