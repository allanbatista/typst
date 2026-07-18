use std::fmt::Display;
use std::io::{Cursor, Write};

use ecow::eco_format;
use typst_library::diag::{At, SourceResult};
use typst_syntax::Span;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::prepare::Resource;

/// The EPUB container descriptor.
const CONTAINER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="EPUB/package.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>
"#;

/// Write all EPUB files into their OCF ZIP container.
pub fn write(
    content: &str,
    navigation: &str,
    package: &str,
    resources: &[Resource],
) -> SourceResult<Vec<u8>> {
    let cursor = Cursor::new(Vec::new());
    let mut archive = ZipWriter::new(cursor);
    let stored =
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let compressed =
        SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    archive_result(archive.start_file("mimetype", stored))?;
    archive_result(archive.write_all(b"application/epub+zip"))?;

    write_file(&mut archive, "META-INF/container.xml", CONTAINER, compressed)?;
    write_file(&mut archive, "EPUB/package.opf", package, compressed)?;
    write_file(&mut archive, "EPUB/nav.xhtml", navigation, compressed)?;
    write_file(&mut archive, "EPUB/content.xhtml", content, compressed)?;

    for resource in resources {
        let path = format!("EPUB/{}", resource.path);
        archive_result(archive.start_file(path, compressed))?;
        archive_result(archive.write_all(&resource.data))?;
    }

    let cursor = archive_result(archive.finish())?;
    Ok(cursor.into_inner())
}

/// Write a UTF-8 file into the archive.
fn write_file(
    archive: &mut ZipWriter<Cursor<Vec<u8>>>,
    path: &str,
    contents: &str,
    options: SimpleFileOptions,
) -> SourceResult<()> {
    archive_result(archive.start_file(path, options))?;
    archive_result(archive.write_all(contents.as_bytes()))
}

/// Turn an archive error into a detached Typst diagnostic.
fn archive_result<T, E: Display>(result: Result<T, E>) -> SourceResult<T> {
    result
        .map_err(|error| eco_format!("failed to write EPUB ({error})"))
        .at(Span::detached())
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use zip::ZipArchive;

    use super::*;

    #[test]
    fn writes_required_epub_entries() {
        let data = write("content", "navigation", "package", &[]).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(data)).unwrap();
        assert_eq!(archive.len(), 5);

        {
            let mut mimetype = archive.by_index(0).unwrap();
            assert_eq!(mimetype.name(), "mimetype");
            assert_eq!(mimetype.compression(), CompressionMethod::Stored);
            let mut contents = String::new();
            mimetype.read_to_string(&mut contents).unwrap();
            assert_eq!(contents, "application/epub+zip");
        }

        for path in [
            "META-INF/container.xml",
            "EPUB/package.opf",
            "EPUB/nav.xhtml",
            "EPUB/content.xhtml",
        ] {
            assert!(archive.by_name(path).is_ok(), "missing {path}");
        }
    }
}
