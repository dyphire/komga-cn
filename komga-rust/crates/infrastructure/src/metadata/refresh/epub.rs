use std::collections::HashMap;

use komga_application::media_assets::BookMetadataAuthor;
use quick_xml::Reader as XmlReader;
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart as XmlBytesStart, Event as XmlEvent};

use super::support::{
    canonicalize_string_set, is_valid_calendar_date, nonblank_string, normalize_isbn13,
    normalize_optional_bcp47_language,
};
use super::{BookMetadataImportPatch, SeriesMetadataImportPatch};

enum EpubTextTarget {
    Title,
    Description,
    Date,
    Identifier,
    Creator {
        id: Option<String>,
        role_attr: Option<String>,
    },
    RoleMeta {
        refines: Option<String>,
    },
    GroupPosition {
        refines: Option<String>,
    },
}

pub(super) fn extract_epub_book_patch(package_document: &[u8]) -> BookMetadataImportPatch {
    let mut reader = XmlReader::from_reader(package_document);
    reader.config_mut().trim_text(true);

    let mut buffer = Vec::new();
    let mut current_target = None::<EpubTextTarget>;
    let mut current_text = String::new();

    let mut title = None::<String>;
    let mut description = None::<String>;
    let mut release_date = None::<String>;
    let mut identifiers = Vec::<String>::new();
    let mut authors = Vec::<BookMetadataAuthor>::new();
    let mut refined_roles = HashMap::<String, String>::new();
    let mut collection_id = None::<String>;
    let mut group_positions = HashMap::<String, String>::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) => {
                if xml_name_matches_local(event.name().as_ref(), b"meta") {
                    handle_epub_meta_event(
                        &event,
                        &mut current_target,
                        &mut current_text,
                        &mut refined_roles,
                        &mut collection_id,
                        &mut group_positions,
                    );
                } else if let Some(target) = epub_text_target_from_start(&event) {
                    current_target = Some(target);
                    current_text.clear();
                }
            }
            Ok(XmlEvent::Empty(event))
                if xml_name_matches_local(event.name().as_ref(), b"meta") =>
            {
                handle_epub_meta_event(
                    &event,
                    &mut current_target,
                    &mut current_text,
                    &mut refined_roles,
                    &mut collection_id,
                    &mut group_positions,
                );
            }
            Ok(XmlEvent::Text(text)) if current_target.is_some() => {
                current_text.push_str(String::from_utf8_lossy(text.as_ref()).as_ref());
            }
            Ok(XmlEvent::CData(text)) if current_target.is_some() => {
                current_text.push_str(String::from_utf8_lossy(text.as_ref()).as_ref());
            }
            Ok(XmlEvent::End(event))
                if epub_text_target_matches_end(current_target.as_ref(), event.name().as_ref()) =>
            {
                let target = current_target
                    .take()
                    .expect("epub text target should exist");
                finalize_epub_text_target(
                    target,
                    current_text.trim().to_string(),
                    &mut title,
                    &mut description,
                    &mut release_date,
                    &mut identifiers,
                    &mut authors,
                    &mut refined_roles,
                    &mut group_positions,
                );
                current_text.clear();
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buffer.clear();
    }

    let number = collection_id
        .as_deref()
        .and_then(|id| group_positions.get(id))
        .cloned()
        .filter(|value| !value.trim().is_empty());
    let number_sort = number
        .as_deref()
        .and_then(|value| value.parse::<f64>().ok());

    BookMetadataImportPatch {
        title,
        summary: description
            .map(|value| strip_markup_tags(&value))
            .filter(|value| !value.is_empty()),
        number,
        number_sort,
        release_date,
        authors: (!authors.is_empty()).then_some(authors),
        tags: None,
        isbn: identifiers
            .into_iter()
            .find_map(|value| normalize_epub_identifier_isbn(&value)),
        links: None,
    }
}

fn epub_text_target_from_start(event: &XmlBytesStart<'_>) -> Option<EpubTextTarget> {
    let name = event.name();
    let name = name.as_ref();
    if xml_name_matches_local(name, b"title") {
        Some(EpubTextTarget::Title)
    } else if xml_name_matches_local(name, b"description") {
        Some(EpubTextTarget::Description)
    } else if xml_name_matches_local(name, b"date") {
        Some(EpubTextTarget::Date)
    } else if xml_name_matches_local(name, b"identifier") {
        Some(EpubTextTarget::Identifier)
    } else if xml_name_matches_local(name, b"creator") {
        Some(EpubTextTarget::Creator {
            id: attribute_value(event, b"id"),
            role_attr: attribute_value(event, b"role"),
        })
    } else {
        None
    }
}

fn epub_text_target_matches_end(target: Option<&EpubTextTarget>, name: &[u8]) -> bool {
    match target {
        Some(EpubTextTarget::Title) => xml_name_matches_local(name, b"title"),
        Some(EpubTextTarget::Description) => xml_name_matches_local(name, b"description"),
        Some(EpubTextTarget::Date) => xml_name_matches_local(name, b"date"),
        Some(EpubTextTarget::Identifier) => xml_name_matches_local(name, b"identifier"),
        Some(EpubTextTarget::Creator { .. }) => xml_name_matches_local(name, b"creator"),
        Some(EpubTextTarget::RoleMeta { .. }) | Some(EpubTextTarget::GroupPosition { .. }) => {
            xml_name_matches_local(name, b"meta")
        }
        None => false,
    }
}

fn handle_epub_meta_event(
    event: &XmlBytesStart<'_>,
    current_target: &mut Option<EpubTextTarget>,
    current_text: &mut String,
    refined_roles: &mut HashMap<String, String>,
    collection_id: &mut Option<String>,
    group_positions: &mut HashMap<String, String>,
) {
    let property = attribute_value(event, b"property");
    let content = attribute_value(event, b"content");

    if property
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("role"))
    {
        let scheme = attribute_value(event, b"scheme");
        if !scheme
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("marc:relators"))
        {
            return;
        }
        let refines = attribute_value(event, b"refines").map(normalize_epub_refines);
        if let Some(value) = content.and_then(nonblank_string) {
            if let Some(refines) = refines {
                refined_roles.entry(refines).or_insert(value);
            }
        } else {
            *current_target = Some(EpubTextTarget::RoleMeta { refines });
            current_text.clear();
        }
    } else if property
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("belongs-to-collection"))
    {
        if collection_id.is_none() {
            *collection_id = attribute_value(event, b"id").and_then(nonblank_string);
        }
    } else if property
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("group-position"))
    {
        let refines = attribute_value(event, b"refines").map(normalize_epub_refines);
        if let Some(value) = content.and_then(nonblank_string) {
            if let Some(refines) = refines {
                group_positions.entry(refines).or_insert(value);
            }
        } else {
            *current_target = Some(EpubTextTarget::GroupPosition { refines });
            current_text.clear();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn finalize_epub_text_target(
    target: EpubTextTarget,
    value: String,
    title: &mut Option<String>,
    description: &mut Option<String>,
    release_date: &mut Option<String>,
    identifiers: &mut Vec<String>,
    authors: &mut Vec<BookMetadataAuthor>,
    refined_roles: &mut HashMap<String, String>,
    group_positions: &mut HashMap<String, String>,
) {
    match target {
        EpubTextTarget::Title => {
            if title.is_none() {
                *title = nonblank_string(value);
            }
        }
        EpubTextTarget::Description => {
            if description.is_none() {
                *description = nonblank_string(value);
            }
        }
        EpubTextTarget::Date => {
            if release_date.is_none() {
                *release_date = normalize_epub_date(&value);
            }
        }
        EpubTextTarget::Identifier => {
            if let Some(value) = nonblank_string(value) {
                identifiers.push(value);
            }
        }
        EpubTextTarget::Creator { id, role_attr } => {
            if let Some(name) = nonblank_string(value) {
                let refined_role = id
                    .as_deref()
                    .and_then(|id| refined_roles.get(id))
                    .map(String::as_str);
                authors.push(BookMetadataAuthor {
                    name,
                    role: map_epub_author_role(role_attr.as_deref().or(refined_role)).to_string(),
                });
            }
        }
        EpubTextTarget::RoleMeta { refines } => {
            if let (Some(refines), Some(value)) = (refines, nonblank_string(value)) {
                refined_roles.entry(refines).or_insert(value);
            }
        }
        EpubTextTarget::GroupPosition { refines } => {
            if let (Some(refines), Some(value)) = (refines, nonblank_string(value)) {
                group_positions.entry(refines).or_insert(value);
            }
        }
    }
}

fn normalize_epub_identifier_isbn(value: &str) -> Option<String> {
    let lowered = value.trim().to_ascii_lowercase();
    let candidate = lowered.strip_prefix("isbn:").unwrap_or(lowered.as_str());

    normalize_isbn13(candidate)
}

fn normalize_epub_date(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let candidate = if trimmed.len() >= 10 {
        &trimmed[..10]
    } else {
        trimmed
    };
    if candidate.len() != 10 {
        return None;
    }

    let bytes = candidate.as_bytes();
    if bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') {
        return None;
    }

    let year = candidate[0..4].parse::<i32>().ok()?;
    let month = candidate[5..7].parse::<u8>().ok()?;
    let day = candidate[8..10].parse::<u8>().ok()?;
    if !is_valid_calendar_date(year, month, day) {
        return None;
    }

    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn map_epub_author_role(value: Option<&str>) -> &'static str {
    let value = value.unwrap_or("writer").trim().to_ascii_lowercase();
    match value.as_str() {
        "aut" => "writer",
        "clr" => "colorist",
        "cov" => "cover",
        "edt" => "editor",
        "art" | "ill" => "penciller",
        "trl" => "translator",
        _ => "writer",
    }
}

fn strip_markup_tags(value: &str) -> String {
    let mut output = String::new();
    let mut inside_tag = false;

    for character in value.chars() {
        match character {
            '<' => {
                inside_tag = true;
                if !output.ends_with(' ') {
                    output.push(' ');
                }
            }
            '>' => {
                inside_tag = false;
                if !output.ends_with(' ') {
                    output.push(' ');
                }
            }
            _ if !inside_tag => output.push(character),
            _ => {}
        }
    }

    output
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

#[derive(Clone, Copy)]
enum EpubSeriesTextTarget {
    Collection,
    Publisher,
    Language,
    Subject,
}

pub(super) fn extract_epub_series_patch(package_document: &[u8]) -> SeriesMetadataImportPatch {
    let mut reader = XmlReader::from_reader(package_document);
    reader.config_mut().trim_text(true);

    let mut buffer = Vec::new();
    let mut current_target = None::<EpubSeriesTextTarget>;
    let mut current_text = String::new();

    let mut collection = None::<String>;
    let mut publisher = None::<String>;
    let mut language = None::<String>;
    let mut genres = Vec::<String>::new();
    let mut reading_direction = None::<String>;

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) => {
                if xml_name_matches_local(event.name().as_ref(), b"spine") {
                    reading_direction = page_progression_direction(&event).or(reading_direction);
                } else if xml_name_matches_local(event.name().as_ref(), b"meta") {
                    if let Some(target) = epub_series_text_target_from_meta(&event) {
                        if let Some(value) =
                            attribute_value(&event, b"content").and_then(nonblank_string)
                        {
                            apply_epub_series_text_target(
                                target,
                                value,
                                &mut collection,
                                &mut publisher,
                                &mut language,
                                &mut genres,
                            );
                        } else {
                            current_target = Some(target);
                            current_text.clear();
                        }
                    }
                } else if let Some(target) = epub_series_text_target_from_start(&event) {
                    current_target = Some(target);
                    current_text.clear();
                }
            }
            Ok(XmlEvent::Empty(event)) => {
                if xml_name_matches_local(event.name().as_ref(), b"spine") {
                    reading_direction = page_progression_direction(&event).or(reading_direction);
                } else if xml_name_matches_local(event.name().as_ref(), b"meta")
                    && let Some(target) = epub_series_text_target_from_meta(&event)
                    && let Some(value) =
                        attribute_value(&event, b"content").and_then(nonblank_string)
                {
                    apply_epub_series_text_target(
                        target,
                        value,
                        &mut collection,
                        &mut publisher,
                        &mut language,
                        &mut genres,
                    );
                }
            }
            Ok(XmlEvent::Text(text)) if current_target.is_some() => {
                current_text.push_str(String::from_utf8_lossy(text.as_ref()).as_ref());
            }
            Ok(XmlEvent::CData(text)) if current_target.is_some() => {
                current_text.push_str(String::from_utf8_lossy(text.as_ref()).as_ref());
            }
            Ok(XmlEvent::End(event))
                if epub_series_text_target_matches_end(
                    current_target.as_ref(),
                    event.name().as_ref(),
                ) =>
            {
                let target = current_target
                    .take()
                    .expect("epub series target should exist");
                apply_epub_series_text_target(
                    target,
                    current_text.trim().to_string(),
                    &mut collection,
                    &mut publisher,
                    &mut language,
                    &mut genres,
                );
                current_text.clear();
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }

        buffer.clear();
    }

    let genres = canonicalize_string_set(genres);

    SeriesMetadataImportPatch {
        title: collection.clone(),
        title_sort: collection,
        status: None,
        summary: None,
        reading_direction,
        publisher,
        age_rating: None,
        language: normalize_optional_bcp47_language(language),
        genres: (!genres.is_empty()).then_some(genres),
        total_book_count: None,
        collections: Vec::new(),
    }
}

fn epub_series_text_target_from_meta(event: &XmlBytesStart<'_>) -> Option<EpubSeriesTextTarget> {
    match attribute_value(event, b"property")?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "belongs-to-collection" => Some(EpubSeriesTextTarget::Collection),
        _ => None,
    }
}

fn epub_series_text_target_from_start(event: &XmlBytesStart<'_>) -> Option<EpubSeriesTextTarget> {
    let name = event.name();
    let name = name.as_ref();
    if xml_name_matches_local(name, b"publisher") {
        Some(EpubSeriesTextTarget::Publisher)
    } else if xml_name_matches_local(name, b"language") {
        Some(EpubSeriesTextTarget::Language)
    } else if xml_name_matches_local(name, b"subject") {
        Some(EpubSeriesTextTarget::Subject)
    } else {
        None
    }
}

fn epub_series_text_target_matches_end(target: Option<&EpubSeriesTextTarget>, name: &[u8]) -> bool {
    match target {
        Some(EpubSeriesTextTarget::Collection) => xml_name_matches_local(name, b"meta"),
        Some(EpubSeriesTextTarget::Publisher) => xml_name_matches_local(name, b"publisher"),
        Some(EpubSeriesTextTarget::Language) => xml_name_matches_local(name, b"language"),
        Some(EpubSeriesTextTarget::Subject) => xml_name_matches_local(name, b"subject"),
        None => false,
    }
}

fn apply_epub_series_text_target(
    target: EpubSeriesTextTarget,
    value: String,
    collection: &mut Option<String>,
    publisher: &mut Option<String>,
    language: &mut Option<String>,
    genres: &mut Vec<String>,
) {
    let Some(value) = nonblank_string(value) else {
        return;
    };

    match target {
        EpubSeriesTextTarget::Collection => {
            if collection.is_none() {
                *collection = Some(value);
            }
        }
        EpubSeriesTextTarget::Publisher => {
            if publisher.is_none() {
                *publisher = Some(value);
            }
        }
        EpubSeriesTextTarget::Language => {
            if language.is_none() {
                *language = Some(value);
            }
        }
        EpubSeriesTextTarget::Subject => genres.push(value),
    }
}

fn page_progression_direction(event: &XmlBytesStart<'_>) -> Option<String> {
    match attribute_value(event, b"page-progression-direction")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "rtl" => Some("RIGHT_TO_LEFT".to_string()),
        "ltr" => Some("LEFT_TO_RIGHT".to_string()),
        _ => None,
    }
}

fn attribute_value(event: &XmlBytesStart<'_>, key: &[u8]) -> Option<String> {
    event.attributes().flatten().find_map(|attribute| {
        xml_name_matches_local(attribute.key.as_ref(), key)
            .then(|| {
                attribute
                    .normalized_value(XmlVersion::Implicit1_0)
                    .ok()
                    .map(|value| value.into_owned())
            })
            .flatten()
    })
}

fn normalize_epub_refines(value: String) -> String {
    value.trim().trim_start_matches('#').to_string()
}

fn xml_name_matches_local(actual: &[u8], expected: &[u8]) -> bool {
    actual == expected || actual.ends_with(expected)
}
