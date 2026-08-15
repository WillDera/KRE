//! OPF package document parsing: metadata, manifest, spine.

use crate::EpubError;
use crate::xml::{DC_NS, OPF_NS, parse};

/// Parsed OPF package: metadata, manifest, spine order.
pub struct Package {
    pub metadata: Metadata,
    pub manifest: Vec<ManifestItem>,
    /// Spine itemrefs in reading order (manifest ids).
    pub spine: Vec<String>,
}

pub struct Metadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
    pub identifiers: Vec<IdentifierValue>,
    pub rights: Option<String>,
}

pub struct IdentifierValue {
    pub scheme: String,
    pub value: String,
}

pub struct ManifestItem {
    pub id: String,
    pub href: String,
    pub media_type: String,
    /// True for the EPUB3 navigation document (manifest `properties="nav"`).
    pub is_nav: bool,
}

pub fn parse_package(bytes: &[u8], path: &str) -> Result<Package, EpubError> {
    let doc = parse(bytes, path)?;
    let root = doc.root_element();

    let mut metadata = Metadata {
        title: None,
        author: None,
        language: None,
        identifiers: Vec::new(),
        rights: None,
    };
    let mut manifest = Vec::new();
    let mut spine = Vec::new();

    for node in root.descendants().filter(|n| n.is_element()) {
        let name = node.tag_name().name();
        let ns = node.tag_name().namespace();
        match (name, ns) {
            ("title", Some(DC_NS)) => {
                metadata.title = node.text().map(str::to_owned).or(metadata.title)
            }
            ("creator", Some(DC_NS)) => {
                metadata.author = node.text().map(str::to_owned).or(metadata.author)
            }
            ("language", Some(DC_NS)) => {
                metadata.language = node.text().map(str::to_owned).or(metadata.language)
            }
            ("identifier", Some(DC_NS)) => {
                if let Some(v) = node.text() {
                    metadata.identifiers.push(IdentifierValue {
                        scheme: "dc".to_owned(),
                        value: v.to_owned(),
                    });
                }
            }
            ("rights", Some(DC_NS)) => {
                metadata.rights = node.text().map(str::to_owned).or(metadata.rights)
            }
            ("item", Some(OPF_NS)) => {
                let id = node.attribute("id").unwrap_or_default().to_owned();
                let href = node.attribute("href").unwrap_or_default().to_owned();
                let media_type = node.attribute("media-type").unwrap_or_default().to_owned();
                let is_nav = node
                    .attribute("properties")
                    .map(|p| p.split_whitespace().any(|t| t == "nav"))
                    .unwrap_or(false);
                manifest.push(ManifestItem {
                    id,
                    href,
                    media_type,
                    is_nav,
                });
            }
            ("itemref", Some(OPF_NS)) => {
                if let Some(idref) = node.attribute("idref") {
                    spine.push(idref.to_owned());
                }
            }
            _ => {}
        }
    }

    if manifest.is_empty() {
        return Err(EpubError::Package("empty manifest".into()));
    }
    Ok(Package {
        metadata,
        manifest,
        spine,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="pub-id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="pub-id">urn:uuid:11111111-2222-3333-4444-555555555555</dc:identifier>
    <dc:title>The Long Dark</dc:title>
    <dc:creator>Big Dog</dc:creator>
    <dc:language>en</dc:language>
    <dc:rights>Public Domain</dc:rights>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ch1" href="text/ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="pic" href="Images/pic.png" media-type="image/png"/>
  </manifest>
  <spine>
    <itemref idref="nav"/>
    <itemref idref="ch1"/>
  </spine>
</package>"#;

    #[test]
    fn parses_metadata_manifest_spine() {
        let p = parse_package(OPF.as_bytes(), "OEBPS/content.opf").expect("parse");
        assert_eq!(p.metadata.title.as_deref(), Some("The Long Dark"));
        assert_eq!(p.metadata.author.as_deref(), Some("Big Dog"));
        assert_eq!(p.metadata.language.as_deref(), Some("en"));
        assert_eq!(p.metadata.rights.as_deref(), Some("Public Domain"));
        assert_eq!(p.metadata.identifiers.len(), 1);
        assert_eq!(p.spine, vec!["nav", "ch1"]);
        assert_eq!(p.manifest.len(), 3);
        assert!(p.manifest.iter().find(|m| m.id == "nav").unwrap().is_nav);
        assert!(!p.manifest.iter().find(|m| m.id == "ch1").unwrap().is_nav);
    }
}
