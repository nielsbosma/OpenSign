use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;

use crate::context::Ctx;
use crate::docs::{self, Section};
use crate::error::{AppError, ErrorKind, Result, bad_request};
use crate::service::{Access, Meta, Service};

fn url(ctx: &Ctx, slug: &str, anchor: &str) -> String {
    let base = ctx.app.public_url();
    if anchor.is_empty() { format!("{base}/docs/{slug}") } else { format!("{base}/docs/{slug}#{anchor}") }
}

#[derive(Deserialize, JsonSchema)]
pub struct SearchInput {
    /// Words to look for, such as "bankid signing order" or "webhook retries".
    pub query: String,
    /// At most this many results, 1 to 20. Defaults to 8.
    pub limit: Option<usize>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub slug: String,
    pub page: String,
    pub heading: String,
    /// Pass with the slug to docs.read for just this section.
    pub anchor: String,
    pub url: String,
    pub snippet: String,
}

fn snippet(text: &str, terms: &[String]) -> String {
    let lower = text.to_lowercase();
    let at = terms.iter().filter_map(|t| lower.find(t.as_str())).min().unwrap_or(0);
    // Lowercasing can move byte offsets, so step back to a boundary in the original.
    let mut start = at.saturating_sub(60).min(text.len());
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    let body: String = text[start..].chars().take(200).collect();
    let more = start + body.len() < text.len();
    format!("{}{body}{}", if start > 0 { "…" } else { "" }, if more { "…" } else { "" })
}

pub struct Search;

#[async_trait]
impl Service for Search {
    type Input = SearchInput;
    type Output = Vec<SearchHit>;
    const META: Meta = Meta::query(
        "docs.search",
        "Search OpenSign's documentation: how features work, settings, roles, signing, identification, the API. Use it before acting when unsure how something works, then read the best hit with docs.read.",
        Access::Authenticated,
    )
    .tool();

    async fn run(ctx: &Ctx, _conn: &mut PgConnection, input: SearchInput) -> Result<Vec<SearchHit>> {
        let terms: Vec<String> = input.query.to_lowercase().split_whitespace().take(12).map(str::to_string).collect();
        if terms.is_empty() {
            return Err(bad_request("Enter something to search for"));
        }
        let limit = input.limit.unwrap_or(8).clamp(1, 20);
        let mut scored: Vec<(usize, usize, Section, String)> = Vec::new();
        for page in docs::pages() {
            for section in docs::sections(&page) {
                let title = format!("{} {}", section.page, section.heading).to_lowercase();
                let text = docs::plain_text(&section.markdown);
                let body = text.to_lowercase();
                let (mut matched, mut score) = (0, 0);
                for term in &terms {
                    let (in_title, in_body) = (title.contains(term.as_str()), body.contains(term.as_str()));
                    if in_title || in_body {
                        matched += 1;
                        score += if in_title { 10 } else { 0 } + body.matches(term.as_str()).count().min(5);
                    }
                }
                if matched > 0 {
                    scored.push((matched, score, section, text));
                }
            }
        }
        scored.sort_by_key(|s| std::cmp::Reverse((s.0, s.1)));
        Ok(scored
            .into_iter()
            .take(limit)
            .map(|(_, _, s, text)| SearchHit {
                url: url(ctx, s.slug, &s.anchor),
                slug: s.slug.to_string(),
                page: s.page.to_string(),
                heading: s.heading,
                anchor: s.anchor,
                snippet: snippet(&text, &terms),
            })
            .collect())
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct ReadInput {
    /// The page, such as "identification". Leave empty for the list of pages.
    pub slug: Option<String>,
    /// A heading's anchor from docs.search, to read one section instead of the whole page.
    pub anchor: Option<String>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PageSummary {
    pub slug: String,
    pub title: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReadOutput {
    pub slug: Option<String>,
    pub title: Option<String>,
    pub url: Option<String>,
    /// The page or section as Markdown. Links like `api.md#api-keys` point at other pages and anchors.
    pub markdown: Option<String>,
    /// Every page, when no slug was given.
    pub pages: Vec<PageSummary>,
}

pub struct Read;

#[async_trait]
impl Service for Read {
    type Input = ReadInput;
    type Output = ReadOutput;
    const META: Meta = Meta::query(
        "docs.read",
        "Read a page of OpenSign's documentation as Markdown, or one section of it. Without a slug, lists every page.",
        Access::Authenticated,
    )
    .tool();

    async fn run(ctx: &Ctx, _conn: &mut PgConnection, input: ReadInput) -> Result<ReadOutput> {
        let Some(slug) = input.slug.filter(|s| !s.trim().is_empty()) else {
            let pages = docs::pages().map(|p| PageSummary { slug: p.slug.into(), title: p.title.into() }).collect();
            return Ok(ReadOutput { slug: None, title: None, url: None, markdown: None, pages });
        };
        let slug = slug.trim().trim_end_matches(".md");
        let page = docs::page(slug)
            .ok_or_else(|| AppError::new(ErrorKind::NotFound, "No such page. Call docs.read without a slug for the list."))?;
        let anchor = input.anchor.unwrap_or_default();
        let anchor = anchor.trim_start_matches('#');
        let markdown = if anchor.is_empty() {
            page.markdown.to_string()
        } else {
            docs::section(&page, anchor).ok_or_else(|| AppError::new(ErrorKind::NotFound, "No such section on that page"))?
        };
        Ok(ReadOutput {
            url: Some(url(ctx, page.slug, anchor)),
            slug: Some(page.slug.into()),
            title: Some(page.title.into()),
            markdown: Some(markdown),
            pages: Vec::new(),
        })
    }
}
