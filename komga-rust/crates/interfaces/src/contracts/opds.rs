use serde::Serialize;
use time::format_description::well_known::Rfc3339;
use time::{OffsetDateTime, UtcOffset};

#[derive(Clone, Debug, Serialize)]
#[serde(transparent)]
pub struct OpdsV2UpdatedDto(String);

impl OpdsV2UpdatedDto {
    pub fn now() -> Self {
        let now_utc = OffsetDateTime::now_utc();
        let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
        let value = now_utc
            .to_offset(offset)
            .format(&Rfc3339)
            .unwrap_or_else(|_| "2000-01-01T00:00:00Z".to_string());
        Self(value)
    }

    pub fn from_storage(value: &str) -> Self {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Self::now();
        }
        if OffsetDateTime::parse(trimmed, &Rfc3339).is_ok() {
            return Self(trimmed.to_string());
        }
        if let Some((date, time)) = trimmed.split_once(' ') {
            return Self(format!("{date}T{time}Z"));
        }
        if trimmed.contains('T') {
            return Self(format!("{trimmed}Z"));
        }
        Self(trimmed.to_string())
    }
}

#[derive(Debug, Serialize)]
pub struct OpdsAuthenticationDto {
    pub authentication: Vec<OpdsAuthenticationMethodDto>,
    pub title: String,
    pub id: String,
    pub description: String,
    pub links: Vec<OpdsAuthenticationLinkDto>,
}

#[derive(Debug, Serialize)]
pub struct OpdsAuthenticationMethodDto {
    #[serde(rename = "type")]
    pub authentication_type: String,
    pub labels: OpdsAuthenticationLabelsDto,
}

#[derive(Debug, Serialize)]
pub struct OpdsAuthenticationLabelsDto {
    pub login: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct OpdsAuthenticationLinkDto {
    pub rel: String,
    pub href: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpdsV2LinkDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rel: Option<String>,
    pub href: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub templated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<OpdsV2LinkPropertiesDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpdsV2LinkPropertiesDto {
    pub authenticate: OpdsV2AuthenticationLinkDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpdsV2AuthenticationLinkDto {
    pub href: String,
    #[serde(rename = "type")]
    pub media_type: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpdsV2FeedMetadataDto {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub modified: OpdsV2UpdatedDto,
    pub items_per_page: usize,
    pub current_page: usize,
    pub number_of_items: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpdsV2RecommendedMetadataDto {
    pub title: String,
    pub modified: OpdsV2UpdatedDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpdsV2GroupMetadataDto {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_per_page: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_page: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number_of_items: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct OpdsV2NavigationGroupDto {
    pub metadata: OpdsV2GroupMetadataDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<OpdsV2LinkDto>>,
    pub navigation: Vec<OpdsV2LinkDto>,
}

#[derive(Debug, Serialize)]
pub struct OpdsV2PublicationGroupDto {
    pub metadata: OpdsV2GroupMetadataDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<OpdsV2LinkDto>>,
    pub publications: Vec<OpdsV2PublicationDto>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum OpdsV2GroupDto {
    Navigation(OpdsV2NavigationGroupDto),
    Publications(OpdsV2PublicationGroupDto),
}

#[derive(Debug, Serialize)]
pub struct OpdsV2RecommendedFeedDto {
    pub metadata: OpdsV2RecommendedMetadataDto,
    pub links: Vec<OpdsV2LinkDto>,
    pub navigation: Vec<OpdsV2LinkDto>,
    pub groups: Vec<OpdsV2GroupDto>,
}

#[derive(Debug, Serialize)]
pub struct OpdsV2GroupedFeedDto {
    pub metadata: OpdsV2FeedMetadataDto,
    pub links: Vec<OpdsV2LinkDto>,
    pub navigation: Vec<OpdsV2LinkDto>,
    pub groups: Vec<OpdsV2GroupDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpdsV2NavigationFeedDto {
    pub metadata: OpdsV2FeedMetadataDto,
    pub links: Vec<OpdsV2LinkDto>,
    pub navigation: Vec<OpdsV2LinkDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpdsV2PublicationFeedDto {
    pub metadata: OpdsV2FeedMetadataDto,
    pub links: Vec<OpdsV2LinkDto>,
    pub publications: Vec<OpdsV2PublicationDto>,
}

#[derive(Debug, Serialize)]
pub struct OpdsV2PublicationFacetFeedDto {
    pub metadata: OpdsV2FeedMetadataDto,
    pub links: Vec<OpdsV2LinkDto>,
    pub publications: Vec<OpdsV2PublicationDto>,
    pub facets: Option<Vec<OpdsV2FacetDto>>,
}

#[derive(Debug, Serialize)]
pub struct OpdsV2FacetDto {
    pub metadata: OpdsV2GroupMetadataDto,
    pub links: Vec<OpdsV2LinkDto>,
}

#[derive(Debug, Serialize)]
pub struct OpdsV2SearchFeedDto {
    pub metadata: OpdsV2RecommendedMetadataDto,
    pub links: Vec<OpdsV2LinkDto>,
    pub groups: Vec<OpdsV2GroupDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpdsV2PublicationDto {
    #[serde(rename = "@context")]
    pub context: String,
    pub metadata: OpdsV2PublicationMetadataDto,
    pub links: Vec<OpdsV2LinkDto>,
    pub images: Vec<OpdsV2LinkDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpdsV2PublicationMetadataDto {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number_of_pages: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<OpdsV2UpdatedDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translator: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub illustrator: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub letterer: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub penciler: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colorist: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inker: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contributor: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub belongs_to: Option<OpdsV2BelongsToDto>,
}

#[derive(Debug, Serialize)]
pub struct OpdsV2BelongsToDto {
    pub series: Vec<OpdsV2PublicationSeriesDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpdsV2PublicationSeriesDto {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<OpdsV2LinkDto>>,
}
