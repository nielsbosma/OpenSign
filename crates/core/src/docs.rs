//! The public documentation from `docs/*.md`, built into the binary for the docs tools.

mod pages {
    include!(concat!(env!("OUT_DIR"), "/docs.rs"));
}

pub struct Page {
    pub slug: &'static str,
    pub title: &'static str,
    pub markdown: &'static str,
}

pub struct Section {
    pub slug: &'static str,
    pub page: &'static str,
    /// Empty for the text before the first heading.
    pub heading: String,
    pub anchor: String,
    pub markdown: String,
}

pub fn pages() -> impl Iterator<Item = Page> {
    pages::PAGES.iter().map(|(slug, markdown)| Page {
        slug,
        title: markdown.lines().next().and_then(|l| l.strip_prefix("# ")).unwrap_or(slug),
        markdown,
    })
}

pub fn page(slug: &str) -> Option<Page> {
    pages().find(|p| p.slug == slug)
}

/// Markdown reduced to words, for matching and snippets.
pub fn plain_text(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut in_code = false;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                // Keep a link's text, drop its target.
                ']' if chars.peek() == Some(&'(') => {
                    for c in chars.by_ref() {
                        if c == ')' {
                            break;
                        }
                    }
                }
                '`' | '*' | '_' | '>' | '|' | '#' | '[' | '!' => out.push(' '),
                c => out.push(c),
            }
        }
        out.push(' ');
    }
    out.replace("---", " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// GitHub-style heading ids, the same as the web app's, so `page.md#heading` links match.
pub fn slugger() -> impl FnMut(&str) -> String {
    let mut seen = std::collections::HashMap::<String, usize>::new();
    move |text: &str| {
        let base: String = text
            .trim()
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '_' || *c == '-')
            .map(|c| if c.is_whitespace() { '-' } else { c })
            .collect();
        let count = seen.entry(base.clone()).or_default();
        *count += 1;
        if *count == 1 { base } else { format!("{base}-{}", *count - 1) }
    }
}

/// Each page split at its `##` and `###` headings.
pub fn sections(page: &Page) -> Vec<Section> {
    let mut slug = slugger();
    let mut out =
        vec![Section { slug: page.slug, page: page.title, heading: String::new(), anchor: String::new(), markdown: String::new() }];
    let mut in_code = false;
    for line in page.markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
        }
        let heading = (!in_code).then(|| line.split_once(' ')).flatten().filter(|(h, _)| !h.is_empty() && h.chars().all(|c| c == '#'));
        if let Some((hashes, title)) = heading {
            let text = plain_text(title);
            let anchor = slug(&text);
            if hashes.len() > 1 {
                out.push(Section { slug: page.slug, page: page.title, heading: text, anchor, markdown: format!("{line}\n") });
            }
            continue;
        }
        if let Some(last) = out.last_mut() {
            last.markdown.push_str(line);
            last.markdown.push('\n');
        }
    }
    out
}

/// The section with this anchor and the deeper sections under it.
pub fn section(page: &Page, anchor: &str) -> Option<String> {
    let all = sections(page);
    let start = all.iter().position(|s| s.anchor == anchor && !s.heading.is_empty())?;
    let depth = |s: &Section| s.markdown.chars().take_while(|c| *c == '#').count();
    let level = depth(&all[start]);
    let mut out = all[start].markdown.clone();
    for s in &all[start + 1..] {
        if depth(s) <= level {
            break;
        }
        out.push_str(&s.markdown);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_are_embedded_with_titles() {
        let mcp = page("mcp").expect("mcp page");
        assert_eq!(mcp.title, "MCP and Claude");
        assert!(pages().count() > 20);
    }

    #[test]
    fn anchors_match_the_web_app() {
        let mut slug = slugger();
        assert_eq!(slug("Step 2: email"), "step-2-email");
        assert_eq!(slug("API keys"), "api-keys");
        assert_eq!(slug("API keys"), "api-keys-1");
        assert_eq!(plain_text("See [the API](api.md#api-keys) and `osk_`."), "See the API and osk .");
    }

    #[test]
    fn reads_a_section_with_its_subsections() {
        let api = page("api").expect("api page");
        let keys = section(&api, "api-keys").expect("section");
        assert!(keys.starts_with("## API keys\n"));
        assert!(keys.contains("### Scopes"));
        assert!(!keys.contains("## API Explorer"));
    }
}
