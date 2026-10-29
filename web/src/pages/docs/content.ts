const sources = import.meta.glob<string>('../../../../docs/*.md', { query: '?raw', import: 'default', eager: true });

export type DocPage = { slug: string; title: string; markdown: string; group: string };
export type Heading = { depth: 2 | 3; text: string; id: string };
export type SearchEntry = { slug: string; page: string; heading?: string; id?: string; text: string };

const NAV: { group: string; slugs: string[] }[] = [
  { group: 'Getting Started', slugs: ['index', 'quick-start', 'setup'] },
  {
    group: 'Sending Documents',
    slugs: [
      'documents',
      'text-tags',
      'templates',
      'folders',
      'parties',
      'identification',
      'attachments',
      'in-person',
      'bulk-send',
      'data-export',
    ],
  },
  { group: 'Signing', slugs: ['signing-page', 'sealed-pdfs', 'verification'] },
  { group: 'Administration', slugs: ['users-and-roles', 'account-security', 'settings', 'retention', 'audit-log', 'ai'] },
  { group: 'Developers', slugs: ['api', 'webhooks', 'embedding', 'scrive', 'docusign', 'mcp'] },
  { group: 'Operations', slugs: ['deployment', 'updates', 'security'] },
];

function slugOf(path: string) {
  return path.slice(path.lastIndexOf('/') + 1).replace(/\.md$/, '');
}

const markdownBySlug = new Map(Object.entries(sources).map(([path, md]) => [slugOf(path), md]));

export const PAGES: DocPage[] = NAV.flatMap(({ group, slugs }) =>
  slugs.flatMap((slug) => {
    const markdown = markdownBySlug.get(slug);
    if (markdown === undefined) return [];
    const title = /^# (.+)$/m.exec(markdown)?.[1]?.trim() ?? slug;
    return [{ slug, title, markdown, group }];
  }),
);

export const GROUPS = NAV.map(({ group }) => ({ group, pages: PAGES.filter((p) => p.group === group) }));

export function findPage(slug: string | undefined) {
  return PAGES.find((p) => p.slug === (slug || 'index'));
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

export function headings(markdown: string): Heading[] {
  const slug = headingSlugger();
  const out: Heading[] = [];
  for (const line of markdown.replace(/```[\s\S]*?```/g, '').split('\n')) {
    const [, hashes = '', title = ''] = /^(#{1,6}) (.+)$/.exec(line) ?? [];
    if (!title) continue;
    const text = plainText(title);
    const id = slug(text);
    if (hashes.length === 2 || hashes.length === 3) out.push({ depth: hashes.length as 2 | 3, text, id });
  }
  return out;
}

function buildIndex(): SearchEntry[] {
  const entries: SearchEntry[] = [];
  for (const page of PAGES) {
    const slug = headingSlugger();
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
        const id = slug(text);
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

let index: SearchEntry[] | null = null;

export type SearchResult = SearchEntry & { snippet: string };

export function search(query: string, limit = 20): SearchResult[] {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!terms.length) return [];
  index ??= buildIndex();
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
