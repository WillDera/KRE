//! Optional front matter (`---` … `---`) for title / author / language.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrontMatter {
    pub title: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
}

/// Split optional front matter from the markdown body.
pub fn split_front_matter(input: &str) -> (FrontMatter, &str) {
    let trimmed = input.trim_start_matches('\u{feff}');
    if !trimmed.starts_with("---") {
        return (FrontMatter::default(), input);
    }
    let after = &trimmed[3..];
    let after = after.strip_prefix('\n').or_else(|| after.strip_prefix("\r\n")).unwrap_or(after);
    let Some(end) = after.find("\n---") else {
        return (FrontMatter::default(), input);
    };
    let block = &after[..end];
    let rest = &after[end + 4..];
    let rest = rest.trim_start_matches(['\r', '\n']);
    (parse_simple_yaml(block), rest)
}

fn parse_simple_yaml(block: &str) -> FrontMatter {
    let mut fm = FrontMatter::default();
    for line in block.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().trim_matches('"').trim_matches('\'').to_owned();
        if value.is_empty() {
            continue;
        }
        match key.as_str() {
            "title" => fm.title = Some(value),
            "author" => fm.author = Some(value),
            "language" | "lang" => fm.language = Some(value),
            _ => {}
        }
    }
    fm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_front_matter() {
        let src = "---\ntitle: The Long Dark\nauthor: Big Dog\nlang: en\n---\n\n# Ch1\n\nHello.\n";
        let (fm, body) = split_front_matter(src);
        assert_eq!(fm.title.as_deref(), Some("The Long Dark"));
        assert_eq!(fm.author.as_deref(), Some("Big Dog"));
        assert_eq!(fm.language.as_deref(), Some("en"));
        assert!(body.starts_with("# Ch1"));
    }

    #[test]
    fn no_front_matter_returns_whole() {
        let src = "# Title\n\nBody.";
        let (fm, body) = split_front_matter(src);
        assert_eq!(fm, FrontMatter::default());
        assert_eq!(body, src);
    }
}
