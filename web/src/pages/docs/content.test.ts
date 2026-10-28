import { readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { docHref, headingSlugger, headings, PAGES, search } from './content';

const docsDir = fileURLToPath(new URL('../../../../docs', import.meta.url));

describe('docs', () => {
  it('lists every markdown file in the navigation', () => {
    const files = readdirSync(docsDir).filter((f) => f.endsWith('.md'));
    expect(PAGES.map((p) => `${p.slug}.md`).sort()).toEqual(files.sort());
  });

  it('starts every page with a title', () => {
    for (const page of PAGES) expect(page.markdown.startsWith(`# ${page.title}\n`), page.slug).toBe(true);
  });

  it('links only to pages and headings that exist', () => {
    const anchors = new Map(PAGES.map((p) => [p.slug, new Set(headings(p.markdown).map((h) => h.id))]));
    const all = new Map(
      PAGES.map((p) => {
        const slug = headingSlugger();
        const ids = [...p.markdown.replace(/```[\s\S]*?```/g, '').matchAll(/^#{1,6} (.+)$/gm)].map((m) => slug(m[1] ?? ''));
        return [p.slug, new Set(ids)];
      }),
    );
    for (const page of PAGES) {
      for (const [, href = ''] of page.markdown.matchAll(/\]\(([^)\s]+)\)/g)) {
        if (/^https?:|^\//.test(href)) continue;
        if (!href.startsWith('#')) expect(docHref(href), `${page.slug}: ${href}`).not.toBeNull();
        const [file = '', hash] = href.split('#');
        const slug = file ? file.replace(/^\.\//, '').replace(/\.md$/, '') : page.slug;
        expect(anchors.has(slug), `${page.slug}: ${href}`).toBe(true);
        if (hash) expect(all.get(slug)?.has(hash), `${page.slug}: ${href}`).toBe(true);
      }
    }
  });

  it('makes GitHub-style heading ids', () => {
    const slug = headingSlugger();
    expect(slug('Step 2: email')).toBe('step-2-email');
    expect(slug('API keys')).toBe('api-keys');
    expect(slug('API keys')).toBe('api-keys-1');
  });

  it('rewrites links between pages', () => {
    expect(docHref('api.md#limits')).toBe('/docs/api#limits');
    expect(docHref('./index.md')).toBe('/docs');
    expect(docHref('https://example.com/a.md')).toBeNull();
  });

  it('finds sections by their words', () => {
    const results = search('legal hold');
    expect(results[0]?.slug).toBe('retention');
    expect(search('bankid tic').some((r) => r.slug === 'identification')).toBe(true);
    expect(search('')).toEqual([]);
  });
});
