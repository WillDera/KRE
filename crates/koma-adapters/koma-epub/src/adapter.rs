//! [`EpubAdapter`]: the EPUB -> KIR content adapter.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

use koma_core::adapters::{AdapterError, ContentAdapter, ContentSource};
use koma_core::kir::{
    Attribution, Block, Chapter, Document, DocumentMetadata, Identifier, KIR_VERSION, Section,
    block,
};
use zip::ZipArchive;

use crate::{container, content, package, path, toc};

/// EPUB content adapter. Registered as a `ContentAdapter` plugin.
pub struct EpubAdapter;

impl ContentAdapter for EpubAdapter {
    fn id(&self) -> &'static str {
        "koma-epub"
    }

    fn supports(&self, source: &ContentSource) -> bool {
        source.format.as_deref() == Some("epub") || source.uri.to_lowercase().ends_with(".epub")
    }

    fn to_kir(&self, source: &ContentSource) -> Result<Document, AdapterError> {
        let file = File::open(&source.uri).map_err(AdapterError::Io)?;
        let mut archive = ZipArchive::new(file).map_err(|e| EpubError::Zip(e.to_string()))?;

        let mimetype = read_bytes(&mut archive, "mimetype")?;
        if String::from_utf8_lossy(&mimetype).trim() != "application/epub+zip" {
            return Err(EpubError::NotEpub(format!(
                "mimetype `{}`",
                String::from_utf8_lossy(&mimetype)
            ))
            .into());
        }

        let container_bytes = read_bytes(&mut archive, "META-INF/container.xml")?;
        let opf_path = container::parse_container(&container_bytes)?.opf_path;
        let opf_bytes = read_bytes(&mut archive, &opf_path)?;
        let pkg = package::parse_package(&opf_bytes, &opf_path)?;

        let base = path::dir(&opf_path);

        // Map resolved manifest paths -> manifest ids, and locate nav/NCX.
        let mut media_ids = HashMap::new();
        let mut nav_path: Option<String> = None;
        let mut ncx_path: Option<String> = None;
        for item in &pkg.manifest {
            let resolved = path::resolve(&base, &item.href);
            media_ids.insert(resolved.clone(), item.id.clone());
            if item.is_nav {
                nav_path = Some(resolved);
            } else if item.media_type == "application/x-dtbncx+xml" {
                ncx_path = Some(resolved);
            }
        }

        let toc = if let Some(nav_path) = &nav_path {
            read_bytes_opt(&mut archive, nav_path)?
                .map(|b| toc::parse_nav(&b, nav_path))
                .transpose()?
                .unwrap_or_default()
        } else if let Some(ncx_path) = &ncx_path {
            read_bytes_opt(&mut archive, ncx_path)?
                .map(|b| toc::parse_ncx(&b, ncx_path))
                .transpose()?
                .unwrap_or_default()
        } else {
            HashMap::new()
        };

        // Spine items in reading order -> one KIR Chapter each (lazy-load
        // boundary). The EPUB3 navigation document is not reading content.
        let mut chapters = Vec::new();
        let mut seed: u64 = 0;
        for idref in &pkg.spine {
            let Some(item) = pkg.manifest.iter().find(|m| &m.id == idref) else {
                continue;
            };
            let resolved = path::resolve(&base, &item.href);
            if nav_path.as_ref() == Some(&resolved) {
                continue;
            }
            let bytes = read_bytes(&mut archive, &resolved)?;
            seed = seed.wrapping_add(fnv1a(&bytes));
            let blocks = content::parse_content_document(&bytes, &resolved, &media_ids)?;
            let title = toc
                .get(&resolved)
                .cloned()
                .or_else(|| first_heading(&blocks))
                .or_else(|| pkg.metadata.title.clone());
            chapters.push(Chapter {
                id: item.id.clone(),
                title,
                sections: vec![Section {
                    id: resolved,
                    blocks,
                }],
            });
        }

        let metadata = pkg.metadata;
        let document = Document {
            version: KIR_VERSION.to_owned(),
            metadata: Some(DocumentMetadata {
                title: metadata.title,
                author: metadata.author,
                language: metadata.language,
                identifiers: metadata
                    .identifiers
                    .into_iter()
                    .map(|i| Identifier {
                        scheme: i.scheme,
                        value: i.value,
                    })
                    .collect(),
                license: metadata.rights.map(|r| Attribution {
                    name: None,
                    creator: None,
                    license: Some(r),
                    source: None,
                    version: None,
                }),
                generator: format!("koma-epub {}", env!("CARGO_PKG_VERSION")),
                seed,
            }),
            chapters,
        };
        Ok(document)
    }
}

/// Error type for EPUB adaptation. Converts into [`AdapterError`].
#[derive(Debug, thiserror::Error)]
pub enum EpubError {
    #[error("not an EPUB package: {0}")]
    NotEpub(String),
    #[error("zip/container error: {0}")]
    Zip(String),
    #[error("required resource missing from package: {0}")]
    Missing(String),
    #[error("XML parse error in `{0}`: {1}")]
    Xml(String, String),
    #[error("invalid OPF package: {0}")]
    Package(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<EpubError> for AdapterError {
    fn from(e: EpubError) -> Self {
        match e {
            EpubError::NotEpub(m) => AdapterError::Parse("container".to_owned(), m),
            EpubError::Zip(m) => AdapterError::Parse("zip".to_owned(), m),
            EpubError::Missing(m) => AdapterError::MissingResource(m),
            EpubError::Xml(p, m) => AdapterError::Parse(p, m),
            EpubError::Package(m) => AdapterError::Parse("package".to_owned(), m),
            EpubError::Io(io) => AdapterError::Io(io),
        }
    }
}

fn read_bytes(archive: &mut ZipArchive<File>, name: &str) -> Result<Vec<u8>, EpubError> {
    let mut f = archive
        .by_name(name)
        .map_err(|e| EpubError::Missing(format!("{name}: {e}")))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    Ok(buf)
}

fn read_bytes_opt(
    archive: &mut ZipArchive<File>,
    name: &str,
) -> Result<Option<Vec<u8>>, EpubError> {
    let mut f = match archive.by_name(name) {
        Ok(f) => f,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(EpubError::Zip(e.to_string())),
    };
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    Ok(Some(buf))
}

fn first_heading(blocks: &[Block]) -> Option<String> {
    for b in blocks {
        if let Some(block::Kind::Heading(h)) = &b.kind {
            let text: String = h.spans.iter().map(|s| s.text.clone()).collect();
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    None
}

/// FNV-1a 64-bit; deterministic content hash for the reproducibility seed.
fn fnv1a(bytes: &[u8]) -> u64 {
    const PRIME: u64 = 1_099_511_628_211;
    const OFFSET: u64 = 14_695_981_039_346_656_037;
    bytes
        .iter()
        .fold(OFFSET, |h, b| (h ^ *b as u64).wrapping_mul(PRIME))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use koma_core::kir::{KIR_VERSION, block};
    use tempfile::NamedTempFile;

    use super::*;

    fn build_epub(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for (name, data) in files {
                w.start_file(*name, opts).unwrap();
                w.write_all(data).unwrap();
            }
            w.finish().unwrap();
        }
        buf
    }

    fn write_temp(bytes: &[u8]) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(bytes).unwrap();
        f.flush().unwrap();
        f
    }

    const MIMETYPE: &[u8] = b"application/epub+zip";

    fn container_xml() -> String {
        r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#
            .to_owned()
    }

    fn opf3(nav: bool) -> String {
        let nav_item = if nav {
            r#"<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#
        } else {
            ""
        };
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="pub-id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="pub-id">urn:uuid:11111111-2222-3333-4444-555555555555</dc:identifier>
    <dc:title>The Long Dark</dc:title>
    <dc:creator>Big Dog</dc:creator>
    <dc:language>en</dc:language>
    <dc:rights>Public Domain</dc:rights>
  </metadata>
  <manifest>
    {nav_item}
    <item id="ch1" href="text/ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch2" href="text/ch2.xhtml" media-type="application/xhtml+xml"/>
    <item id="pic" href="Images/pic.png" media-type="image/png"/>
  </manifest>
  <spine>
    {spine}
  </spine>
</package>"#,
            nav_item = nav_item,
            spine = if nav {
                "<itemref idref=\"nav\"/><itemref idref=\"ch1\"/><itemref idref=\"ch2\"/>"
            } else {
                "<itemref idref=\"ch1\"/><itemref idref=\"ch2\"/>"
            },
        )
    }

    fn nav_xhtml() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head><title>Contents</title></head>
<body>
  <nav epub:type="toc">
    <ol>
      <li><a href="text/ch1.xhtml">Chapter 1</a></li>
      <li><a href="text/ch2.xhtml">Chapter 2</a></li>
    </ol>
  </nav>
</body>
</html>"#
            .to_owned()
    }

    fn ch1_xhtml() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Chapter 1</title></head>
<body>
  <h1>Chapter 1</h1>
  <p>A cold <em>coming</em>.</p>
  <img src="../Images/pic.png" alt="frost"/>
</body>
</html>"#
            .to_owned()
    }

    fn ch2_xhtml() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Chapter 2</title></head>
<body>
  <h2>Deep Winter</h2>
  <p>Snow fell without sound.</p>
</body>
</html>"#
            .to_owned()
    }

    fn epub3_bytes() -> Vec<u8> {
        build_epub(&[
            ("mimetype", MIMETYPE),
            ("META-INF/container.xml", container_xml().as_bytes()),
            ("OEBPS/content.opf", opf3(true).as_bytes()),
            ("OEBPS/nav.xhtml", nav_xhtml().as_bytes()),
            ("OEBPS/text/ch1.xhtml", ch1_xhtml().as_bytes()),
            ("OEBPS/text/ch2.xhtml", ch2_xhtml().as_bytes()),
            ("OEBPS/Images/pic.png", b"\x89PNG-fake"),
        ])
    }

    fn adapt_bytes(bytes: &[u8]) -> Document {
        let f = write_temp(bytes);
        let adapter = EpubAdapter;
        adapter
            .to_kir(&ContentSource::new(f.path().to_string_lossy().into_owned()))
            .expect("adapt")
    }

    #[test]
    fn supports_epub_sources() {
        let adapter = EpubAdapter;
        assert!(adapter.supports(&ContentSource::new("book.epub")));
        assert!(adapter.supports(&ContentSource::new("book.EPUB")));
        assert!(adapter.supports(&ContentSource::new("book").with_format("epub")));
        assert!(!adapter.supports(&ContentSource::new("book.pdf")));
        assert!(!adapter.supports(&ContentSource::new("book").with_format("markdown")));
    }

    #[test]
    fn adapts_epub3_to_kir() {
        let doc = adapt_bytes(&epub3_bytes());

        assert_eq!(doc.version, KIR_VERSION);
        let meta = doc.metadata.as_ref().expect("metadata");
        assert_eq!(meta.title.as_deref(), Some("The Long Dark"));
        assert_eq!(meta.author.as_deref(), Some("Big Dog"));
        assert_eq!(meta.language.as_deref(), Some("en"));
        assert_eq!(
            meta.license.as_ref().and_then(|l| l.license.as_deref()),
            Some("Public Domain")
        );
        assert!(meta.generator.starts_with("koma-epub "));
        assert_ne!(meta.seed, 0);
        assert_eq!(meta.identifiers.len(), 1);

        // Two chapters (nav skipped); chapter 1 from toc, chapter 2 from toc.
        assert_eq!(doc.chapters.len(), 2);
        assert_eq!(doc.chapters[0].title.as_deref(), Some("Chapter 1"));
        assert_eq!(doc.chapters[1].title.as_deref(), Some("Chapter 2"));
        assert_eq!(doc.chapters[0].sections.len(), 1);

        // Chapter 1: heading + paragraph + image, image id from manifest.
        let blocks = &doc.chapters[0].sections[0].blocks;
        assert!(matches!(blocks[0].kind, Some(block::Kind::Heading(_))));
        let para = match &blocks[1].kind {
            Some(block::Kind::Paragraph(p)) => p,
            _ => panic!("expected paragraph"),
        };
        assert_eq!(para.spans.len(), 3);
        assert_eq!(para.spans[0].text, "A cold");
        assert_eq!(para.spans[0].style, None);
        assert_eq!(para.spans[1].text, "coming");
        assert_eq!(
            para.spans[1].style.as_ref().map(|s| s.italic),
            Some(Some(true))
        );
        assert_eq!(para.spans[2].text, ".");
        assert_eq!(para.spans[2].style, None);
        let img = match &blocks[2].kind {
            Some(block::Kind::Image(i)) => i,
            _ => panic!("expected image"),
        };
        assert_eq!(img.media_id, "pic");
        assert_eq!(img.alt.as_deref(), Some("frost"));
    }

    #[test]
    fn chapter_title_falls_back_to_first_heading() {
        // EPUB without a TOC: chapter titles fall back to first headings.
        let epub = build_epub(&[
            ("mimetype", MIMETYPE),
            ("META-INF/container.xml", container_xml().as_bytes()),
            ("OEBPS/content.opf", opf3(false).as_bytes()),
            ("OEBPS/text/ch1.xhtml", ch1_xhtml().as_bytes()),
            ("OEBPS/text/ch2.xhtml", ch2_xhtml().as_bytes()),
            ("OEBPS/Images/pic.png", b"\x89PNG-fake"),
        ]);
        // Without NCX manifest entry this has no toc; titles come from headings.
        let doc = adapt_bytes(&epub);
        assert_eq!(doc.chapters.len(), 2);
        assert_eq!(doc.chapters[0].title.as_deref(), Some("Chapter 1"));
        assert_eq!(doc.chapters[1].title.as_deref(), Some("Deep Winter"));
    }

    #[test]
    fn uses_ncx_for_epub2() {
        let ncx = r#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <navMap>
    <navPoint id="n1" playOrder="1">
      <navLabel><text>Chapter 1</text></navLabel>
      <content src="text/ch1.xhtml"/>
    </navPoint>
  </navMap>
</ncx>"#;
        let opf2 = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="pub-id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="pub-id">urn:uuid:11111111-2222-3333-4444-555555555555</dc:identifier>
    <dc:title>The Long Dark</dc:title>
    <dc:creator>Big Dog</dc:creator>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="ch1" href="text/ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine toc="ncx">
    <itemref idref="ch1"/>
  </spine>
</package>"#;
        let epub = build_epub(&[
            ("mimetype", MIMETYPE),
            ("META-INF/container.xml", container_xml().as_bytes()),
            ("OEBPS/content.opf", opf2.as_bytes()),
            ("OEBPS/toc.ncx", ncx.as_bytes()),
            ("OEBPS/text/ch1.xhtml", ch1_xhtml().as_bytes()),
            ("OEBPS/Images/pic.png", b"\x89PNG-fake"),
        ]);
        let doc = adapt_bytes(&epub);
        assert_eq!(doc.chapters.len(), 1);
        assert_eq!(doc.chapters[0].title.as_deref(), Some("Chapter 1"));
    }

    #[test]
    fn rejects_non_epub_mimetype() {
        let epub = build_epub(&[
            ("mimetype", b"application/zip"),
            ("META-INF/container.xml", container_xml().as_bytes()),
        ]);
        let f = write_temp(&epub);
        let err = EpubAdapter
            .to_kir(&ContentSource::new(f.path().to_string_lossy().into_owned()))
            .expect_err("should fail");
        assert!(matches!(err, AdapterError::Parse(..)));
    }

    #[test]
    fn rejects_missing_container() {
        let epub = build_epub(&[("mimetype", MIMETYPE)]);
        let f = write_temp(&epub);
        let err = EpubAdapter
            .to_kir(&ContentSource::new(f.path().to_string_lossy().into_owned()))
            .expect_err("should fail");
        assert!(matches!(err, AdapterError::MissingResource(_)));
    }
}
