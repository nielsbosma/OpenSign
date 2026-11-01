import { msg } from '@/lib/i18n';

const sources = import.meta.glob<string>('../../../../docs/*.md', { query: '?raw', import: 'default', eager: true });
const translationSources = import.meta.glob<string>('../../../../docs/*/*.md', { query: '?raw', import: 'default' });

/** `anchors` is set on a translated page: the ids of the English headings, in order, so links keep working in every language. */
export type DocPage = { slug: string; title: string; markdown: string; group: string; anchors?: string[] };
export type Heading = { depth: 2 | 3; text: string; id: string };
export type SearchEntry = { slug: string; page: string; heading?: string; id?: string; text: string };

const NAV: { group: string; slugs: string[] }[] = [
  { group: msg('Getting Started'), slugs: ['index', 'quick-start', 'setup'] },
  {
    group: msg('Sending Documents'),
    slugs: [
      'documents',
      'text-tags',
      'templates',
      'share-links',
      'folders',
      'parties',
      'identification',
      'attachments',
      'in-person',
      'bulk-send',
      'data-export',
    ],
  },
  { group: msg('Signing'), slugs: ['signing-page', 'sealed-pdfs', 'verification'] },
  {
    group: msg('Administration'),
    slugs: ['users-and-roles', 'account-security', 'single-sign-on', 'settings', 'retention', 'audit-log', 'ai', 'automations'],
  },
  { group: msg('Developers'), slugs: ['api', 'webhooks', 'embedding', 'scrive', 'docusign', 'mcp'] },
  { group: msg('Operations'), slugs: ['deployment', 'updates', 'security'] },
];

function slugOf(path: string) {
  return path.slice(path.lastIndexOf('/') + 1).replace(/\.md$/, '');
}

function titleOf(markdown: string, slug: string) {
  return /^# (.+)$/m.exec(markdown)?.[1]?.trim() ?? slug;
}

const markdownBySlug = new Map(Object.entries(sources).map(([path, md]) => [slugOf(path), md]));

export const PAGES: DocPage[] = NAV.flatMap(({ group, slugs }) =>
  slugs.flatMap((slug) => {
    const markdown = markdownBySlug.get(slug);
    if (markdown === undefined) return [];
    return [{ slug, title: titleOf(markdown, slug), markdown, group }];
  }),
);

export function groupsOf(pages: DocPage[]) {
  return NAV.map(({ group }) => ({ group, pages: pages.filter((p) => p.group === group) }));
}

export function findPage(slug: string | undefined, pages = PAGES) {
  return pages.find((p) => p.slug === (slug || 'index'));
}

/** The pages in a language: its translations, and English for the pages it does not have. */
export function localizedPages(translations: Map<string, string>): DocPage[] {
  return PAGES.map((page) => {
    const markdown = translations.get(page.slug);
    if (markdown === undefined) return page;
    return { ...page, markdown, title: titleOf(markdown, page.slug), anchors: anchorIds(page.markdown) };
  });
}

/** Loads a language's pages only when someone reads them. A failed load leaves that language's pages in English. */
export async function loadPages(language: string): Promise<DocPage[]> {
  if (language === 'en') return PAGES;
  const folder = `/docs/${language}/`;
  const files = Object.entries(translationSources).filter(([path]) => path.includes(folder));
  try {
    const loaded = await Promise.all(files.map(async ([path, load]) => [slugOf(path), await load()] as const));
    return localizedPages(new Map(loaded));
  } catch {
    return PAGES;
  }
}

export function pagePath(slug: string) {
  return slug === 'index' ? '/docs' : `/docs/${slug}`;
}

/** Turns a link between markdown files (`api.md#limits`) into an app path; anything else is left alone. */
export function docHref(href: string) {
  const match = /^(?:\.\/)?([a-z0-9-]+)\.md(#[\w-]*)?$/.exec(href);
  return match?.[1] ? `${pagePath(match[1])}${match[2] ?? ''}` : null;
}

export function plainText(markdown: string) {
  return markdown
    .replace(/```[\s\S]*?```/g, ' ')
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/[`*_>|#]/g, ' ')
    .replace(/-{3,}|:-:/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/** GitHub-style heading ids, so `page.md#heading` links work the same here and on GitHub. */
export function headingSlugger() {
  const seen = new Map<string, number>();
  return (text: string) => {
    const base = text
      .toLowerCase()
      .trim()
      .replace(/[^\p{L}\p{N}\s_-]/gu, '')
      .replace(/\s/g, '-');
    const count = seen.get(base) ?? 0;
    seen.set(base, count + 1);
    return count ? `${base}-${count}` : base;
  };
}

function headingLines(markdown: string) {
  return [...markdown.replace(/```[\s\S]*?```/g, '').matchAll(/^(#{1,6}) (.+)$/gm)].map((m) => ({
    depth: m[1]?.length ?? 0,
    text: plainText(m[2] ?? ''),
  }));
}

/** The id of every heading on a page, in order, including the title. */
export function anchorIds(markdown: string) {
  const slug = headingSlugger();
  return headingLines(markdown).map((h) => slug(h.text));
}

export function headings(markdown: string, anchors = anchorIds(markdown)): Heading[] {
  const out: Heading[] = [];
  headingLines(markdown).forEach(({ depth, text }, i) => {
    const id = anchors[i] ?? '';
    if (depth === 2 || depth === 3) out.push({ depth, text, id });
  });
  return out;
}

function buildIndex(pages: DocPage[]): SearchEntry[] {
  const entries: SearchEntry[] = [];
  for (const page of pages) {
    const slug = headingSlugger();
    let headingCount = 0;
    let current: SearchEntry = { slug: page.slug, page: page.title, text: '' };
    const lines: string[] = [];
    const flush = () => {
      current.text = plainText(lines.join('\n'));
      entries.push(current);
      lines.length = 0;
    };
    let inCode = false;
    for (const line of page.markdown.split('\n')) {
      if (line.startsWith('```')) inCode = !inCode;
      const [, hashes = '', title = ''] = (!inCode && /^(#{1,6}) (.+)$/.exec(line)) || [];
      if (title) {
        const text = plainText(title);
        const generated = slug(text);
        const id = page.anchors?.[headingCount] ?? generated;
        headingCount++;
        if (hashes.length === 1) continue;
        flush();
        current = { slug: page.slug, page: page.title, heading: text, id, text: '' };
      } else {
        lines.push(line);
      }
    }
    flush();
  }
  return entries;
}

const indexes = new WeakMap<DocPage[], SearchEntry[]>();

export type SearchResult = SearchEntry & { snippet: string };

export function search(query: string, pages = PAGES, limit = 20): SearchResult[] {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!terms.length) return [];
  const index = indexes.get(pages) ?? buildIndex(pages);
  indexes.set(pages, index);
  const scored: { entry: SearchEntry; score: number }[] = [];
  for (const entry of index) {
    const title = `${entry.page} ${entry.heading ?? ''}`.toLowerCase();
    const body = entry.text.toLowerCase();
    let score = 0;
    for (const term of terms) {
      const inTitle = title.includes(term);
      const inBody = body.includes(term);
      if (!inTitle && !inBody) {
        score = 0;
        break;
      }
      score += (inTitle ? 10 : 0) + (inBody ? 1 : 0);
    }
    if (score) scored.push({ entry, score: score + (entry.heading ? 0 : 1) });
  }
  return scored
    .sort((a, b) => b.score - a.score)
    .slice(0, limit)
    .map(({ entry }) => ({ ...entry, snippet: snippet(entry.text, terms) }));
}

function snippet(text: string, terms: string[]) {
  const lower = text.toLowerCase();
  const at = Math.min(...terms.map((t) => lower.indexOf(t)).filter((i) => i >= 0));
  if (!Number.isFinite(at)) return text.slice(0, 140);
  const start = Math.max(0, at - 50);
  return `${start > 0 ? '…' : ''}${text.slice(start, start + 160)}${start + 160 < text.length ? '…' : ''}`;
}
