use komga_domain::discovery::{DiscoveryError, MediaProfile, MediaStatus, ReadStatus};
use serde_json::Value;

pub(super) fn parse_u16_value(
    condition: &Value,
    condition_type: &str,
) -> Result<u16, DiscoveryError> {
    condition
        .get("value")
        .and_then(|value| {
            value
                .as_u64()
                .and_then(|number| u16::try_from(number).ok())
                .or_else(|| {
                    value
                        .as_str()
                        .and_then(|raw| raw.trim().parse::<u16>().ok())
                })
        })
        .ok_or_else(|| {
            DiscoveryError::InvalidSemantics(format!(
                "{condition_type} filter requires a numeric value",
            ))
        })
}

pub(super) fn parse_read_status_value(
    value: &str,
    condition_type: &str,
) -> Result<ReadStatus, DiscoveryError> {
    ReadStatus::parse(value).ok_or_else(|| {
        DiscoveryError::InvalidSemantics(format!("{condition_type} filter requires a valid value"))
    })
}

pub(super) fn parse_read_status_values(
    values: Vec<String>,
    condition_type: &str,
) -> Result<Vec<ReadStatus>, DiscoveryError> {
    values
        .into_iter()
        .map(|value| parse_read_status_value(&value, condition_type))
        .collect()
}

pub(super) fn parse_media_profile_value(
    value: &str,
    condition_type: &str,
) -> Result<MediaProfile, DiscoveryError> {
    MediaProfile::parse(value).ok_or_else(|| {
        DiscoveryError::InvalidSemantics(format!("{condition_type} filter requires a valid value"))
    })
}

pub(super) fn parse_media_status_value(
    value: &str,
    condition_type: &str,
) -> Result<MediaStatus, DiscoveryError> {
    MediaStatus::parse(value).ok_or_else(|| {
        DiscoveryError::InvalidSemantics(format!("{condition_type} filter requires a valid value"))
    })
}

pub(super) fn parse_media_status_values(
    values: Vec<String>,
    condition_type: &str,
) -> Result<Vec<MediaStatus>, DiscoveryError> {
    values
        .into_iter()
        .map(|value| parse_media_status_value(&value, condition_type))
        .collect()
}

pub(super) fn parse_media_status_prefix(
    value: &str,
    condition_type: &str,
) -> Result<Vec<MediaStatus>, DiscoveryError> {
    let statuses = MediaStatus::matching_persisted_name_prefix(value);
    if statuses.is_empty() {
        return Err(DiscoveryError::InvalidSemantics(format!(
            "{condition_type} filter requires a valid value",
        )));
    }
    Ok(statuses)
}
