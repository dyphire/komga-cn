use serde::Deserialize;
use std::io::ErrorKind;
use std::path::Path;
use url::Url;

use komga_application::discovery::SeriesReadingDirection;
use komga_application::media_assets::{BookMetadataAuthor, BookMetadataLink};

use super::readlist::ComicInfoReadListEntry;
use super::support::{
    canonicalize_string_set, compute_series_from_series_and_volume, dedupe_strings_preserve_order,
    extract_xml_tag, is_valid_calendar_date, normalize_comicinfo_age_rating, normalize_isbn13,
    normalize_optional_bcp47_language, split_comicinfo_list,
};
use super::{BookMetadataImportPatch, SeriesMetadataImportPatch};

pub(super) fn extract_comicinfo_book_patch(xml: &str) -> BookMetadataImportPatch {
    let number = extract_xml_tag(xml, "Number");

    BookMetadataImportPatch {
        title: extract_xml_tag(xml, "Title"),
        summary: extract_xml_tag(xml, "Summary"),
        number_sort: number
            .as_deref()
            .and_then(|value| value.parse::<f64>().ok()),
        number,
        release_date: extract_comicinfo_release_date(xml),
        authors: extract_comicinfo_authors(xml),
        tags: extract_comicinfo_tags(xml),
        isbn: extract_comicinfo_isbn(xml),
        links: extract_comicinfo_links(xml),
    }
}

fn extract_comicinfo_release_date(xml: &str) -> Option<String> {
    let year = extract_xml_tag(xml, "Year")?.parse::<i32>().ok()?;
    let month = extract_xml_tag(xml, "Month")
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(1);
    let day = extract_xml_tag(xml, "Day")
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(1);
    if !is_valid_calendar_date(year, month, day) {
        return None;
    }

    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn extract_comicinfo_authors(xml: &str) -> Option<Vec<BookMetadataAuthor>> {
    let mut authors = Vec::new();

    for (tag, role) in [
        ("Writer", "writer"),
        ("Penciller", "penciller"),
        ("Inker", "inker"),
        ("Colorist", "colorist"),
        ("Letterer", "letterer"),
        ("CoverArtist", "cover"),
        ("Editor", "editor"),
        ("Translator", "translator"),
    ] {
        if let Some(value) = extract_xml_tag(xml, tag) {
            authors.extend(
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|name| BookMetadataAuthor {
                        name: name.to_string(),
                        role: role.to_string(),
                    }),
            );
        }
    }

    (!authors.is_empty()).then_some(authors)
}

fn extract_comicinfo_tags(xml: &str) -> Option<Vec<String>> {
    let mut tags = extract_xml_tag(xml, "Tags")?
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>();
    tags.sort();
    tags.dedup();

    (!tags.is_empty()).then_some(tags)
}

fn extract_comicinfo_isbn(xml: &str) -> Option<String> {
    normalize_isbn13(&extract_xml_tag(xml, "GTIN")?)
}

fn extract_comicinfo_links(xml: &str) -> Option<Vec<BookMetadataLink>> {
    let links = extract_xml_tag(xml, "Web")?
        .split(' ')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter_map(|value| {
            let url = Url::parse(value).ok()?;
            if !matches!(url.scheme(), "http" | "https") {
                return None;
            }
            Some(BookMetadataLink {
                label: url.host_str()?.to_string(),
                url: value.to_string(),
            })
        })
        .collect::<Vec<_>>();

    (!links.is_empty()).then_some(links)
}

pub(super) fn extract_comicinfo_readlists(xml: &str) -> Vec<ComicInfoReadListEntry> {
    let mut readlists = Vec::new();

    if let Some(alternate_series) = extract_xml_tag(xml, "AlternateSeries") {
        readlists.push(ComicInfoReadListEntry {
            number: extract_xml_tag(xml, "AlternateNumber").and_then(|value| value.parse().ok()),
            name: alternate_series,
        });
    }

    if let Some(story_arc) = extract_xml_tag(xml, "StoryArc") {
        let arcs = story_arc
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        let numbers = extract_xml_tag(xml, "StoryArcNumber")
            .map(|numbers| {
                numbers
                    .split(',')
                    .map(|value| value.trim().parse::<i64>().ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if numbers.is_empty() {
            readlists.extend(
                arcs.into_iter()
                    .map(|name| ComicInfoReadListEntry { name, number: None }),
            );
        } else {
            for (name, number) in arcs.into_iter().zip(numbers) {
                if let Some(number) = number {
                    readlists.push(ComicInfoReadListEntry {
                        name,
                        number: Some(number),
                    });
                }
            }
        }
    }

    readlists
}

#[derive(Deserialize)]
struct MylarSeriesFile {
    metadata: MylarSeriesMetadata,
}

#[derive(Deserialize)]
struct MylarSeriesMetadata {
    publisher: String,
    name: String,
    year: i64,
    #[serde(rename = "description_text")]
    description_text: Option<String>,
    #[serde(rename = "description_formatted")]
    description_formatted: Option<String>,
    volume: Option<i64>,
    #[serde(rename = "age_rating")]
    age_rating: Option<MylarAgeRating>,
    #[serde(rename = "total_issues")]
    total_issues: i64,
    status: MylarStatus,
}

#[derive(Deserialize)]
enum MylarStatus {
    Ended,
    Continuing,
}

#[derive(Deserialize)]
enum MylarAgeRating {
    #[serde(rename = "All")]
    All,
    #[serde(rename = "9+")]
    Nine,
    #[serde(rename = "12+")]
    Twelve,
    #[serde(rename = "15+")]
    Fifteen,
    #[serde(rename = "17+")]
    Seventeen,
    #[serde(rename = "Adult")]
    Adult,
}

fn mylar_age_rating_value(value: MylarAgeRating) -> u32 {
    match value {
        MylarAgeRating::All => 0,
        MylarAgeRating::Nine => 9,
        MylarAgeRating::Twelve => 12,
        MylarAgeRating::Fifteen => 15,
        MylarAgeRating::Seventeen => 17,
        MylarAgeRating::Adult => 18,
    }
}

pub(super) fn load_mylar_series_patch(
    series_dir: &Path,
) -> Result<Option<SeriesMetadataImportPatch>, String> {
    let series_json_path = series_dir.join("series.json");
    let json = match std::fs::read_to_string(&series_json_path) {
        Ok(json) => json,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "failed to read Mylar series.json '{}': {error}",
                series_json_path.display()
            ));
        }
    };
    let metadata = serde_json::from_str::<MylarSeriesFile>(&json)
        .map_err(|error| {
            format!(
                "failed to parse Mylar series.json '{}': {error}",
                series_json_path.display()
            )
        })?
        .metadata;
    let title = if metadata.volume.is_none() || metadata.volume == Some(1) {
        metadata.name
    } else {
        format!("{} ({})", metadata.name, metadata.year)
    };

    Ok(Some(SeriesMetadataImportPatch {
        title: Some(title.clone()),
        title_sort: Some(title),
        status: Some(match metadata.status {
            MylarStatus::Ended => "ENDED".to_string(),
            MylarStatus::Continuing => "ONGOING".to_string(),
        }),
        summary: match metadata.description_formatted {
            Some(summary) => Some(summary),
            None => metadata.description_text,
        },
        reading_direction: None,
        publisher: Some(metadata.publisher),
        age_rating: metadata.age_rating.map(mylar_age_rating_value),
        language: None,
        genres: None,
        total_book_count: u32::try_from(metadata.total_issues).ok(),
        collections: Vec::new(),
    }))
}

pub(super) fn extract_comicinfo_series_patch(
    xml: &str,
    append_volume_to_title: bool,
) -> SeriesMetadataImportPatch {
    let series = if append_volume_to_title {
        compute_series_from_series_and_volume(
            extract_xml_tag(xml, "Series"),
            extract_xml_tag(xml, "Volume").and_then(|value| value.parse::<i64>().ok()),
        )
    } else {
        extract_xml_tag(xml, "Series")
    };
    let genres = canonicalize_string_set(split_comicinfo_list(extract_xml_tag(xml, "Genre")));
    let collections =
        dedupe_strings_preserve_order(split_comicinfo_list(extract_xml_tag(xml, "SeriesGroup")));

    SeriesMetadataImportPatch {
        title: series.clone(),
        title_sort: series,
        status: None,
        summary: None,
        reading_direction: match extract_xml_tag(xml, "Manga")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "no" => Some(SeriesReadingDirection::LeftToRight),
            "yesandrighttoleft" => Some(SeriesReadingDirection::RightToLeft),
            _ => None,
        },
        publisher: extract_xml_tag(xml, "Publisher"),
        age_rating: extract_xml_tag(xml, "AgeRating")
            .as_deref()
            .and_then(normalize_comicinfo_age_rating),
        language: normalize_optional_bcp47_language(extract_xml_tag(xml, "LanguageISO")),
        genres: (!genres.is_empty()).then_some(genres),
        total_book_count: extract_xml_tag(xml, "Count")
            .and_then(|value| value.parse::<i64>().ok())
            .and_then(|value| u32::try_from(value).ok()),
        collections,
    }
}
