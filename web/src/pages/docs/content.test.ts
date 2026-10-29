import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { docHref, headingSlugger, headings, PAGES, search } from './content';

const docsDir = fileURLToPath(new URL('../../../../docs', import.meta.url));
const srcDir = fileURLToPath(new URL('../..', import.meta.url));

function headingIds() {
  return new Map(
    PAGES.map((p) => {
      const slug = headingSlugger();
      const ids = [...p.markdown.replace(/```[\s\S]*?```/g, '').matchAll(/^#{1,6} (.+)$/gm)].map((m) => slug(m[1] ?? ''));
      return [p.slug, new Set(ids)];
    }),
  );
}

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
    const all = headingIds();
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

  it('points every in-app help link at a page and heading that exist', () => {
    const all = headingIds();
    const files = readdirSync(srcDir, { recursive: true, encoding: 'utf8' }).filter((f) => f.endsWith('.tsx') && !f.includes('docs'));
    let count = 0;
    for (const file of files) {
      const source = readFileSync(`${srcDir}/${file}`, 'utf8');
      for (const m of source.matchAll(/\bhelp(?:=|: )["']([^"']+)["']|["'`]\/docs\/([a-z0-9#-]+)["'`]/g)) {
        const page = m[1] ?? m[2] ?? '';
        const [slug = '', hash] = page.split('#');
        expect(all.has(slug), `${file}: ${page}`).toBe(true);
        if (hash) expect(all.get(slug)?.has(hash), `${file}: ${page}`).toBe(true);
        count++;
      }
    }
    expect(count).toBeGreaterThan(10);
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
