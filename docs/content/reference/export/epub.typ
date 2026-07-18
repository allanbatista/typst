#import "../../../components/index.typ": docs-category, info

#show: docs-category.with(
  title: "EPUB",
  description: "Documentation for Typst's EPUB export target.",
  category: "epub",
)

#info[
  Typst's EPUB export is experimental and builds on the HTML export pipeline. It is only available behind the `html` feature flag and is not yet intended for production use. In the CLI, pass `--features html` or set the `TYPST_FEATURES` environment variable to `html`. EPUB export is not currently available in the web app.
]

EPUB is a reflowable e-book format supported by dedicated readers and reading applications. Typst exports an EPUB 3 publication containing XHTML content, a navigation document, package metadata, and embedded image assets.

Like @html[HTML export], EPUB export preserves the semantic structure of a document instead of its paged layout. Headings become the publication's table of contents, text remains selectable, and supported images are stored inside the EPUB container. Page dimensions, fixed positioning, and other visual details specific to paged output are not preserved.

= Exporting as EPUB <exporting-as-epub>
== Command Line <command-line>
Pass `--format epub` to the `compile` or `watch` subcommand, or provide an output file name ending in `.epub`. The experimental HTML pipeline must also be enabled.

```sh
typst compile book.typ book.epub --features html
```

Pass `--pretty` to make the generated XML and XHTML files easier to inspect. This does not affect how the book is rendered by readers.

== Web App <web-app>
Not currently available.

= Publication metadata <publication-metadata>
EPUB metadata is populated from the @document function. The title, authors, description, keywords, date, and language become their corresponding publication metadata. If no title is configured, Typst uses the first heading and otherwise falls back to "Untitled."

```typ
#set document(
  title: "A Short Book",
  author: ("Ada Lovelace", "Alan Turing"),
  description: "An example publication.",
  keywords: ("example", "epub"),
  date: datetime(year: 2026, month: 7, day: 18),
)
```

= Contents and assets <contents-and-assets>
Typst creates a nested table of contents from document headings. Existing heading labels remain valid fragment targets; headings without an identifier receive a stable identifier during export.

Base64-embedded GIF, JPEG, PNG, SVG, and WebP images are extracted into the publication and listed in its package manifest. Math is emitted as namespaced MathML so compatible readers can present it semantically.
