//! Table of contents parsing: EPUB3 `nav.xhtml` and EPUB2 `NCX`.
//!
//! Both produce a map from resolved content path -> chapter title. The first
//! entry referencing a spine document wins.

use std::collections::HashMap;

use crate::EpubError;
use crate::path;
use crate::xml::{EPUB_VOCAB_NS, NCX_NS, is_ns, is_xhtml, parse};

pub type TocMap = HashMap<String, String>;

/// Parse an EPUB3 navigation document (`<nav epub:type="toc">`).
pub fn parse_nav(bytes: &[u8], path: &str) -> Result<TocMap, EpubError> {
    let doc = parse(bytes, path)?;
    let root = doc.root_element();
    let base = path::dir(path);

    let mut nav_node = None;
    for n in root
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "nav")
    {
        let t = n.attribute((EPUB_VOCAB_NS, "type")).unwrap_or("");
        if t == "toc" {
            nav_node = Some(n);
            break;
        }
        nav_node.get_or_insert(n);
    }

    let mut map = TocMap::new();
    if let Some(nav) = nav_node {
        for a in nav.descendants().filter(|n| is_xhtml(n, "a")) {
            let title = a.text().map(str::to_owned);
            let href = a.attribute("href").map(str::to_owned);
            if let (Some(title), Some(href)) = (title, href) {
                let resolved = path::resolve(&base, &href);
                map.entry(resolved).or_insert(title);
            }
        }
    }
    Ok(map)
}

/// Parse an EPUB2 `NCX` navigation document.
pub fn parse_ncx(bytes: &[u8], path: &str) -> Result<TocMap, EpubError> {
    let doc = parse(bytes, path)?;
    let root = doc.root_element();
    let base = path::dir(path);

    let mut map = TocMap::new();
    for np in root.descendants().filter(|n| is_ns(n, NCX_NS, "navPoint")) {
        let title = np
            .descendants()
            .find(|n| is_ns(n, NCX_NS, "text"))
            .and_then(|n| n.text())
            .map(str::to_owned);
        let src = np
            .descendants()
            .find(|n| is_ns(n, NCX_NS, "content"))
            .and_then(|n| n.attribute("src"))
            .map(str::to_owned);
        if let (Some(title), Some(src)) = (title, src) {
            let resolved = path::resolve(&base, &src);
            map.entry(resolved).or_insert(title);
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAV: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml"
      xmlns:epub="http://www.idpf.org/2007/ops">
<head><title>TOC</title></head>
<body>
  <nav epub:type="toc">
    <ol>
      <li><a href="text/ch1.xhtml">Chapter 1</a></li>
      <li><a href="text/ch2.xhtml#s">Chapter 2</a></li>
    </ol>
  </nav>
</body>
</html>"#;

    const NCX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <navMap>
    <navPoint id="n1" playOrder="1">
      <navLabel><text>Chapter 1</text></navLabel>
      <content src="text/ch1.xhtml"/>
    </navPoint>
    <navPoint id="n2" playOrder="2">
      <navLabel><text>Chapter 2</text></navLabel>
      <content src="text/ch2.xhtml"/>
    </navPoint>
  </navMap>
</ncx>"#;

    #[test]
    fn parses_epub3_nav() {
        let map = parse_nav(NAV.as_bytes(), "OEBPS/nav.xhtml").expect("parse nav");
        assert_eq!(
            map.get("OEBPS/text/ch1.xhtml").map(String::as_str),
            Some("Chapter 1")
        );
        assert_eq!(
            map.get("OEBPS/text/ch2.xhtml").map(String::as_str),
            Some("Chapter 2")
        );
    }

    #[test]
    fn parses_epub2_ncx() {
        let map = parse_ncx(NCX.as_bytes(), "OEBPS/toc.ncx").expect("parse ncx");
        assert_eq!(
            map.get("OEBPS/text/ch1.xhtml").map(String::as_str),
            Some("Chapter 1")
        );
        assert_eq!(
            map.get("OEBPS/text/ch2.xhtml").map(String::as_str),
            Some("Chapter 2")
        );
    }
}
