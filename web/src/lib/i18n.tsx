import { Direction } from 'radix-ui';
import { createContext, Fragment, type ReactNode, useCallback, useContext, useEffect, useState } from 'react';

/** Languages the app is shown in, each in its own name. */
export const APP_LANGUAGES: [string, string][] = [
  ['en', 'English'],
  ['zh', '中文'],
  ['hi', 'हिन्दी'],
  ['es', 'Español'],
  ['fr', 'Français'],
  ['ar', 'العربية'],
  ['bn', 'বাংলা'],
  ['pt', 'Português'],
  ['ru', 'Русский'],
  ['ur', 'اردو'],
  ['sv', 'Svenska'],
];

export const isRtl = (code: string) => code === 'ar' || code === 'ur';

type Catalog = Record<string, string>;

const catalogs = import.meta.glob<Catalog>('../locales/*.json', { import: 'default' });
const STORAGE_KEY = 'opensign-language';

let current = 'en';
let catalog: Catalog = {};

/**
 * The text in the current language. Texts are keyed by their English wording, so a missing translation shows English.
 * `{name}` placeholders are filled from `values`.
 */
export function t(text: string, values?: Record<string, string | number>): string {
  const translated = catalog[text] || text;
  return values ? translated.replace(/\{(\w+)\}/g, (match, key: string) => (key in values ? String(values[key]) : match)) : translated;
}

/** Like `t`, with elements such as links or code in the placeholders. */
export function tx(text: string, parts: Record<string, ReactNode>): ReactNode[] {
  return (catalog[text] || text)
    .split(/\{(\w+)\}/)
    .map((piece, i) => (i % 2 ? <Fragment key={i}>{piece in parts ? parts[piece] : `{${piece}}`}</Fragment> : piece));
}

/** Marks a text for translation where it is defined; translate it with `t` where it is shown. */
export const msg = (text: string) => text;

/** A lookup table whose values read in the current language. */
export function labels(table: Record<string, string>): Record<string, string> {
  return new Proxy(table, { get: (target, key) => (typeof key === 'string' && key in target ? t(target[key]!) : undefined) });
}

export const currentLanguage = () => current;

function supported(code: string | null | undefined) {
  const base = code?.toLowerCase().split(/[-_]/)[0];
  return APP_LANGUAGES.some(([c]) => c === base) ? base! : null;
}

function initial() {
  try {
    const saved = supported(localStorage.getItem(STORAGE_KEY));
    if (saved) return saved;
  } catch {}
  for (const code of navigator.languages ?? [navigator.language]) {
    const found = supported(code);
    if (found) return found;
  }
  return 'en';
}

const I18nContext = createContext<{ language: string; setLanguage: (code: string) => void }>({
  language: 'en',
  setLanguage: () => {},
});

export function I18nProvider({ children }: { children: ReactNode }) {
  const [language, setLanguageState] = useState(initial);
  const [ready, setReady] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    const load = catalogs[`../locales/${language}.json`];
    (load ? load() : Promise.resolve({}))
      .catch(() => ({}))
      .then((loaded) => {
        if (!live) return;
        current = language;
        catalog = loaded;
        document.documentElement.lang = language;
        document.documentElement.dir = isRtl(language) ? 'rtl' : 'ltr';
        setReady(language);
      });
    return () => {
      live = false;
    };
  }, [language]);

  const setLanguage = useCallback((code: string) => {
    const next = supported(code) ?? 'en';
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {}
    setLanguageState(next);
  }, []);

  if (!ready) return null;
  return (
    <I18nContext.Provider value={{ language: ready, setLanguage }}>
      <Direction.Provider dir={isRtl(ready) ? 'rtl' : 'ltr'}>
        <Fragment key={ready}>{children}</Fragment>
      </Direction.Provider>
    </I18nContext.Provider>
  );
}

export const useLanguage = () => useContext(I18nContext);
