import { describe, expect, it } from 'vitest';
import { APP_LANGUAGES } from '@/lib/i18n';

const sources = import.meta.glob<string>(['../**/*.{ts,tsx}', '!../pages/sign/**', '!../**/*.test.ts'], {
  query: '?raw',
  import: 'default',
  eager: true,
});
const catalogs = import.meta.glob<Record<string, string>>('./*.json', { import: 'default', eager: true });

const CALL = /(?<![\w.])(?:t|tx|msg)\(\s*('(?:[^'\\]|\\.)*'|"(?:[^"\\]|\\.)*"|`[^`$]*`)/g;

function keys() {
  const found = new Set<string>();
  for (const source of Object.values(sources)) {
    for (const [, literal] of source.matchAll(CALL)) {
      const body = literal!.slice(1, -1);
      found.add(literal!.startsWith('"') ? JSON.parse(literal!) : body.replace(/\\(.)/g, '$1'));
    }
  }
  return found;
}

const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

describe('translations', () => {
  const texts = keys();

  it('finds the texts', () => {
    expect(texts.size).toBeGreaterThan(300);
    expect(texts.has('Send for Signing')).toBe(true);
  });

  for (const [code] of APP_LANGUAGES.filter(([c]) => c !== 'en')) {
    it(`has every text in ${code} with the same placeholders`, () => {
      const catalog = catalogs[`./${code}.json`];
      expect(catalog, `src/locales/${code}.json`).toBeDefined();
      const missing = [...texts].filter((text) => !catalog![text]?.trim());
      const stale = Object.keys(catalog!).filter((text) => !texts.has(text));
      const broken = [...texts].filter((text) => catalog![text] && placeholders(catalog![text]!).join() !== placeholders(text).join());
      expect({ missing, stale, broken }).toEqual({ missing: [], stale: [], broken: [] });
    });
  }
});
