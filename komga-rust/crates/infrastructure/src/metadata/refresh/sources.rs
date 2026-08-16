use serde::Deserialize;
use std::path::Path;
use url::Url;

use komga_application::discovery::SeriesReadingDirection;
use komga_application::media_assets::{BookMetadataAuthor, BookMetadataLink};

use crate::metadata::ComicInfoDocument;

use super::readlist::ComicInfoReadListEntry;
use super::support::{
    canonicalize_string_set, compute_series_from_series_and_volume, dedupe_strings_preserve_order,
    is_valid_calendar_date, normalize_comicinfo_age_rating, normalize_isbn13,
    normalize_optional_bcp47_language, split_comicinfo_list,
};
use super::{BookMetadataImportPatch, SeriesMetadataImportPatch};

pub(super) fn extract_comicinfo_book_patch(
    document: &ComicInfoDocument,
) -> BookMetadataImportPatch {
    let number = document.number.clone();

    BookMetadataImportPatch {
        title: document.title.clone(),
        summary: document.summary.clone(),
        number_sort: number
            .as_deref()
            .and_then(|value| value.parse::<f64>().ok()),
        number,
        release_date: extract_comicinfo_release_date(document),
        authors: extract_comicinfo_authors(document),
        tags: extract_comicinfo_tags(document),
        isbn: extract_comicinfo_isbn(document),
        links: extract_comicinfo_links(document),
    }
}

fn extract_comicinfo_release_date(document: &ComicInfoDocument) -> Option<String> {
    let year = document.year.as_deref()?.parse::<i32>().ok()?;
    let month = document
        .month
        .as_deref()
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(1);
    let day = document
        .day
        .as_deref()
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(1);
    if !is_valid_calendar_date(year, month, day) {
        return None;
    }

    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn extract_comicinfo_authors(document: &ComicInfoDocument) -> Option<Vec<BookMetadataAuthor>> {
    let mut authors = Vec::new();

    for (value, role) in [
        (document.writer.as_deref(), "writer"),
        (document.penciller.as_deref(), "penciller"),
        (document.inker.as_deref(), "inker"),
        (document.colorist.as_deref(), "colorist"),
        (document.letterer.as_deref(), "letterer"),
        (document.cover_artist.as_deref(), "cover"),
        (document.editor.as_deref(), "editor"),
        (document.translator.as_deref(), "translator"),
    ] {
        if let Some(value) = value {
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

fn extract_comicinfo_tags(document: &ComicInfoDocument) -> Option<Vec<String>> {
    let mut tags = document
        .tags
        .as_deref()?
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>();
    tags.sort();
    tags.dedup();

    (!tags.is_empty()).then_some(tags)
}

fn extract_comicinfo_isbn(document: &ComicInfoDocument) -> Option<String> {
    normalize_isbn13(document.gtin.as_deref()?)
}

fn extract_comicinfo_links(document: &ComicInfoDocument) -> Option<Vec<BookMetadataLink>> {
    let links = document
        .web
        .as_deref()?
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

pub(super) fn extract_comicinfo_readlists(
    document: &ComicInfoDocument,
) -> Vec<ComicInfoReadListEntry> {
    let mut readlists = Vec::new();

    if let Some(alternate_series) = document.alternate_series.clone() {
        readlists.push(ComicInfoReadListEntry {
            number: document
                .alternate_number
                .as_deref()
                .and_then(|value| value.parse().ok()),
            name: alternate_series,
        });
    }

    if let Some(story_arc) = document.story_arc.as_deref() {
        let arcs = story_arc
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        let numbers = document
            .story_arc_number
            .clone()
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
    #[serde(rename = "type")]
    _type: String,
    publisher: String,
    #[serde(rename = "imprint")]
    _imprint: Option<String>,
    name: String,
    #[serde(rename = "comicid", alias = "cid")]
    _comicid: serde_json::Value,
    year: i64,
    #[serde(rename = "description_text")]
    description_text: Option<String>,
    #[serde(rename = "description_formatted")]
    description_formatted: Option<String>,
    volume: Option<i64>,
    #[serde(rename = "booktype")]
    _book_type: String,
    #[serde(rename = "age_rating")]
    age_rating: Option<MylarAgeRating>,
    #[serde(rename = "comic_image", alias = "ComicImage")]
    _comic_image: String,
    #[serde(rename = "total_issues")]
    total_issues: i64,
    #[serde(rename = "publication_run")]
    _publication_run: String,
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
) -> anyhow::Result<Option<SeriesMetadataImportPatch>> {
    let series_json_path = series_dir.join("series.json");
    let Ok(json) = std::fs::read_to_string(&series_json_path) else {
        return Ok(None);
    };
    let Ok(metadata) = serde_json::from_str::<MylarSeriesFile>(&json).map(|file| file.metadata)
    else {
        return Ok(None);
    };
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
    document: &ComicInfoDocument,
    append_volume_to_title: bool,
) -> SeriesMetadataImportPatch {
    let series = if append_volume_to_title {
        compute_series_from_series_and_volume(
            document.series.clone(),
            document
                .volume
                .as_deref()
                .and_then(|value| value.parse::<i64>().ok()),
        )
    } else {
        document.series.clone()
    };
    let genres = canonicalize_string_set(split_comicinfo_list(document.genre.clone()));
    let collections =
        dedupe_strings_preserve_order(split_comicinfo_list(document.series_group.clone()));

    SeriesMetadataImportPatch {
        title: series.clone(),
        title_sort: series,
        status: None,
        summary: None,
        reading_direction: match document
            .manga
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "no" => Some(SeriesReadingDirection::LeftToRight),
            "yesandrighttoleft" => Some(SeriesReadingDirection::RightToLeft),
            _ => None,
        },
        publisher: document.publisher.clone(),
        age_rating: document
            .age_rating
            .as_deref()
            .and_then(normalize_comicinfo_age_rating),
        language: normalize_optional_bcp47_language(document.language_iso.clone()),
        genres: (!genres.is_empty()).then_some(genres),
        total_book_count: document
            .count
            .as_deref()
            .and_then(|value| value.parse::<i64>().ok())
            .and_then(|value| u32::try_from(value).ok()),
        collections,
    }
}
