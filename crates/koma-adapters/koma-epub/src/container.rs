//! `META-INF/container.xml` parsing: locates the OPF package document.

use crate::EpubError;
use crate::xml::{CONTAINER_NS, is_ns};

/// Path of the OPF package document inside the zip container.
pub struct Container {
    pub opf_path: String,
}

pub fn parse_container(bytes: &[u8]) -> Result<Container, EpubError> {
    let doc = crate::xml::parse(bytes, "META-INF/container.xml")?;
    let root = doc.root_element();
    let opf_path = root
        .descendants()
        .find(|n| is_ns(n, CONTAINER_NS, "rootfile"))
        .and_then(|n| n.attribute("full-path"))
        .map(str::to_owned)
        .ok_or_else(|| EpubError::NotEpub("missing rootfile/full-path".into()))?;
    if opf_path.is_empty() {
        return Err(EpubError::NotEpub("empty rootfile full-path".into()));
    }
    Ok(Container { opf_path })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTAINER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

    #[test]
    fn finds_opf_path() {
        let c = parse_container(CONTAINER.as_bytes()).expect("parse");
        assert_eq!(c.opf_path, "OEBPS/content.opf");
    }

    #[test]
    fn rejects_missing_rootfile() {
        assert!(parse_container(b"<container/>").is_err());
    }
}
