//! Exporting Typst documents to EPUB.

mod archive;
mod prepare;
mod xml;

use ecow::EcoString;
use time::OffsetDateTime;
use typst_html::{HtmlDocument, HtmlOptions};
use typst_library::diag::SourceResult;
use typst_library::foundations::{Datetime, Smart};
use typst_library::model::Document;

/// Export an HTML document into an EPUB file.
///
/// The resulting file conforms to EPUB 3 and contains a package document,
/// navigation document, XHTML content document, and any embedded image assets.
#[typst_macros::time(name = "epub")]
pub fn epub(document: &HtmlDocument, options: &EpubOptions) -> SourceResult<Vec<u8>> {
    let prepared = prepare::prepare(document)?;
    let info = prepared.document.info();
    let title = info
        .title
        .as_deref()
        .or_else(|| prepared.headings.first().map(|heading| heading.title.as_str()))
        .unwrap_or("Untitled");

    let identifier = match &options.ident {
        Smart::Custom(ident) => ident.clone(),
        Smart::Auto => {
            let hash = typst_utils::hash128(&(&info.title, &info.author));
            format!("urn:typst:{hash:032x}")
        }
    };

    let modified = options
        .timestamp
        .and_then(format_modified)
        .unwrap_or_else(current_timestamp);
    let publication_date = match info.date {
        Smart::Custom(Some(datetime)) => format_publication_date(datetime),
        Smart::Auto | Smart::Custom(None) => None,
    };

    let html_options = HtmlOptions { pretty: options.pretty };
    let content = typst_html::xhtml(&prepared.document, &html_options)?;
    let navigation = xml::navigation(
        title,
        prepare::language(info),
        &prepared.headings,
        options.pretty,
    );
    let package = xml::package(
        info,
        title,
        &identifier,
        &modified,
        publication_date.as_deref(),
        &prepared.resources,
        prepared.properties,
        options.pretty,
    );

    archive::write(&content, &navigation, &package, &prepared.resources)
}

/// Settings for EPUB export.
#[derive(Debug, Default, Clone, PartialEq, Hash)]
pub struct EpubOptions {
    /// A unique and stable identifier for the publication.
    ///
    /// When set to [`Smart::Auto`], an identifier is derived from the title and
    /// authors. Callers that have a stable project identifier should provide it
    /// explicitly.
    pub ident: Smart<String>,
    /// The UTC modification timestamp written into the package metadata.
    ///
    /// If omitted, the current time is used.
    pub timestamp: Option<Datetime>,
    /// Whether to format XML and XHTML in a human-readable way.
    pub pretty: bool,
}

/// Format a Typst datetime as the UTC timestamp required by EPUB.
fn format_modified(datetime: Datetime) -> Option<EcoString> {
    let year = datetime.year()?;
    if !(0..=9999).contains(&year) {
        return None;
    }

    Some(format_datetime(
        year,
        datetime.month()?,
        datetime.day()?,
        datetime.hour().unwrap_or(0),
        datetime.minute().unwrap_or(0),
        datetime.second().unwrap_or(0),
        true,
    ))
}

/// Format the document's optional publication date.
fn format_publication_date(datetime: Datetime) -> Option<EcoString> {
    let year = datetime.year()?;
    if !(0..=9999).contains(&year) {
        return None;
    }

    let month = datetime.month()?;
    let day = datetime.day()?;
    match (datetime.hour(), datetime.minute(), datetime.second()) {
        (Some(hour), Some(minute), Some(second)) => {
            Some(format_datetime(year, month, day, hour, minute, second, false))
        }
        _ => Some(format!("{year:04}-{month:02}-{day:02}").into()),
    }
}

/// Return the current time as an EPUB modification timestamp.
fn current_timestamp() -> EcoString {
    let now = OffsetDateTime::now_utc();
    format_datetime(
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        true,
    )
}

/// Format a date and time with second precision.
fn format_datetime(
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    utc: bool,
) -> EcoString {
    let suffix = if utc { "Z" } else { "" };
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}{suffix}")
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_epub_timestamps() {
        let datetime = Datetime::from_ymd_hms(2025, 3, 7, 8, 9, 10).unwrap();
        assert_eq!(format_modified(datetime).unwrap(), "2025-03-07T08:09:10Z");
        assert_eq!(format_publication_date(datetime).unwrap(), "2025-03-07T08:09:10");

        let date = Datetime::from_ymd(2025, 3, 7).unwrap();
        assert_eq!(format_modified(date).unwrap(), "2025-03-07T00:00:00Z");
        assert_eq!(format_publication_date(date).unwrap(), "2025-03-07");
    }
}
