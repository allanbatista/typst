use std::collections::{BTreeMap, HashSet};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ecow::EcoString;
use typst_html::{HtmlDocument, HtmlElement, HtmlNode, attr, tag};
use typst_library::diag::{At, SourceResult};
use typst_library::foundations::Smart;
use typst_library::model::DocumentInfo;
use typst_library::text::Locale;
use typst_syntax::Span;

/// An extracted resource that will be stored in the EPUB container.
#[derive(Debug)]
pub struct Resource {
    /// Path relative to the EPUB content directory.
    pub path: EcoString,
    /// Internet media type for the package manifest.
    pub media_type: &'static str,
    /// Raw resource data.
    pub data: Vec<u8>,
}

/// A heading exposed in the EPUB navigation document.
#[derive(Debug)]
pub struct Heading {
    /// HTML heading level.
    pub level: u8,
    /// Fragment identifier in the content document.
    pub id: EcoString,
    /// Plain-text heading title.
    pub title: EcoString,
}

/// Special content properties required in the package manifest.
#[derive(Debug, Default, Copy, Clone)]
pub struct ContentProperties {
    pub mathml: bool,
    pub remote_resources: bool,
    pub scripted: bool,
    pub svg: bool,
}

impl ContentProperties {
    /// Serialize properties in a stable order.
    pub fn manifest_value(self) -> EcoString {
        let mut properties = Vec::with_capacity(4);
        if self.mathml {
            properties.push("mathml");
        }
        if self.remote_resources {
            properties.push("remote-resources");
        }
        if self.scripted {
            properties.push("scripted");
        }
        if self.svg {
            properties.push("svg");
        }
        properties.join(" ").into()
    }
}

/// A document prepared for serialization into an EPUB.
pub struct Prepared {
    pub document: HtmlDocument,
    pub headings: Vec<Heading>,
    pub resources: Vec<Resource>,
    pub properties: ContentProperties,
}

/// Clone and adapt an HTML document for the restrictions of EPUB.
pub fn prepare(document: &HtmlDocument) -> SourceResult<Prepared> {
    let mut document = document.clone();
    let mut used_ids = HashSet::new();
    collect_ids(document.root(), &mut used_ids);

    let mut headings = vec![];
    let mut resources = BTreeMap::new();
    let mut properties = ContentProperties::default();
    let mut heading_index = 1;
    visit(
        document.root_mut(),
        &mut used_ids,
        &mut heading_index,
        &mut headings,
        &mut resources,
        &mut properties,
    )?;

    Ok(Prepared {
        document,
        headings,
        resources: resources.into_values().collect(),
        properties,
    })
}

/// Resolve the document language, defaulting to English.
pub fn language(info: &DocumentInfo) -> Locale {
    match info.locale {
        Smart::Custom(locale) => locale,
        Smart::Auto => Locale::DEFAULT,
    }
}

/// Collect all existing fragment identifiers before assigning heading IDs.
fn collect_ids(element: &HtmlElement, ids: &mut HashSet<EcoString>) {
    if let Some(id) = element.attrs.get(attr::id) {
        ids.insert(id.clone());
    }

    for child in &element.children {
        match child {
            HtmlNode::Element(child) => collect_ids(child, ids),
            HtmlNode::Frame(frame) => {
                if let Some(id) = &frame.id {
                    ids.insert(id.clone());
                }
            }
            HtmlNode::Tag(_) | HtmlNode::Text(..) => {}
        }
    }
}

/// Visit the DOM, extracting resources and building navigation metadata.
fn visit(
    element: &mut HtmlElement,
    used_ids: &mut HashSet<EcoString>,
    heading_index: &mut usize,
    headings: &mut Vec<Heading>,
    resources: &mut BTreeMap<EcoString, Resource>,
    properties: &mut ContentProperties,
) -> SourceResult<()> {
    properties.mathml |= tag::mathml::is_mathml(element.tag);
    properties.scripted |= element.tag == tag::script;
    properties.svg |= element.tag.resolve().as_str() == "svg";

    for (name, value) in &element.attrs.0 {
        let name = name.resolve();
        properties.scripted |= name.as_str().starts_with("on");
        properties.remote_resources |=
            matches!(name.as_str(), "data" | "poster" | "src") && is_remote(value);
    }

    if let Some(src) = element.attrs.get_mut(attr::src)
        && let Some(resource) = decode_data_resource(src, element.span)?
    {
        let path = resource.path.clone();
        resources.entry(path.clone()).or_insert(resource);
        *src = path;
    }

    if let Some(level) = heading_level(element) {
        let title = plain_text(&element.children);
        if !title.is_empty() {
            let id = match element.attrs.get(attr::id) {
                Some(id) => id.clone(),
                None => {
                    let id = next_heading_id(used_ids, heading_index);
                    element.attrs.push(attr::id, id.clone());
                    id
                }
            };
            headings.push(Heading { level, id, title });
        }
    }

    for child in element.children.make_mut() {
        match child {
            HtmlNode::Element(child) => {
                visit(child, used_ids, heading_index, headings, resources, properties)?;
            }
            HtmlNode::Frame(_) => properties.svg = true,
            HtmlNode::Tag(_) | HtmlNode::Text(..) => {}
        }
    }

    Ok(())
}

/// Decode a supported base64 data URL into an EPUB resource.
fn decode_data_resource(value: &str, span: Span) -> SourceResult<Option<Resource>> {
    let Some(rest) = value.strip_prefix("data:") else { return Ok(None) };
    let Some((metadata, encoded)) = rest.split_once(',') else {
        return Ok(None);
    };
    let mut parts = metadata.split(';');
    let Some(media_type) = parts.next() else { return Ok(None) };
    if !parts.any(|part| part.eq_ignore_ascii_case("base64")) {
        return Ok(None);
    }
    let Some((media_type, extension)) = supported_resource(media_type) else {
        return Ok(None);
    };

    let data = STANDARD
        .decode(encoded)
        .map_err(|error| {
            EcoString::from(format!("failed to decode EPUB resource ({error})"))
        })
        .at(span)?;
    let hash = typst_utils::hash128(&data);
    let path = format!("assets/{hash:032x}.{extension}").into();
    Ok(Some(Resource { path, media_type, data }))
}

/// Return canonical media type and extension for EPUB core image formats.
fn supported_resource(media_type: &str) -> Option<(&'static str, &'static str)> {
    Some(match media_type.to_ascii_lowercase().as_str() {
        "image/gif" => ("image/gif", "gif"),
        "image/jpeg" | "image/jpg" => ("image/jpeg", "jpg"),
        "image/png" => ("image/png", "png"),
        "image/svg+xml" => ("image/svg+xml", "svg"),
        "image/webp" => ("image/webp", "webp"),
        _ => return None,
    })
}

/// Whether a URL points to a remote resource.
fn is_remote(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

/// Find the level of an HTML heading element.
fn heading_level(element: &HtmlElement) -> Option<u8> {
    Some(match element.tag {
        tag::h1 => 1,
        tag::h2 => 2,
        tag::h3 => 3,
        tag::h4 => 4,
        tag::h5 => 5,
        tag::h6 => 6,
        _ => return None,
    })
}

/// Return a new heading ID that does not collide with existing DOM IDs.
fn next_heading_id(ids: &mut HashSet<EcoString>, index: &mut usize) -> EcoString {
    loop {
        let id: EcoString = format!("heading-{index}").into();
        *index += 1;
        if ids.insert(id.clone()) {
            return id;
        }
    }
}

/// Collect and normalize the textual contents of a node list.
fn plain_text(nodes: &[HtmlNode]) -> EcoString {
    fn collect(nodes: &[HtmlNode], output: &mut String) {
        for node in nodes {
            match node {
                HtmlNode::Text(text, _) => output.push_str(text),
                HtmlNode::Element(element) => collect(&element.children, output),
                HtmlNode::Tag(_) | HtmlNode::Frame(_) => {}
            }
        }
    }

    let mut text = String::new();
    collect(nodes, &mut text);
    text.split_whitespace().collect::<Vec<_>>().join(" ").into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_supported_resources() {
        assert_eq!(supported_resource("image/png"), Some(("image/png", "png")));
        assert_eq!(supported_resource("IMAGE/JPEG"), Some(("image/jpeg", "jpg")));
        assert_eq!(supported_resource("text/plain"), None);
    }

    #[test]
    fn extracts_base64_image_resources() {
        let resource =
            decode_data_resource("data:image/png;base64,iVBORw0KGgo=", Span::detached())
                .unwrap()
                .unwrap();
        assert_eq!(resource.media_type, "image/png");
        assert!(resource.path.starts_with("assets/"));
        assert!(resource.path.ends_with(".png"));
        assert_eq!(resource.data, b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn creates_unique_heading_ids() {
        let mut ids = HashSet::from([EcoString::from("heading-1")]);
        let mut index = 1;
        assert_eq!(next_heading_id(&mut ids, &mut index), "heading-2");
        assert_eq!(next_heading_id(&mut ids, &mut index), "heading-3");
    }
}
