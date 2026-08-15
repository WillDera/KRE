//! EPUB/XML namespace constants and parsing helpers.

use std::borrow::Cow;

use crate::EpubError;

pub const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";
pub const OPF_NS: &str = "http://www.idpf.org/2007/opf";
pub const DC_NS: &str = "http://purl.org/dc/elements/1.1/";
pub const NCX_NS: &str = "http://www.daisy.org/z3986/2005/ncx/";
pub const CONTAINER_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:container";
pub const EPUB_VOCAB_NS: &str = "http://www.idpf.org/epub/vocab/structure/#";

/// Parse a UTF-8 XML document, tagging errors with the source path.
///
/// roxmltree rejects DOCTYPE declarations, which EPUB2 XHTML commonly
/// carries; they are stripped here. The cleaned source string is
/// intentionally leaked so the returned `Document<'static>` can borrow it —
/// this adapter is compiler-side and each call's source is bounded by a
/// single file, which is dropped after block extraction.
pub fn parse(bytes: &[u8], path: &str) -> Result<roxmltree::Document<'static>, EpubError> {
    let raw = std::str::from_utf8(bytes)
        .map_err(|e| EpubError::Xml(path.to_string(), format!("utf-8: {e}")))?;
    let cleaned = strip_doctype(raw);
    let source: &'static str = Box::leak(cleaned.into_owned().into_boxed_str());
    roxmltree::Document::parse(source).map_err(|e| EpubError::Xml(path.to_string(), e.to_string()))
}

/// Remove a `<!DOCTYPE ...>` declaration (including an internal subset) so
/// the string can be parsed by roxmltree.
fn strip_doctype(s: &str) -> Cow<'_, str> {
    let Some(start) = s.find("<!DOCTYPE").or_else(|| s.find("<!doctype")) else {
        return Cow::Borrowed(s);
    };
    let after = &s[start + 9..];
    let end = if let Some(b) = after.find('[') {
        let rest = &after[b + 1..];
        match rest.find("]>") {
            Some(close) => start + 9 + b + 1 + close + 2,
            None => return Cow::Borrowed(s),
        }
    } else {
        match after.find('>') {
            Some(gt) => start + 9 + gt + 1,
            None => return Cow::Borrowed(s),
        }
    };
    let mut out = String::with_capacity(s.len());
    out.push_str(&s[..start]);
    out.push_str(&s[end..]);
    Cow::Owned(out)
}

/// True if `node` is an element with the given name in the XHTML namespace
/// (or, permissively, without a namespace).
pub fn is_xhtml(node: &roxmltree::Node, name: &str) -> bool {
    if !node.is_element() {
        return false;
    }
    let tag = node.tag_name();
    tag.name() == name && matches!(tag.namespace(), Some(XHTML_NS) | None)
}

/// True if `node` is an element with the given name in the given namespace.
pub fn is_ns(node: &roxmltree::Node, ns: &str, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name && node.tag_name().namespace() == Some(ns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_doctype() {
        let s = r#"<?xml version="1.0"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml"><body><p>x</p></body></html>"#;
        let cleaned = strip_doctype(s);
        assert!(!cleaned.contains("DOCTYPE"));
        let doc = parse(s.as_bytes(), "t.xhtml").expect("parse with doctype");
        assert_eq!(doc.root_element().tag_name().name(), "html");
    }

    #[test]
    fn strips_doctype_with_internal_subset() {
        let s = "<!DOCTYPE html [ <!ENTITY x \"y\"> ]><html><body><p>x</p></body></html>";
        let doc = parse(s.as_bytes(), "t.xhtml").expect("parse with subset");
        assert_eq!(doc.root_element().tag_name().name(), "html");
    }
}
