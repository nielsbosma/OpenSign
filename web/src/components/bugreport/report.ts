import { matchPath } from 'react-router';

const REMOVED = '[removed]';

/** The same rules as `scrub` in crates/core/src/services/bugreports.rs, so the review shows what is sent. */
const RULES = [
  /\b[A-Za-z][A-Za-z0-9+.-]*:\/\/\S+/g,
  /[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/g,
  /eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]*)?/g,
  /\b(?:bearer|basic|token)\s+\S+/gi,
  /\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b/gi,
  /\S*\?\S*=\S*/g,
  /\b[0-9a-fA-F]{24,}\b/g,
];

export function scrub(text: string, host = location.hostname): string {
  let out = host.length > 2 ? text.split(host).join(REMOVED) : text;
  for (const rule of RULES) out = out.replace(rule, REMOVED);
  out = out.replace(/[A-Za-z0-9+/_-]{24,}={0,2}/g, (run) => (/\d/.test(run) && /[A-Za-z]/.test(run) ? REMOVED : run));
  out = out.replace(/\+?\d[\d -]{8,}\d/g, (run) => (run.replace(/\D/g, '').length >= 10 && !/\d{4}-\d{2}-\d{2}/.test(run) ? REMOVED : run));
  // biome-ignore lint/suspicious/noControlCharactersInRegex: control characters are what is removed
  return out.replace(/[\u0000-\u0008\u000b-\u001f\u007f]/g, '');
}

/** Every page in the signed-in app, so a report names `/documents/:id` and never the real path. */
const ROUTES = [
  '/documents/:id',
  '/documents',
  '/templates',
  '/bulk/:id',
  '/settings/webhooks/:id',
  '/settings/automations/:id',
  '/settings/admin/:group',
  '/settings/:section',
  '/settings',
  '/',
];

export function routePattern(pathname: string): string {
  return ROUTES.find((pattern) => matchPath(pattern, pathname)) ?? '/*';
}

/** Name, major version and system, such as `Firefox 146 (Windows)`. Never the full user agent. */
export function browserName(ua = navigator.userAgent): string {
  const found =
    [
      ['Edge', /Edg\/(\d+)/],
      ['Opera', /OPR\/(\d+)/],
      ['Firefox', /Firefox\/(\d+)/],
      ['Chrome', /Chrome\/(\d+)/],
      ['Safari', /Version\/(\d+).*Safari/],
    ]
      .map(([name, pattern]) => [name, (pattern as RegExp).exec(ua)?.[1]] as const)
      .find(([, version]) => version) ?? (['Browser', ''] as const);
  const system = /Android/.test(ua)
    ? 'Android'
    : /iPhone|iPad|iPod/.test(ua)
      ? 'iOS'
      : /Windows/.test(ua)
        ? 'Windows'
        : /Mac OS X|Macintosh/.test(ua)
          ? 'macOS'
          : /CrOS/.test(ua)
            ? 'ChromeOS'
            : /Linux/.test(ua)
              ? 'Linux'
              : '';
  return `${found[0]}${found[1] ? ` ${found[1]}` : ''}${system ? ` (${system})` : ''}`;
}

export type Box = { x: number; y: number; width: number; height: number };

const TEXT_INPUT =
  'input:not([type=checkbox]):not([type=radio]):not([type=button]):not([type=submit]):not([type=range]):not([type=color]):not([type=hidden]), textarea, select, [contenteditable]:not([contenteditable=false])';

/** Where the page shows something private: typed values and everything marked `data-private`. */
export function privateBoxes(exclude: Element | null): Box[] {
  const boxes: Box[] = [];
  for (const el of document.querySelectorAll(`${TEXT_INPUT}, [data-private]`)) {
    if (exclude?.contains(el)) continue;
    // Inside a closed section, an element still has a box but shows nothing.
    if (el.checkVisibility?.({ opacityProperty: true, visibilityProperty: true }) === false) continue;
    const r = el.getBoundingClientRect();
    const x = Math.max(0, r.left);
    const y = Math.max(0, r.top);
    const width = Math.min(window.innerWidth, r.right) - x;
    const height = Math.min(window.innerHeight, r.bottom) - y;
    if (width > 0 && height > 0) boxes.push({ x, y, width, height });
  }
  return boxes;
}

/** Inside a private element or a typed field, an element's text is not used as its label. */
export function isPrivate(el: Element): boolean {
  return !!el.closest(`[data-private], ${TEXT_INPUT}`);
}

export function openDialogTitles(): string[] {
  return Array.from(document.querySelectorAll('[role="dialog"][data-state="open"], [role="alertdialog"][data-state="open"]'))
    .map((d) => {
      const labelledBy = d.getAttribute('aria-labelledby');
      const title = labelledBy ? document.getElementById(labelledBy) : null;
      if (title && isPrivate(title)) return '';
      return clip(title?.textContent ?? d.getAttribute('aria-label') ?? '', 120);
    })
    .filter(Boolean)
    .slice(0, 10);
}

export function clip(text: string, max: number) {
  const t = text.replace(/\s+/g, ' ').trim();
  return t.length > max ? `${t.slice(0, max - 1)}…` : t;
}
