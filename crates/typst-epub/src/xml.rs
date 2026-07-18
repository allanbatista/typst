use typst_library::model::DocumentInfo;
use typst_library::text::Locale;
use xmlwriter::{Indent, Options, XmlWriter};

use crate::prepare::{ContentProperties, Heading, Resource, language};

/// Build the EPUB package document.
#[expect(clippy::too_many_arguments)]
pub fn package(
    info: &DocumentInfo,
    title: &str,
    identifier: &str,
    modified: &str,
    publication_date: Option<&str>,
    resources: &[Resource],
    properties: ContentProperties,
    pretty: bool,
) -> String {
    let locale = language(info);
    let lang = locale.rfc_3066();
    let mut xml = writer(pretty);
    xml.write_declaration();
    xml.start_element("package");
    xml.write_attribute("xmlns", "http://www.idpf.org/2007/opf");
    xml.write_attribute("version", "3.0");
    xml.write_attribute("unique-identifier", "pub-id");
    xml.write_attribute("xml:lang", &lang);

    xml.start_element("metadata");
    xml.write_attribute("xmlns:dc", "http://purl.org/dc/elements/1.1/");
    text_element_with_id(&mut xml, "dc:identifier", "pub-id", identifier);
    text_element(&mut xml, "dc:title", title);
    text_element(&mut xml, "dc:language", &lang);
    for author in &info.author {
        text_element(&mut xml, "dc:creator", author);
    }
    if let Some(description) = &info.description {
        text_element(&mut xml, "dc:description", description);
    }
    for keyword in &info.keywords {
        text_element(&mut xml, "dc:subject", keyword);
    }
    if let Some(date) = publication_date {
        text_element(&mut xml, "dc:date", date);
    }
    property_element(&mut xml, "dcterms:modified", modified);
    xml.end_element();

    xml.start_element("manifest");
    manifest_item(&mut xml, "nav", "nav.xhtml", "application/xhtml+xml", Some("nav"));
    let properties = properties.manifest_value();
    manifest_item(
        &mut xml,
        "content",
        "content.xhtml",
        "application/xhtml+xml",
        (!properties.is_empty()).then_some(properties.as_str()),
    );
    for (index, resource) in resources.iter().enumerate() {
        let id = format!("asset-{}", index + 1);
        manifest_item(&mut xml, &id, &resource.path, resource.media_type, None);
    }
    xml.end_element();

    xml.start_element("spine");
    if locale.lang.dir() == typst_library::layout::Dir::RTL {
        xml.write_attribute("page-progression-direction", "rtl");
    }
    xml.start_element("itemref");
    xml.write_attribute("idref", "content");
    xml.end_element();
    xml.end_element();

    xml.end_document()
}

/// Build the EPUB navigation document.
pub fn navigation(
    title: &str,
    locale: Locale,
    headings: &[Heading],
    pretty: bool,
) -> String {
    let lang = locale.rfc_3066();
    let mut xml = writer(pretty);
    xml.write_declaration();
    xml.start_element("html");
    xml.write_attribute("xmlns", "http://www.w3.org/1999/xhtml");
    xml.write_attribute("xmlns:epub", "http://www.idpf.org/2007/ops");
    xml.write_attribute("lang", &lang);
    xml.write_attribute("xml:lang", &lang);

    xml.start_element("head");
    text_element(&mut xml, "title", title);
    xml.end_element();

    xml.start_element("body");
    xml.start_element("nav");
    xml.write_attribute("epub:type", "toc");
    xml.write_attribute("id", "toc");
    text_element(&mut xml, "h1", "Contents");

    if headings.is_empty() {
        xml.start_element("ol");
        xml.start_element("li");
        link(&mut xml, "content.xhtml", title);
        xml.end_element();
        xml.end_element();
    } else {
        let tree = toc_tree(headings);
        write_toc(&mut xml, &tree);
    }

    xml.end_element();
    xml.end_element();
    xml.end_document()
}

/// A nested table-of-contents item.
struct TocItem<'a> {
    heading: &'a Heading,
    children: Vec<TocItem<'a>>,
}

/// Turn heading levels into a nested table of contents.
fn toc_tree(headings: &[Heading]) -> Vec<TocItem<'_>> {
    let mut roots = vec![];
    let mut path = vec![];
    let mut levels = vec![];

    for heading in headings {
        while levels.last().is_some_and(|level| *level >= heading.level) {
            levels.pop();
            path.pop();
        }

        let siblings = toc_children_mut(&mut roots, &path);
        siblings.push(TocItem { heading, children: vec![] });
        path.push(siblings.len() - 1);
        levels.push(heading.level);
    }

    roots
}

/// Follow a path into a table-of-contents tree and return its child list.
fn toc_children_mut<'a, 'h>(
    items: &'a mut Vec<TocItem<'h>>,
    path: &[usize],
) -> &'a mut Vec<TocItem<'h>> {
    match path.split_first() {
        Some((&index, rest)) => toc_children_mut(&mut items[index].children, rest),
        None => items,
    }
}

/// Write a nested navigation list.
fn write_toc(xml: &mut XmlWriter, items: &[TocItem<'_>]) {
    xml.start_element("ol");
    for item in items {
        xml.start_element("li");
        let href = format!("content.xhtml#{}", item.heading.id);
        link(xml, &href, &item.heading.title);
        if !item.children.is_empty() {
            write_toc(xml, &item.children);
        }
        xml.end_element();
    }
    xml.end_element();
}

/// Write an XML text element without introducing formatting whitespace.
fn text_element(xml: &mut XmlWriter, name: &str, value: &str) {
    xml.start_element(name);
    preserve_text(xml, value);
}

/// Write an XML text element with an `id` attribute.
fn text_element_with_id(xml: &mut XmlWriter, name: &str, id: &str, value: &str) {
    xml.start_element(name);
    xml.write_attribute("id", id);
    preserve_text(xml, value);
}

/// Write an EPUB metadata property.
fn property_element(xml: &mut XmlWriter, property: &str, value: &str) {
    xml.start_element("meta");
    xml.write_attribute("property", property);
    preserve_text(xml, value);
}

/// Finish an element with a compact text node.
fn preserve_text(xml: &mut XmlWriter, value: &str) {
    xml.set_preserve_whitespaces(true);
    xml.write_text(value);
    xml.end_element();
    xml.set_preserve_whitespaces(false);
}

/// Write an EPUB manifest item.
fn manifest_item(
    xml: &mut XmlWriter,
    id: &str,
    href: &str,
    media_type: &str,
    properties: Option<&str>,
) {
    xml.start_element("item");
    xml.write_attribute("id", id);
    xml.write_attribute("href", href);
    xml.write_attribute("media-type", media_type);
    if let Some(properties) = properties {
        xml.write_attribute("properties", properties);
    }
    xml.end_element();
}

/// Write a navigation link.
fn link(xml: &mut XmlWriter, href: &str, title: &str) {
    xml.start_element("a");
    xml.write_attribute("href", href);
    preserve_text(xml, title);
}

/// Construct a consistently configured XML writer.
fn writer(pretty: bool) -> XmlWriter {
    XmlWriter::new(Options {
        use_single_quote: false,
        indent: if pretty { Indent::Spaces(2) } else { Indent::None },
        attributes_indent: Indent::None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nests_heading_levels() {
        let headings = [
            Heading { level: 1, id: "a".into(), title: "A".into() },
            Heading { level: 2, id: "b".into(), title: "B".into() },
            Heading { level: 3, id: "c".into(), title: "C".into() },
            Heading { level: 2, id: "d".into(), title: "D".into() },
            Heading { level: 1, id: "e".into(), title: "E".into() },
        ];
        let tree = toc_tree(&headings);
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].children.len(), 2);
        assert_eq!(tree[0].children[0].children.len(), 1);
        assert!(tree[1].children.is_empty());
    }
}
