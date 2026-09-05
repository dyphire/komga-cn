use anyhow::Context;
use std::collections::HashMap;

use komga_application::discovery::SeriesReadingDirection;
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

#[derive(Default)]
struct EpubMetadataAccumulator {
    title: Option<String>,
    description: Option<String>,
    release_date: Option<String>,
    identifiers: Vec<String>,
    authors: Vec<BookMetadataAuthor>,
    refined_roles: HashMap<String, String>,
    group_positions: HashMap<String, String>,
}

pub(super) fn extract_epub_book_patch(
    package_document: &[u8],
) -> anyhow::Result<BookMetadataImportPatch> {
    let mut reader = XmlReader::from_reader(package_document);
    reader.config_mut().trim_text(true);

    let mut buffer = Vec::new();
    let mut current_target = None::<EpubTextTarget>;
    let mut current_text = String::new();

    let mut acc = EpubMetadataAccumulator::default();
    let mut collection_id = None::<String>;

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) => {
                if xml_name_matches_local(event.name().as_ref(), "meta") {
                    handle_epub_meta_event(
                        &event,
                        &mut current_target,
                        &mut current_text,
                        &mut acc.refined_roles,
                        &mut collection_id,
                        &mut acc.group_positions,
                    )?;
                } else if let Some(target) = epub_text_target_from_start(&event)? {
                    current_target = Some(target);
                    current_text.clear();
                }
            }
            Ok(XmlEvent::Empty(event)) if xml_name_matches_local(event.name().as_ref(), "meta") => {
                handle_epub_meta_event(
                    &event,
                    &mut current_target,
                    &mut current_text,
                    &mut acc.refined_roles,
                    &mut collection_id,
                    &mut acc.group_positions,
                )?;
            }
            Ok(XmlEvent::Text(text)) if current_target.is_some() => {
                current_text.push_str(text.as_ref());
            }
            Ok(XmlEvent::CData(text)) if current_target.is_some() => {
                current_text.push_str(text.as_ref());
            }
            Ok(XmlEvent::End(event))
                if epub_text_target_matches_end(current_target.as_ref(), event.name().as_ref()) =>
            {
                let target = current_target
                    .take()
                    .expect("epub text target should exist");
                finalize_epub_text_target(target, current_text.trim().to_string(), &mut acc);
                current_text.clear();
            }
            Ok(XmlEvent::Eof) => break,
            Err(error) => {
                return Err(anyhow::anyhow!(format!(
                    "failed to parse EPUB package document for book metadata: {error}"
                )));
            }
            _ => {}
        }
        buffer.clear();
    }

    let number = collection_id
        .as_deref()
        .and_then(|id| acc.group_positions.get(id))
        .cloned()
        .filter(|value| !value.trim().is_empty());
    let number_sort = number
        .as_deref()
        .and_then(|value| value.parse::<f64>().ok());

    Ok(BookMetadataImportPatch {
        title: acc.title,
        summary: acc
            .description
            .map(|value| strip_markup_tags(&value))
            .filter(|value| !value.is_empty()),
        number,
        number_sort,
        release_date: acc.release_date,
        authors: (!acc.authors.is_empty()).then_some(acc.authors),
        tags: None,
        isbn: acc
            .identifiers
            .into_iter()
            .find_map(|value| normalize_epub_identifier_isbn(&value)),
        links: None,
    })
}

fn epub_text_target_from_start(
    event: &XmlBytesStart<'_>,
) -> anyhow::Result<Option<EpubTextTarget>> {
    let name = event.name();
    let name = name.as_ref();
    if xml_name_matches_local(name, "title") {
        Ok(Some(EpubTextTarget::Title))
    } else if xml_name_matches_local(name, "description") {
        Ok(Some(EpubTextTarget::Description))
    } else if xml_name_matches_local(name, "date") {
        Ok(Some(EpubTextTarget::Date))
    } else if xml_name_matches_local(name, "identifier") {
        Ok(Some(EpubTextTarget::Identifier))
    } else if xml_name_matches_local(name, "creator") {
        Ok(Some(EpubTextTarget::Creator {
            id: attribute_value(event, "id")?,
            role_attr: attribute_value(event, "role")?,
        }))
    } else {
        Ok(None)
    }
}

fn epub_text_target_matches_end(target: Option<&EpubTextTarget>, name: &str) -> bool {
    match target {
        Some(EpubTextTarget::Title) => xml_name_matches_local(name, "title"),
        Some(EpubTextTarget::Description) => xml_name_matches_local(name, "description"),
        Some(EpubTextTarget::Date) => xml_name_matches_local(name, "date"),
        Some(EpubTextTarget::Identifier) => xml_name_matches_local(name, "identifier"),
        Some(EpubTextTarget::Creator { .. }) => xml_name_matches_local(name, "creator"),
        Some(EpubTextTarget::RoleMeta { .. }) | Some(EpubTextTarget::GroupPosition { .. }) => {
            xml_name_matches_local(name, "meta")
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
) -> anyhow::Result<()> {
    let property = attribute_value(event, "property")?;
    let content = attribute_value(event, "content")?;

    if property
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("role"))
    {
        let scheme = attribute_value(event, "scheme")?;
        if !scheme
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("marc:relators"))
        {
            return Ok(());
        }
        let refines = attribute_value(event, "refines")?.map(normalize_epub_refines);
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
            *collection_id = attribute_value(event, "id")?.and_then(nonblank_string);
        }
    } else if property
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("group-position"))
    {
        let refines = attribute_value(event, "refines")?.map(normalize_epub_refines);
        if let Some(value) = content.and_then(nonblank_string) {
            if let Some(refines) = refines {
                group_positions.entry(refines).or_insert(value);
            }
        } else {
            *current_target = Some(EpubTextTarget::GroupPosition { refines });
            current_text.clear();
        }
    }

    Ok(())
}

fn finalize_epub_text_target(
    target: EpubTextTarget,
    value: String,
    acc: &mut EpubMetadataAccumulator,
) {
    match target {
        EpubTextTarget::Title => {
            if acc.title.is_none() {
                acc.title = nonblank_string(value);
            }
        }
        EpubTextTarget::Description => {
            if acc.description.is_none() {
                acc.description = nonblank_string(value);
            }
        }
        EpubTextTarget::Date => {
            if acc.release_date.is_none() {
                acc.release_date = normalize_epub_date(&value);
            }
        }
        EpubTextTarget::Identifier => {
            if let Some(value) = nonblank_string(value) {
                acc.identifiers.push(value);
            }
        }
        EpubTextTarget::Creator { id, role_attr } => {
            if let Some(name) = nonblank_string(value) {
                let refined_role = id
                    .as_deref()
                    .and_then(|id| acc.refined_roles.get(id))
                    .map(String::as_str);
                acc.authors.push(BookMetadataAuthor {
                    name,
                    role: map_epub_author_role(role_attr.as_deref().or(refined_role)).to_string(),
                });
            }
        }
        EpubTextTarget::RoleMeta { refines } => {
            if let (Some(refines), Some(value)) = (refines, nonblank_string(value)) {
                acc.refined_roles.entry(refines).or_insert(value);
            }
        }
        EpubTextTarget::GroupPosition { refines } => {
            if let (Some(refines), Some(value)) = (refines, nonblank_string(value)) {
                acc.group_positions.entry(refines).or_insert(value);
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

pub(super) fn extract_epub_series_patch(
    package_document: &[u8],
) -> anyhow::Result<SeriesMetadataImportPatch> {
    let mut reader = XmlReader::from_reader(package_document);
    reader.config_mut().trim_text(true);

    let mut buffer = Vec::new();
    let mut current_target = None::<EpubSeriesTextTarget>;
    let mut current_text = String::new();

    let mut collection = None::<String>;
    let mut publisher = None::<String>;
    let mut language = None::<String>;
    let mut genres = Vec::<String>::new();
    let mut reading_direction = None::<SeriesReadingDirection>;

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(XmlEvent::Start(event)) => {
                if xml_name_matches_local(event.name().as_ref(), "spine") {
                    reading_direction = page_progression_direction(&event)?.or(reading_direction);
                } else if xml_name_matches_local(event.name().as_ref(), "meta") {
                    if let Some(target) = epub_series_text_target_from_meta(&event)? {
                        if let Some(value) =
                            attribute_value(&event, "content")?.and_then(nonblank_string)
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
                if xml_name_matches_local(event.name().as_ref(), "spine") {
                    reading_direction = page_progression_direction(&event)?.or(reading_direction);
                } else if xml_name_matches_local(event.name().as_ref(), "meta")
                    && let Some(target) = epub_series_text_target_from_meta(&event)?
                    && let Some(value) =
                        attribute_value(&event, "content")?.and_then(nonblank_string)
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
                current_text.push_str(text.as_ref());
            }
            Ok(XmlEvent::CData(text)) if current_target.is_some() => {
                current_text.push_str(text.as_ref());
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
            Err(error) => {
                return Err(anyhow::anyhow!(format!(
                    "failed to parse EPUB package document for series metadata: {error}"
                )));
            }
            _ => {}
        }

        buffer.clear();
    }

    let genres = canonicalize_string_set(genres);

    Ok(SeriesMetadataImportPatch {
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
    })
}

fn epub_series_text_target_from_meta(
    event: &XmlBytesStart<'_>,
) -> anyhow::Result<Option<EpubSeriesTextTarget>> {
    match attribute_value(event, "property")?
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "belongs-to-collection" => Ok(Some(EpubSeriesTextTarget::Collection)),
        _ => Ok(None),
    }
}

fn epub_series_text_target_from_start(event: &XmlBytesStart<'_>) -> Option<EpubSeriesTextTarget> {
    let name = event.name();
    let name = name.as_ref();
    if xml_name_matches_local(name, "publisher") {
        Some(EpubSeriesTextTarget::Publisher)
    } else if xml_name_matches_local(name, "language") {
        Some(EpubSeriesTextTarget::Language)
    } else if xml_name_matches_local(name, "subject") {
        Some(EpubSeriesTextTarget::Subject)
    } else {
        None
    }
}

fn epub_series_text_target_matches_end(target: Option<&EpubSeriesTextTarget>, name: &str) -> bool {
    match target {
        Some(EpubSeriesTextTarget::Collection) => xml_name_matches_local(name, "meta"),
        Some(EpubSeriesTextTarget::Publisher) => xml_name_matches_local(name, "publisher"),
        Some(EpubSeriesTextTarget::Language) => xml_name_matches_local(name, "language"),
        Some(EpubSeriesTextTarget::Subject) => xml_name_matches_local(name, "subject"),
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

fn page_progression_direction(
    event: &XmlBytesStart<'_>,
) -> anyhow::Result<Option<SeriesReadingDirection>> {
    match attribute_value(event, "page-progression-direction")
        .map(|value| value.unwrap_or_default())?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "rtl" => Ok(Some(SeriesReadingDirection::RightToLeft)),
        "ltr" => Ok(Some(SeriesReadingDirection::LeftToRight)),
        _ => Ok(None),
    }
}

fn attribute_value(event: &XmlBytesStart<'_>, key: &str) -> anyhow::Result<Option<String>> {
    for attribute in event.attributes() {
        let attribute = attribute.context("failed to parse EPUB package document attribute")?;
        if xml_name_matches_local(attribute.key.as_ref(), key) {
            return attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .map(|value| Some(value.into_owned()))
                .map_err(|error| {
                    anyhow::anyhow!(error)
                        .context("failed to parse EPUB package document attribute value: ")
                });
        }
    }

    Ok(None)
}

fn normalize_epub_refines(value: String) -> String {
    value.trim().trim_start_matches('#').to_string()
}

fn xml_name_matches_local(actual: &str, expected: &str) -> bool {
    actual == expected || actual.ends_with(expected)
}

#[cfg(test)]
mod tests {
    use super::{extract_epub_book_patch, extract_epub_series_patch};

    #[test]
    fn extract_epub_book_patch_rejects_malformed_package_document() {
        let error = match extract_epub_book_patch(
            br#"<package><metadata><dc:title><</dc:title></metadata></package>"#,
        ) {
            Ok(_) => panic!("malformed EPUB package document should fail book metadata parsing"),
            Err(error) => error,
        };

        assert!(
            error.to_string().contains("EPUB package document"),
            "{error}"
        );
    }

    #[test]
    fn extract_epub_book_patch_rejects_malformed_package_attributes() {
        let error = match extract_epub_book_patch(
            br#"<package><metadata><dc:creator id= role="aut">Jane</dc:creator></metadata></package>"#,
        ) {
            Ok(_) => panic!("malformed EPUB attribute should fail book metadata parsing"),
            Err(error) => error,
        };

        assert!(
            error.to_string().contains("EPUB package document"),
            "{error}"
        );
    }

    #[test]
    fn extract_epub_series_patch_rejects_malformed_package_document() {
        let error = match extract_epub_series_patch(
            br#"<package><metadata><dc:publisher><</dc:publisher></metadata></package>"#,
        ) {
            Ok(_) => panic!("malformed EPUB package document should fail series metadata parsing"),
            Err(error) => error,
        };

        assert!(
            error.to_string().contains("EPUB package document"),
            "{error}"
        );
    }

    #[test]
    fn extract_epub_series_patch_rejects_malformed_package_attributes() {
        let error = match extract_epub_series_patch(
            br#"<package><spine page-progression-direction= /></package>"#,
        ) {
            Ok(_) => panic!("malformed EPUB attribute should fail series metadata parsing"),
            Err(error) => error,
        };

        assert!(
            error.to_string().contains("EPUB package document"),
            "{error}"
        );
    }
}
