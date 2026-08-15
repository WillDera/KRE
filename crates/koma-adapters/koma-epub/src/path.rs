//! EPUB-internal path resolution (ZIP entries use `/` separators).

/// Normalize a zip-internal path: drop query/fragment, resolve `.` and `..`,
/// strip empty segments.
pub fn normalize(path: &str) -> String {
    let clean = path.split(['?', '#']).next().unwrap_or(path);
    let mut parts: Vec<&str> = Vec::new();
    for seg in clean.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// Directory part of a path ("" when there is none).
pub fn dir(path: &str) -> String {
    match path.rfind('/') {
        Some(i) => path[..i].to_string(),
        None => String::new(),
    }
}

/// Resolve `href` (which may be relative to `base_dir`) to a normalized
/// zip-internal path. Fragments (`#section`) are dropped.
pub fn resolve(base_dir: &str, href: &str) -> String {
    if let Some(stripped) = href.strip_prefix('/') {
        return normalize(stripped);
    }
    if base_dir.is_empty() {
        return normalize(href);
    }
    normalize(&format!("{}/{}", base_dir, href))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_handles_relative_and_parent_segments() {
        assert_eq!(resolve("OEBPS", "ch1.xhtml"), "OEBPS/ch1.xhtml");
        assert_eq!(
            resolve("OEBPS/text", "../Images/pic.png"),
            "OEBPS/Images/pic.png"
        );
        assert_eq!(resolve("OEBPS", "ch1.xhtml#sec-2"), "OEBPS/ch1.xhtml");
        assert_eq!(resolve("", "ch1.xhtml"), "ch1.xhtml");
        assert_eq!(
            resolve("OEBPS", "/absolute/path.xhtml"),
            "absolute/path.xhtml"
        );
        assert_eq!(dir("OEBPS/text/ch1.xhtml"), "OEBPS/text");
        assert_eq!(dir("ch1.xhtml"), "");
    }
}
