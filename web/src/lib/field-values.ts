import type { FieldKind, Validation } from './api';

/**
 * What a field may hold and how its value is written on the page, the same rules as
 * `crates/core/src/documents/values.rs`. Both run `field-values.cases.json`. The server checks again on submit, and
 * alone runs a field's pattern: patterns come from the sender, and a browser's regular expressions can be made to hang.
 */

export type Invalid =
  | { code: 'number' | 'phone' | 'date' | 'option' }
  | { code: 'tooShort' | 'tooLong'; n: number }
  | { code: 'below' | 'above'; limit: string };

export type Spec = { options?: string[] | null; validation?: Validation | null; format?: string | null };

export type Checked = { ok: true; value: string | null } | { ok: false; error: Invalid };

export const DATE_FORMATS = ['YYYY-MM-DD', 'DD/MM/YYYY', 'MM/DD/YYYY', 'DD.MM.YYYY', 'D MMM YYYY'] as const;

/** Number formats offered in the editor, written as 1234 would look. Any other such example works too. */
export const NUMBER_FORMATS = [
  '1234',
  '1234.00',
  '1,234.00',
  '1 234,00',
  '1.234,00',
  '$1,234.00',
  '€1.234,00',
  '1 234,00 kr',
  '1.234,00 €',
];

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
const MAX_DIGITS = 15;
const GROUP_CHARS = [',', '.', ' ', ' ', ' ', "'"];

const TEXT_KINDS: FieldKind[] = ['text', 'initials', 'company'];
export const takesPattern = (kind: FieldKind) => TEXT_KINDS.includes(kind);

const fail = (error: Invalid): Checked => ({ ok: false, error });

/** A value in its stored form: numbers as plain decimals, phones in E.164, dates as YYYY-MM-DD, several choices as JSON. */
export function normalize(kind: FieldKind, spec: Spec, input: string): Checked {
  const raw = input.trim();
  if (!raw || raw === '[]') return { ok: true, value: null };
  const v = spec.validation ?? {};
  const options = spec.options ?? [];
  switch (kind) {
    case 'number': {
      const format = spec.format ? parseNumberFormat(spec.format) : null;
      const canonical = canonicalNumber(raw, format);
      const n = canonical === null ? null : numberValue(canonical);
      if (canonical === null || n === null) return fail({ code: 'number' });
      const min = v.min != null ? numberValue(v.min) : null;
      if (min !== null && n < min) return fail({ code: 'below', limit: v.min! });
      const max = v.max != null ? numberValue(v.max) : null;
      if (max !== null && n > max) return fail({ code: 'above', limit: v.max! });
      return { ok: true, value: canonical };
    }
    case 'phone': {
      const value = phone(raw);
      return value ? { ok: true, value } : fail({ code: 'phone' });
    }
    case 'date_input': {
      const date = parseDate(raw, spec.format) ?? null;
      if (!date) return fail({ code: 'date' });
      if (v.min && isoDate(v.min) && date < v.min) return fail({ code: 'below', limit: v.min });
      if (v.max && isoDate(v.max) && date > v.max) return fail({ code: 'above', limit: v.max });
      return { ok: true, value: date };
    }
    case 'select':
    case 'radio': {
      const found = option(options, raw);
      return found === undefined ? fail({ code: 'option' }) : { ok: true, value: found };
    }
    case 'multiselect': {
      let picked: unknown;
      if (raw.startsWith('[')) {
        try {
          picked = JSON.parse(raw);
        } catch {
          return fail({ code: 'option' });
        }
        if (!Array.isArray(picked) || picked.some((p) => typeof p !== 'string')) return fail({ code: 'option' });
      } else picked = raw.split(/[,;|]/);
      const chosen: string[] = [];
      for (const p of (picked as string[]).map((p) => p.trim()).filter(Boolean)) {
        const found = option(options, p);
        if (found === undefined) return fail({ code: 'option' });
        chosen.push(found);
      }
      const ordered = options.filter((o) => chosen.includes(o));
      return { ok: true, value: ordered.length ? JSON.stringify(ordered) : null };
    }
    default: {
      if (takesPattern(kind)) {
        const length = [...raw].length;
        if (v.minLength != null && length < v.minLength) return fail({ code: 'tooShort', n: v.minLength });
        if (v.maxLength != null && length > v.maxLength) return fail({ code: 'tooLong', n: v.maxLength });
      }
      return { ok: true, value: raw };
    }
  }
}

function option(options: string[], value: string) {
  return options.find((o) => o === value) ?? options.find((o) => o.toLowerCase() === value.toLowerCase());
}

function phone(raw: string) {
  const compact = raw.replace(/[ \-()./ ]/g, '');
  const digits = compact.startsWith('+') ? compact.slice(1) : compact.startsWith('00') ? compact.slice(2) : null;
  if (digits === null || !/^[1-9][0-9]{6,14}$/.test(digits)) return null;
  return `+${digits}`;
}

const isoDate = (s: string) => /^\d{4}-\d{2}-\d{2}$/.test(s) && realDate(+s.slice(0, 4), +s.slice(5, 7), +s.slice(8, 10)) !== null;

function realDate(y: number, m: number, d: number) {
  const date = new Date(Date.UTC(y, m - 1, d));
  if (date.getUTCFullYear() !== y || date.getUTCMonth() !== m - 1 || date.getUTCDate() !== d) return null;
  return `${String(y).padStart(4, '0')}-${String(m).padStart(2, '0')}-${String(d).padStart(2, '0')}`;
}

function parseDate(raw: string, format?: string | null): string | null {
  let m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(raw);
  if (m) return realDate(+m[1]!, +m[2]!, +m[3]!);
  switch (format) {
    case 'DD/MM/YYYY':
      m = /^(\d{1,2})\/(\d{1,2})\/(\d{4})$/.exec(raw);
      return m ? realDate(+m[3]!, +m[2]!, +m[1]!) : null;
    case 'MM/DD/YYYY':
      m = /^(\d{1,2})\/(\d{1,2})\/(\d{4})$/.exec(raw);
      return m ? realDate(+m[3]!, +m[1]!, +m[2]!) : null;
    case 'DD.MM.YYYY':
      m = /^(\d{1,2})\.(\d{1,2})\.(\d{4})$/.exec(raw);
      return m ? realDate(+m[3]!, +m[2]!, +m[1]!) : null;
    case 'D MMM YYYY': {
      m = /^(\d{1,2}) ([A-Za-z]{3}) (\d{4})$/.exec(raw);
      const month = m ? MONTHS.findIndex((x) => x.toLowerCase() === m![2]!.toLowerCase()) : -1;
      return m && month >= 0 ? realDate(+m[3]!, month + 1, +m[1]!) : null;
    }
    default:
      return null;
  }
}

export function numberValue(canonical: string): number | null {
  const clean = canonical.trim();
  const unsigned = clean.replace(/^-+/, '');
  if (!clean || !/^[0-9.]+$/.test(unsigned) || unsigned.split('.').length > 2) return null;
  if ((clean.match(/[0-9]/g) ?? []).length > MAX_DIGITS) return null;
  const n = Number(clean);
  return Number.isFinite(n) ? n : null;
}

type NumberFormat = { prefix: string; group: string | null; decimal: string; decimals: number; suffix: string };

export function parseNumberFormat(format: string): NumberFormat | null {
  const start = format.indexOf('1');
  if (start < 0) return null;
  const prefix = format.slice(0, start);
  let rest = format.slice(start + 1);
  let group: string | null = null;
  if (GROUP_CHARS.includes(rest[0] ?? '')) {
    group = rest[0]!;
    rest = rest.slice(1);
  }
  if (!rest.startsWith('234')) return null;
  rest = rest.slice(3);
  let decimal = group === '.' ? ',' : '.';
  let decimals = 0;
  if ((rest[0] === ',' || rest[0] === '.') && rest[1] === '0') {
    decimal = rest[0];
    decimals = /^0+/.exec(rest.slice(1))![0].length;
    rest = rest.slice(1 + decimals);
  }
  const affixOk = (s: string) => [...s].length <= 8 && !/[0-9\p{Cc}]/u.test(s);
  if (group === decimal || decimals > 6 || !affixOk(prefix) || !affixOk(rest)) return null;
  return { prefix, group, decimal, decimals, suffix: rest };
}

function canonicalNumber(raw: string, format: NumberFormat | null): string | null {
  let text = raw.trim();
  if (format) {
    for (const affix of [format.prefix.trim(), format.suffix.trim()]) if (affix) text = text.split(affix).join('');
  }
  const trimmed = text.trim();
  const unsigned = trimmed.replace(/^[-−]+/, '');
  const negative = unsigned.length < trimmed.length;
  const body = unsigned.replace(/[   ']/g, '');
  if (!body || !/^[0-9,.]+$/.test(body)) return null;
  const commas = body.split(',').length - 1;
  const points = body.split('.').length - 1;
  let decimal: string | null;
  if (commas === 0 && points === 0) decimal = null;
  else if (points === 0 && commas > 1) decimal = null;
  else if (commas === 0 && points > 1) decimal = null;
  else if (commas + points === 1) {
    const sep = commas === 1 ? ',' : '.';
    const after = body.length - body.lastIndexOf(sep) - 1;
    decimal = format?.group === sep && after === 3 ? null : sep;
  } else {
    const sep = body.lastIndexOf(',') > body.lastIndexOf('.') ? ',' : '.';
    if (body.split(sep).length - 1 > 1) return null;
    decimal = sep;
  }
  let int: string;
  let frac: string;
  if (decimal) {
    const at = body.lastIndexOf(decimal);
    int = body.slice(0, at).replace(/[,.]/g, '');
    frac = body.slice(at + 1);
  } else {
    int = body.replace(/[,.]/g, '');
    frac = '';
  }
  if (/[,.]/.test(frac) || (!int && !frac) || int.length + frac.length > MAX_DIGITS) return null;
  if (format) [int, frac] = roundHalfUp(int, frac, format.decimals);
  int = int.replace(/^0+/, '');
  frac = frac.replace(/0+$/, '');
  return `${negative && (int || frac) ? '-' : ''}${int || '0'}${frac ? `.${frac}` : ''}`;
}

/** Half away from zero on the digits themselves, so 0.125 rounds the same here and on the server. */
export function roundHalfUp(int: string, frac: string, decimals: number): [string, string] {
  if (frac.length <= decimals) return [int, frac];
  const digits = `${int}${frac.slice(0, decimals)}`.split('').map(Number);
  if (Number(frac[decimals]) >= 5) {
    let i = digits.length;
    for (;;) {
      if (i === 0) {
        digits.unshift(1);
        break;
      }
      i -= 1;
      if (digits[i] === 9) digits[i] = 0;
      else {
        digits[i]! += 1;
        break;
      }
    }
  }
  const text = digits.join('');
  const split = text.length - decimals;
  return [text.slice(0, split), text.slice(split)];
}

function writeNumber(format: NumberFormat, canonical: string): string | null {
  if (numberValue(canonical) === null) return null;
  const negative = canonical.startsWith('-');
  const [rawInt = '', rawFrac = ''] = canonical.replace(/^-+/, '').split('.');
  const [roundedInt, roundedFrac] = roundHalfUp(rawInt, rawFrac, format.decimals);
  const frac = roundedFrac.padEnd(format.decimals, '0');
  const int = roundedInt.replace(/^0+/, '') || '0';
  let grouped = '';
  for (let i = 0; i < int.length; i++) {
    if (i > 0 && (int.length - i) % 3 === 0 && format.group) grouped += format.group;
    grouped += int[i];
  }
  const sign = negative && /[1-9]/.test(`${int}${frac}`) ? '-' : '';
  return `${sign}${format.prefix}${grouped}${frac ? `${format.decimal}${frac}` : ''}${format.suffix}`;
}

function writeDate(format: string, iso: string) {
  const [y, m, d] = iso.split('-');
  switch (format) {
    case 'DD/MM/YYYY':
      return `${d}/${m}/${y}`;
    case 'MM/DD/YYYY':
      return `${m}/${d}/${y}`;
    case 'DD.MM.YYYY':
      return `${d}.${m}.${y}`;
    case 'D MMM YYYY':
      return `${Number(d)} ${MONTHS[Number(m) - 1]} ${y}`;
    default:
      return iso;
  }
}

/** A stored value as it is written on the page. */
export function display(kind: string, format: string | null | undefined, value: string): string {
  switch (kind) {
    case 'number':
    case 'formula': {
      const f = format ? parseNumberFormat(format) : null;
      return (f && writeNumber(f, value)) ?? value;
    }
    case 'date':
    case 'date_input':
      return format && isoDate(value) ? writeDate(format, value) : value;
    case 'multiselect':
      try {
        const list = JSON.parse(value);
        return Array.isArray(list) ? list.join(', ') : value;
      } catch {
        return value;
      }
    default:
      return value;
  }
}

/** The stored choices of a multi-select field. */
export function picked(value: string | null | undefined): string[] {
  try {
    const list = JSON.parse(value ?? '[]');
    return Array.isArray(list) ? list.filter((x): x is string => typeof x === 'string') : [];
  } catch {
    return [];
  }
}
