#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct OpdsV1XmlLink {
    media_type: String,
    rel: String,
    href: String,
    attributes: Vec<OpdsV1XmlAttribute>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OpdsV1XmlAttribute {
    name: String,
    value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct OpdsV1NavigationFeedDocument {
    pub id: String,
    pub title: String,
    pub updated: String,
    pub self_href: String,
    pub start_href: String,
    pub previous_href: Option<String>,
    pub next_href: Option<String>,
    pub extra_links: Vec<OpdsV1XmlLink>,
    pub entries: Vec<OpdsV1NavigationFeedEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct OpdsV1NavigationFeedEntry {
    pub id: String,
    pub title: String,
    pub updated: String,
    pub content: String,
    pub href: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct OpdsV1AcquisitionFeedDocument {
    pub id: String,
    pub title: String,
    pub updated: String,
    pub self_href: String,
    pub start_href: String,
    pub previous_href: Option<String>,
    pub next_href: Option<String>,
    pub entries: Vec<OpdsV1AcquisitionFeedEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct OpdsV1AcquisitionFeedEntry {
    pub id: String,
    pub title: String,
    pub updated: String,
    pub content: String,
    pub authors: Vec<String>,
    pub acquisition_media_type: String,
    pub acquisition_href: String,
    pub thumbnail_href: String,
    pub image_href: String,
    pub extra_links: Vec<OpdsV1XmlLink>,
}

impl OpdsV1XmlLink {
    pub(super) fn new(
        media_type: impl Into<String>,
        rel: impl Into<String>,
        href: impl Into<String>,
    ) -> Self {
        Self {
            media_type: media_type.into(),
            rel: rel.into(),
            href: href.into(),
            attributes: Vec::new(),
        }
    }

    pub(super) fn with_attribute(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.attributes.push(OpdsV1XmlAttribute {
            name: name.into(),
            value: value.into(),
        });
        self
    }
}

pub(super) fn render_opds_v1_navigation_feed(document: OpdsV1NavigationFeedDocument) -> String {
    let mut body = atom_feed_header(
        &document.id,
        &document.title,
        &document.updated,
        &document.self_href,
        "application/atom+xml;profile=opds-catalog;kind=navigation",
        &document.start_href,
    );

    for link in document.extra_links {
        render_link(&mut body, &link);
    }
    render_optional_link(
        &mut body,
        "application/atom+xml;profile=opds-catalog;kind=navigation",
        "previous",
        document.previous_href.as_deref(),
    );
    render_optional_link(
        &mut body,
        "application/atom+xml;profile=opds-catalog;kind=navigation",
        "next",
        document.next_href.as_deref(),
    );

    for entry in document.entries {
        body.push_str("<entry>");
        push_text_element(&mut body, "title", &entry.title);
        push_text_element(&mut body, "updated", &entry.updated);
        push_text_element(&mut body, "id", &entry.id);
        body.push_str("<content>");
        body.push_str(&opds_content_markup(&entry.content));
        body.push_str("</content>");
        render_link(
            &mut body,
            &OpdsV1XmlLink::new(
                "application/atom+xml;profile=opds-catalog;kind=navigation",
                "subsection",
                entry.href,
            ),
        );
        body.push_str("</entry>");
    }

    body.push_str("</feed>");
    body
}

pub(super) fn render_opds_v1_acquisition_feed(document: OpdsV1AcquisitionFeedDocument) -> String {
    let mut body = atom_feed_header(
        &document.id,
        &document.title,
        &document.updated,
        &document.self_href,
        "application/atom+xml;profile=opds-catalog;kind=acquisition",
        &document.start_href,
    );

    render_optional_link(
        &mut body,
        "application/atom+xml;profile=opds-catalog;kind=acquisition",
        "previous",
        document.previous_href.as_deref(),
    );
    render_optional_link(
        &mut body,
        "application/atom+xml;profile=opds-catalog;kind=acquisition",
        "next",
        document.next_href.as_deref(),
    );

    for entry in document.entries {
        body.push_str("<entry>");
        push_text_element(&mut body, "title", &entry.title);
        push_text_element(&mut body, "updated", &entry.updated);
        push_text_element(&mut body, "id", &entry.id);
        body.push_str("<content>");
        body.push_str(&opds_content_markup(&entry.content));
        body.push_str("</content>");
        for author in entry.authors {
            body.push_str("<author>");
            push_text_element(&mut body, "name", &author);
            body.push_str("</author>");
        }
        render_link(
            &mut body,
            &OpdsV1XmlLink::new(
                entry.acquisition_media_type,
                "http://opds-spec.org/acquisition",
                entry.acquisition_href,
            ),
        );
        render_link(
            &mut body,
            &OpdsV1XmlLink::new(
                "image/jpeg",
                "http://opds-spec.org/image/thumbnail",
                entry.thumbnail_href,
            ),
        );
        render_link(
            &mut body,
            &OpdsV1XmlLink::new("image/jpeg", "http://opds-spec.org/image", entry.image_href),
        );
        for link in entry.extra_links {
            render_link(&mut body, &link);
        }
        body.push_str("</entry>");
    }

    body.push_str("</feed>");
    body
}

fn atom_feed_header(
    id: &str,
    title: &str,
    updated: &str,
    self_href: &str,
    self_media_type: &str,
    start_href: &str,
) -> String {
    let mut body = String::new();
    body.push_str(
        "<feed xmlns=\"http://www.w3.org/2005/Atom\" xmlns:pse=\"http://vaemendis.net/opds-pse/ns\">",
    );
    push_text_element(&mut body, "id", id);
    push_text_element(&mut body, "title", title);
    push_text_element(&mut body, "updated", updated);
    body.push_str(
        "<author><name>Komga</name><uri>https://github.com/huihuimoe/komga-riir</uri></author>",
    );
    render_link(
        &mut body,
        &OpdsV1XmlLink::new(self_media_type, "self", self_href),
    );
    render_link(
        &mut body,
        &OpdsV1XmlLink::new(
            "application/atom+xml;profile=opds-catalog;kind=navigation",
            "start",
            start_href,
        ),
    );
    body
}

fn render_optional_link(
    body: &mut String,
    media_type: &'static str,
    rel: &'static str,
    href: Option<&str>,
) {
    if let Some(href) = href {
        render_link(body, &OpdsV1XmlLink::new(media_type, rel, href));
    }
}

fn render_link(body: &mut String, link: &OpdsV1XmlLink) {
    body.push_str("<link type=\"");
    body.push_str(&xml_escape(&link.media_type));
    body.push_str("\" rel=\"");
    body.push_str(&xml_escape(&link.rel));
    body.push_str("\" href=\"");
    body.push_str(&xml_escape(&link.href));
    body.push('"');
    for attribute in &link.attributes {
        body.push(' ');
        body.push_str(&attribute.name);
        body.push_str("=\"");
        body.push_str(&xml_escape(&attribute.value));
        body.push('"');
    }
    body.push_str("/>");
}

fn push_text_element(body: &mut String, name: &str, value: &str) {
    body.push('<');
    body.push_str(name);
    body.push('>');
    body.push_str(&xml_escape(value));
    body.push_str("</");
    body.push_str(name);
    body.push('>');
}

pub(super) fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn opds_content_markup(value: &str) -> String {
    xml_escape(value).replace('\n', "<br/>")
}

#[cfg(test)]
mod tests {
    use super::{
        OpdsV1NavigationFeedDocument, OpdsV1NavigationFeedEntry, OpdsV1XmlLink,
        render_opds_v1_navigation_feed,
    };

    #[test]
    fn navigation_feed_renderer_escapes_text_links_and_extra_attributes() {
        let body = render_opds_v1_navigation_feed(OpdsV1NavigationFeedDocument {
            id: "feed<&".to_string(),
            title: "Feed \"Title\"".to_string(),
            updated: "2024-01-02T03:04:05Z".to_string(),
            self_href: "http://example.test/opds?x=1&y=2".to_string(),
            start_href: "http://example.test/opds/v1.2/catalog".to_string(),
            previous_href: None,
            next_href: None,
            extra_links: vec![
                OpdsV1XmlLink::new(
                    "application/opds+json",
                    "alternate",
                    "http://example.test/opds/v2/catalog?x=1&y=2",
                )
                .with_attribute("pse:count", "7&8"),
            ],
            entries: vec![OpdsV1NavigationFeedEntry {
                id: "entry-1".to_string(),
                title: "Entry <One>".to_string(),
                updated: "2024-01-02T03:04:05Z".to_string(),
                content: "Line 1\nLine <2>".to_string(),
                href: "http://example.test/opds/v1.2/entry?x=1&y=2".to_string(),
            }],
        });

        assert!(body.contains("<id>feed&lt;&amp;</id>"));
        assert!(body.contains("<title>Feed &quot;Title&quot;</title>"));
        assert!(body.contains("href=\"http://example.test/opds?x=1&amp;y=2\""));
        assert!(body.contains("pse:count=\"7&amp;8\""));
        assert!(body.contains("<content>Line 1<br/>Line &lt;2&gt;</content>"));
    }
}
