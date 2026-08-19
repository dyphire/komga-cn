use serde::Serialize;
use serde::ser::Error as SerdeError;
use time::Date;
use time::OffsetDateTime;
use time::PrimitiveDateTime;
use time::UtcOffset;
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;

const SQLITE_DATETIME: &[time::format_description::FormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
const SQLITE_DATETIME_WITH_SUBSECOND: &[time::format_description::FormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second].[subsecond]");
const ISO_DATETIME: &[time::format_description::FormatItem<'static>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]");
const ISO_DATETIME_WITH_SUBSECOND: &[time::format_description::FormatItem<'static>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond]");
const KOTLIN_LOCAL_DATE: &[time::format_description::FormatItem<'static>] =
    format_description!("[year]-[month]-[day]");

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct KotlinLocalDate(Date);

impl KotlinLocalDate {
    pub fn parse(raw: &str) -> Result<Self, WireLocalDateError> {
        Date::parse(raw, KOTLIN_LOCAL_DATE)
            .map(Self)
            .map_err(|_| WireLocalDateError {
                input: raw.to_string(),
            })
    }
}

impl Serialize for KotlinLocalDate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let value = self.0.format(KOTLIN_LOCAL_DATE).map_err(S::Error::custom)?;
        serializer.serialize_str(&value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WireLocalDateError {
    input: String,
}

impl std::fmt::Display for WireLocalDateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "unsupported wire local date: {}", self.input)
    }
}

impl std::error::Error for WireLocalDateError {}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct KotlinUtcDateTime(OffsetDateTime);

impl KotlinUtcDateTime {
    pub fn parse(raw: &str) -> Result<Self, WireDateTimeError> {
        let parsed = OffsetDateTime::parse(raw, &Rfc3339)
            .ok()
            .or_else(|| parse_naive_datetime(raw, SQLITE_DATETIME).ok())
            .or_else(|| parse_naive_datetime(raw, SQLITE_DATETIME_WITH_SUBSECOND).ok())
            .or_else(|| parse_naive_datetime(raw, ISO_DATETIME).ok())
            .or_else(|| parse_naive_datetime(raw, ISO_DATETIME_WITH_SUBSECOND).ok())
            .or_else(|| {
                raw.parse::<i64>()
                    .ok()
                    .and_then(|seconds| OffsetDateTime::from_unix_timestamp(seconds).ok())
            })
            .ok_or_else(|| WireDateTimeError {
                input: raw.to_string(),
            })?;

        Ok(Self(parsed.to_offset(UtcOffset::UTC)))
    }
}

impl Serialize for KotlinUtcDateTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let value = self.0.format(&Rfc3339).map_err(S::Error::custom)?;
        serializer.serialize_str(&value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WireDateTimeError {
    input: String,
}

impl std::fmt::Display for WireDateTimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "unsupported wire datetime: {}", self.input)
    }
}

impl std::error::Error for WireDateTimeError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KotlinLocalDateTime(PrimitiveDateTime);

impl KotlinLocalDateTime {
    pub fn parse(raw: &str) -> Result<Self, WireDateTimeError> {
        let parsed = OffsetDateTime::parse(raw, &Rfc3339)
            .ok()
            .or_else(|| parse_naive_datetime(raw, SQLITE_DATETIME).ok())
            .or_else(|| parse_naive_datetime(raw, SQLITE_DATETIME_WITH_SUBSECOND).ok())
            .or_else(|| parse_naive_datetime(raw, ISO_DATETIME).ok())
            .or_else(|| parse_naive_datetime(raw, ISO_DATETIME_WITH_SUBSECOND).ok())
            .or_else(|| {
                raw.parse::<i64>()
                    .ok()
                    .and_then(|seconds| OffsetDateTime::from_unix_timestamp(seconds).ok())
            })
            .ok_or_else(|| WireDateTimeError {
                input: raw.to_string(),
            })?;

        Ok(Self(PrimitiveDateTime::new(parsed.date(), parsed.time())))
    }

    pub fn from_unix_timestamp_nanos(unix_nanos: i128) -> Result<Self, WireDateTimeError> {
        let local_offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
        let datetime = OffsetDateTime::from_unix_timestamp_nanos(unix_nanos)
            .map_err(|_| WireDateTimeError {
                input: unix_nanos.to_string(),
            })?
            .to_offset(local_offset);
        Ok(Self(PrimitiveDateTime::new(
            datetime.date(),
            datetime.time(),
        )))
    }
}

impl Serialize for KotlinLocalDateTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let datetime = self.0;
        let mut value = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            datetime.year(),
            datetime.month() as u8,
            datetime.day(),
            datetime.hour(),
            datetime.minute(),
            datetime.second(),
        );
        if datetime.nanosecond() > 0 {
            let fraction = format!("{:09}", datetime.nanosecond());
            value.push('.');
            value.push_str(fraction.trim_end_matches('0'));
        }
        serializer.serialize_str(&value)
    }
}

fn parse_naive_datetime(
    raw: &str,
    format: &'static [time::format_description::FormatItem<'static>],
) -> Result<OffsetDateTime, time::error::Parse> {
    PrimitiveDateTime::parse(raw, format).map(|value| value.assume_utc())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageDto<T> {
    pub content: Vec<T>,
    pub pageable: PageableDto,
    pub last: bool,
    pub total_elements: usize,
    pub total_pages: usize,
    pub first: bool,
    pub size: usize,
    pub number: usize,
    pub sort: SortDto,
    pub number_of_elements: usize,
    pub empty: bool,
}

impl<T> PageDto<T> {
    pub fn paged(
        content: Vec<T>,
        page: usize,
        size: usize,
        total_elements: usize,
        total_pages: usize,
        sorted: bool,
    ) -> Self {
        Self::from_parts(
            content,
            page,
            size,
            total_elements,
            total_pages,
            true,
            sorted,
        )
    }

    pub fn from_parts(
        content: Vec<T>,
        page: usize,
        size: usize,
        total_elements: usize,
        total_pages: usize,
        paged: bool,
        sorted: bool,
    ) -> Self {
        let number_of_elements = content.len();
        let sort = SortDto::new(sorted);

        Self {
            content,
            pageable: PageableDto {
                page_number: page,
                page_size: size,
                sort,
                offset: if paged { page.saturating_mul(size) } else { 0 },
                paged,
                unpaged: !paged,
            },
            last: total_pages == 0 || page + 1 >= total_pages,
            total_elements,
            total_pages,
            first: page == 0,
            size,
            number: page,
            sort,
            number_of_elements,
            empty: number_of_elements == 0,
        }
    }

    pub fn unpaged(content: Vec<T>, sorted: bool) -> Self {
        let total_elements = content.len();
        let size = total_elements.max(1);
        let total_pages = usize::from(total_elements != 0);
        Self::from_parts(content, 0, size, total_elements, total_pages, false, sorted)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageableDto {
    pub page_number: usize,
    pub page_size: usize,
    pub sort: SortDto,
    pub offset: usize,
    pub paged: bool,
    pub unpaged: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SortDto {
    pub empty: bool,
    pub sorted: bool,
    pub unsorted: bool,
}

impl SortDto {
    pub fn new(sorted: bool) -> Self {
        Self {
            empty: !sorted,
            sorted,
            unsorted: !sorted,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpringErrorDto {
    pub error: String,
    pub message: String,
    pub path: String,
    pub status: u16,
    pub timestamp: u64,
}

#[derive(Debug, Serialize)]
pub struct SpringInternalErrorDto {
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct MessageDto {
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ValidationErrorDto {
    pub violations: Vec<ViolationDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViolationDto {
    pub field_name: Option<String>,
    pub message: Option<String>,
}
