import type { Condition, ConditionOp, FieldKind } from './api';
import { numberValue, parseNumberFormat, roundHalfUp } from './field-values';
import { msg, t } from './i18n';

/**
 * Conditional fields and formulas, the same rules as `crates/core/src/documents/logic.rs`. Both run
 * `field-logic.cases.json`. Formulas are parsed into a small tree and walked: there is no `eval`.
 * The server works everything out again when the party signs.
 */

export type { ConditionOp } from './api';

export const CONDITION_OPS: ConditionOp[] = ['equals', 'not_equals', 'filled', 'empty', 'contains', 'greater', 'less'];
export const takesValue = (op: ConditionOp) => op !== 'filled' && op !== 'empty';

export const MAX_CONDITIONS = 10;
const MAX_FORMULA_CHARS = 500;
const MAX_DEPTH = 32;
const MAX_DECIMALS = 10;
const MAX_DIGITS = 15;

export type Item = {
  label: string;
  kind: FieldKind | string;
  value?: string | null;
  conditions?: Condition[] | null;
  formula?: string | null;
  format?: string | null;
};

export type Outcome = { visible: boolean; value: string | null };

type Val = { t: 'empty' } | { t: 'num'; n: number } | { t: 'text'; s: string } | { t: 'bool'; b: boolean } | { t: 'list'; l: string[] };

type Expr =
  | { e: 'num'; n: number }
  | { e: 'text'; s: string }
  | { e: 'bool'; b: boolean }
  | { e: 'ref'; name: string }
  | { e: 'neg'; x: Expr }
  | { e: 'bin'; op: string; a: Expr; b: Expr }
  | { e: 'cmp'; op: string; a: Expr; b: Expr }
  | { e: 'call'; name: string; args: Expr[] };

const EMPTY: Val = { t: 'empty' };

export const FUNCTIONS: Record<string, [number, number]> = {
  round: [1, 2],
  floor: [1, 1],
  ceil: [1, 1],
  abs: [1, 1],
  min: [1, 50],
  max: [1, 50],
  sum: [1, 50],
  if: [3, 3],
  and: [1, 50],
  or: [1, 50],
  not: [1, 1],
  contains: [2, 2],
};

type Token =
  | { k: 'num'; n: number }
  | { k: 'text'; s: string }
  | { k: 'ref'; s: string }
  | { k: 'word'; s: string }
  | { k: 'op'; s: string };

class FormulaError extends Error {
  constructor(
    text: string,
    readonly values?: Record<string, string | number>,
  ) {
    super(text);
  }
}

function tokens(source: string): Token[] {
  const chars = Array.from(source);
  const out: Token[] = [];
  let i = 0;
  while (i < chars.length) {
    const c = chars[i]!;
    if (/\s/u.test(c)) {
      i += 1;
    } else if (c === '{' || c === '"') {
      const close = c === '{' ? '}' : '"';
      const end = chars.indexOf(close, i + 1);
      if (end < 0) throw new FormulaError(c === '{' ? msg('A field name has no closing brace') : msg('A text has no closing quote'));
      const inner = chars.slice(i + 1, end).join('');
      if (c === '{') {
        if (!inner.trim()) throw new FormulaError(msg('A field name between braces is empty'));
        out.push({ k: 'ref', s: inner.trim() });
      } else out.push({ k: 'text', s: inner });
      i = end + 1;
    } else if (/[0-9.]/.test(c)) {
      const start = i;
      while (i < chars.length && /[0-9.]/.test(chars[i]!)) i += 1;
      const text = chars.slice(start, i).join('');
      const n = text.startsWith('.') || text.endsWith('.') ? null : numberValue(text);
      if (n === null) throw new FormulaError(msg('{text} is not a number'), { text });
      out.push({ k: 'num', n });
    } else if (/[A-Za-z]/.test(c)) {
      const start = i;
      while (i < chars.length && /[A-Za-z0-9]/.test(chars[i]!)) i += 1;
      out.push({ k: 'word', s: chars.slice(start, i).join('').toLowerCase() });
    } else {
      const two = c + (chars[i + 1] ?? '');
      const pairs: Record<string, string> = { '!=': '!=', '<>': '!=', '<=': '<=', '>=': '>=', '==': '=' };
      const singles: Record<string, string> = {
        '=': '=',
        '<': '<',
        '>': '>',
        '+': '+',
        '-': '-',
        '−': '-',
        '*': '*',
        '×': '*',
        '/': '/',
        '÷': '/',
        '(': '(',
        ')': ')',
        ',': ',',
        ';': ',',
      };
      if (pairs[two]) {
        out.push({ k: 'op', s: pairs[two]! });
        i += 2;
      } else if (singles[c]) {
        out.push({ k: 'op', s: singles[c]! });
        i += 1;
      } else throw new FormulaError(msg('“{char}” cannot be used in a formula'), { char: c });
    }
  }
  return out;
}

class Parser {
  at = 0;
  depth = 0;
  constructor(readonly tokens: Token[]) {}

  peek() {
    return this.tokens[this.at];
  }

  eat(op: string) {
    const tok = this.peek();
    if (tok?.k === 'op' && tok.s === op) {
      this.at += 1;
      return true;
    }
    return false;
  }

  deeper() {
    this.depth += 1;
    if (this.depth > MAX_DEPTH) throw new FormulaError(msg('The formula is nested too deeply'));
  }

  expr(): Expr {
    this.deeper();
    const left = this.sum();
    const tok = this.peek();
    if (!(tok?.k === 'op' && ['=', '!=', '<', '<=', '>', '>='].includes(tok.s))) {
      this.depth -= 1;
      return left;
    }
    this.at += 1;
    const right = this.sum();
    this.depth -= 1;
    return { e: 'cmp', op: tok.s, a: left, b: right };
  }

  sum(): Expr {
    let left = this.product();
    for (;;) {
      const op = this.eat('+') ? '+' : this.eat('-') ? '-' : null;
      if (!op) return left;
      left = { e: 'bin', op, a: left, b: this.product() };
    }
  }

  product(): Expr {
    let left = this.unary();
    for (;;) {
      const op = this.eat('*') ? '*' : this.eat('/') ? '/' : null;
      if (!op) return left;
      left = { e: 'bin', op, a: left, b: this.unary() };
    }
  }

  unary(): Expr {
    if (this.eat('-')) {
      this.deeper();
      const inner = this.unary();
      this.depth -= 1;
      return { e: 'neg', x: inner };
    }
    if (this.eat('+')) return this.unary();
    return this.atom();
  }

  atom(): Expr {
    const tok = this.peek();
    if (!tok) throw new FormulaError(msg('The formula ends too early'));
    this.at += 1;
    switch (tok.k) {
      case 'num':
        return { e: 'num', n: tok.n };
      case 'text':
        return { e: 'text', s: tok.s };
      case 'ref':
        return { e: 'ref', name: tok.s };
      case 'word': {
        if (tok.s === 'true' || tok.s === 'false') return { e: 'bool', b: tok.s === 'true' };
        const limits = Object.hasOwn(FUNCTIONS, tok.s) ? FUNCTIONS[tok.s] : undefined;
        if (!limits) throw new FormulaError(msg('There is no function called {name}'), { name: tok.s });
        if (!this.eat('(')) throw new FormulaError(msg('Write {name}( ) with its values in brackets'), { name: tok.s });
        const args: Expr[] = [];
        if (!this.eat(')')) {
          for (;;) {
            args.push(this.expr());
            if (this.eat(')')) break;
            if (!this.eat(',')) throw new FormulaError(msg('Separate the values of {name}( ) with commas'), { name: tok.s });
          }
        }
        const [min, max] = limits;
        if (args.length < min || args.length > max) {
          throw min === max
            ? new FormulaError(msg('{name}( ) takes {n} values'), { name: tok.s, n: min })
            : new FormulaError(msg('{name}( ) takes from {min} to {max} values'), { name: tok.s, min, max });
        }
        return { e: 'call', name: tok.s, args };
      }
      default:
        if (tok.s === '(') {
          const inner = this.expr();
          if (!this.eat(')')) throw new FormulaError(msg('A bracket is not closed'));
          return inner;
        }
        throw new FormulaError(msg('“{op}” is in an unexpected place'), { op: tok.s });
    }
  }
}

function parse(source: string): Expr {
  if (Array.from(source).length > MAX_FORMULA_CHARS)
    throw new FormulaError(msg('A formula is at most {n} characters'), { n: MAX_FORMULA_CHARS });
  const parser = new Parser(tokens(source));
  if (!parser.tokens.length) throw new FormulaError(msg('The formula is empty'));
  const expr = parser.expr();
  if (parser.at < parser.tokens.length) throw new FormulaError(msg('Something follows the end of the formula; is an operator missing?'));
  return expr;
}

/** Why a formula cannot be read, in the app's language, or null when it can. */
export function formulaError(source: string): string | null {
  try {
    parse(source);
    return null;
  } catch (e) {
    if (e instanceof FormulaError) return t(e.message, e.values);
    throw e;
  }
}

function refs(expr: Expr, out: string[]) {
  switch (expr.e) {
    case 'ref':
      if (!out.includes(expr.name)) out.push(expr.name);
      break;
    case 'neg':
      refs(expr.x, out);
      break;
    case 'bin':
    case 'cmp':
      refs(expr.a, out);
      refs(expr.b, out);
      break;
    case 'call':
      for (const a of expr.args) refs(a, out);
      break;
  }
}

/** The labels a field's conditions and formula look at. */
export function dependencies(item: Pick<Item, 'conditions' | 'formula'>): string[] {
  const out: string[] = [];
  for (const c of item.conditions ?? []) if (!out.includes(c.field.trim())) out.push(c.field.trim());
  if (item.formula) {
    try {
      refs(parse(item.formula), out);
    } catch {
      // A formula that cannot be read uses nothing.
    }
  }
  return out;
}

/** The shortest decimal that reads back as `n`, written out in full as Rust does. */
function plain(n: number): string {
  if (n === 0) return '0';
  const s = String(n);
  const m = /^(-?)(\d)(?:\.(\d+))?e([+-]\d+)$/.exec(s);
  if (!m) return s;
  const [, sign, lead, frac = '', exp] = m;
  const e = Number(exp);
  const digits = lead! + frac;
  return e >= 0 ? `${sign}${digits.padEnd(e + 1, '0')}` : `${sign}0.${'0'.repeat(-e - 1)}${digits}`;
}

function split(n: number): [boolean, string, string] {
  const text = plain(n);
  const negative = text.startsWith('-');
  const [int = '', frac = ''] = text.replace(/^-/, '').split('.');
  return [negative, int, frac];
}

function roundTo(n: number, decimals: number): number | null {
  const [negative, int, frac] = split(n);
  const [i, f] = roundHalfUp(int, frac, decimals);
  const out = Number(`${negative ? '-' : ''}${i}.${f}0`);
  if (!Number.isFinite(out)) return null;
  return out === 0 ? 0 : out;
}

function canonical(n: number, format?: string | null): string | null {
  if (!Number.isFinite(n)) return null;
  const [negative, rawInt, rawFrac] = split(n);
  const int0 = rawInt.replace(/^0+/, '');
  if (int0.length > MAX_DIGITS) return null;
  const parsed = format ? parseNumberFormat(format) : null;
  const wanted = parsed ? parsed.decimals : MAX_DECIMALS;
  const [rInt, rFrac] = roundHalfUp(int0, rawFrac, Math.min(wanted, MAX_DIGITS - int0.length));
  const int = rInt.replace(/^0+/, '');
  if (int.length > MAX_DIGITS) return null;
  const frac = rFrac.replace(/0+$/, '');
  return `${negative && (int || frac) ? '-' : ''}${int || '0'}${frac ? `.${frac}` : ''}`;
}

function numberOf(v: Val): number | null {
  if (v.t === 'num') return v.n;
  if (v.t === 'text') return numberValue(v.s.trim());
  return null;
}

function arithmetic(v: Val): number | null {
  if (v.t === 'empty') return 0;
  if (v.t === 'bool') return v.b ? 1 : 0;
  if (v.t === 'list') return null;
  return numberOf(v);
}

function textOf(v: Val): string {
  switch (v.t) {
    case 'empty':
      return '';
    case 'num':
      return plain(v.n);
    case 'text':
      return v.s;
    case 'bool':
      return String(v.b);
    case 'list':
      return v.l.join(', ');
  }
}

function filled(v: Val): boolean {
  switch (v.t) {
    case 'empty':
      return false;
    case 'num':
      return true;
    case 'text':
      return v.s !== '';
    case 'bool':
      return v.b;
    case 'list':
      return v.l.length > 0;
  }
}

const truthy = (v: Val) => (v.t === 'num' ? v.n !== 0 : filled(v));

function equal(a: Val, b: Val): boolean {
  if (a.t === 'empty' && b.t === 'empty') return true;
  if (a.t === 'num' || b.t === 'num') {
    const x = numberOf(a);
    const y = numberOf(b);
    return x !== null && y !== null && x === y;
  }
  return textOf(a).toLowerCase() === textOf(b).toLowerCase();
}

function order(a: Val, b: Val): -1 | 0 | 1 | null {
  const x = numberOf(a);
  const y = numberOf(b);
  if (x !== null && y !== null) return x < y ? -1 : x > y ? 1 : 0;
  if (a.t === 'text' && b.t === 'text' && a.s && b.s) return a.s < b.s ? -1 : a.s > b.s ? 1 : 0;
  return null;
}

function contains(a: Val, b: Val): boolean {
  if (a.t === 'list') return a.l.some((i) => equal({ t: 'text', s: i }, b));
  if (a.t === 'text') {
    const needle = textOf(b).toLowerCase();
    return needle !== '' && a.s.toLowerCase().includes(needle);
  }
  return false;
}

function evaluateExpr(expr: Expr, lookup: (name: string) => Val): Val | null {
  switch (expr.e) {
    case 'num':
      return { t: 'num', n: expr.n };
    case 'text':
      return { t: 'text', s: expr.s };
    case 'bool':
      return { t: 'bool', b: expr.b };
    case 'ref':
      return lookup(expr.name);
    case 'neg': {
      const v = evaluateExpr(expr.x, lookup);
      const n = v && arithmetic(v);
      return n === null ? null : { t: 'num', n: -n };
    }
    case 'bin': {
      const a = evaluateExpr(expr.a, lookup);
      const x = a && arithmetic(a);
      if (x === null) return null;
      const b = evaluateExpr(expr.b, lookup);
      const y = b && arithmetic(b);
      if (y === null) return null;
      if (expr.op === '/' && y === 0) return null;
      const n = expr.op === '+' ? x + y : expr.op === '-' ? x - y : expr.op === '*' ? x * y : x / y;
      return Number.isFinite(n) ? { t: 'num', n } : null;
    }
    case 'cmp': {
      const a = evaluateExpr(expr.a, lookup);
      if (!a) return null;
      const b = evaluateExpr(expr.b, lookup);
      if (!b) return null;
      const o = order(a, b);
      const results: Record<string, boolean> = {
        '=': equal(a, b),
        '!=': !equal(a, b),
        '<': o === -1,
        '>': o === 1,
        '<=': o === -1 || o === 0,
        '>=': o === 1 || o === 0,
      };
      return { t: 'bool', b: results[expr.op]! };
    }
    case 'call':
      return call(expr.name, expr.args, lookup);
  }
}

function call(name: string, args: Expr[], lookup: (name: string) => Val): Val | null {
  if (name === 'if') {
    const condition = evaluateExpr(args[0]!, lookup);
    if (!condition) return null;
    return evaluateExpr(args[truthy(condition) ? 1 : 2]!, lookup);
  }
  const vals: Val[] = [];
  for (const a of args) {
    const v = evaluateExpr(a, lookup);
    if (!v) return null;
    vals.push(v);
  }
  const nums = () => {
    const out: number[] = [];
    for (const v of vals) {
      const n = arithmetic(v);
      if (n === null) return null;
      out.push(n);
    }
    return out;
  };
  const num = (n: number | null): Val | null => (n === null ? null : { t: 'num', n });
  switch (name) {
    case 'round': {
      const ns = nums();
      if (!ns) return null;
      const decimals = ns[1] ?? 0;
      if (decimals < 0 || decimals > 6 || !Number.isInteger(decimals)) return null;
      return num(roundTo(ns[0]!, decimals));
    }
    case 'floor':
    case 'ceil':
    case 'abs': {
      const n = arithmetic(vals[0]!);
      return num(n === null ? null : Math[name](n));
    }
    case 'min':
    case 'max':
    case 'sum': {
      const ns = nums();
      if (!ns) return null;
      return num(name === 'sum' ? ns.reduce((s, n) => s + n, 0) : Math[name](...ns));
    }
    case 'and':
      return { t: 'bool', b: vals.every(truthy) };
    case 'or':
      return { t: 'bool', b: vals.some(truthy) };
    case 'not':
      return { t: 'bool', b: !truthy(vals[0]!) };
    case 'contains':
      return { t: 'bool', b: contains(vals[0]!, vals[1]!) };
    default:
      return null;
  }
}

function holds(condition: Condition, value: Val): boolean {
  const other: Val = { t: 'text', s: condition.value ?? '' };
  switch (condition.op) {
    case 'equals':
      return equal(value, other);
    case 'not_equals':
      return !equal(value, other);
    case 'filled':
      return filled(value);
    case 'empty':
      return !filled(value);
    case 'contains':
      return contains(value, other);
    case 'greater':
      return order(value, other) === 1;
    case 'less':
      return order(value, other) === -1;
  }
}

function fieldValue(kind: string, raw?: string | null): Val {
  const value = raw?.trim() || null;
  switch (kind) {
    case 'checkbox':
      return { t: 'bool', b: value === 'true' };
    case 'number':
    case 'formula': {
      const n = value === null ? null : numberValue(value);
      return n === null ? EMPTY : { t: 'num', n };
    }
    case 'multiselect': {
      if (value === null) return EMPTY;
      try {
        const list = JSON.parse(value);
        return { t: 'list', l: Array.isArray(list) ? list.filter((x): x is string => typeof x === 'string') : [] };
      } catch {
        return { t: 'list', l: [] };
      }
    }
    default:
      return value === null ? EMPTY : { t: 'text', s: value };
  }
}

/** Works out which fields are shown and what each formula comes to. */
export function evaluate(items: Item[]): Outcome[] {
  const byLabel = new Map<string, number[]>();
  items.forEach((item, i) => {
    const label = item.label.trim();
    if (label) byLabel.set(label, [...(byLabel.get(label) ?? []), i]);
  });
  const done: (Outcome | undefined)[] = new Array(items.length);
  const busy: boolean[] = new Array(items.length).fill(false);

  const labelValue = (label: string): Val => {
    const indexes = byLabel.get(label.trim());
    if (!indexes) return EMPTY;
    let chosen: Val = EMPTY;
    for (const i of indexes) {
      const o = outcome(i);
      const item = items[i]!;
      if (item.kind === 'radio') {
        if (o.visible && o.value !== null) chosen = { t: 'text', s: o.value };
      } else return o.visible ? fieldValue(item.kind, o.value) : EMPTY;
    }
    return chosen;
  };

  const outcome = (i: number): Outcome => {
    const known = done[i];
    if (known) return known;
    if (busy[i]) return { visible: false, value: null };
    busy[i] = true;
    const item = items[i]!;
    const visible = (item.conditions ?? []).every((c) => holds(c, labelValue(c.field)));
    let value: string | null = null;
    if (visible && item.kind === 'formula') {
      let expr: Expr | null = null;
      try {
        expr = item.formula ? parse(item.formula) : null;
      } catch {
        expr = null;
      }
      const result = expr && evaluateExpr(expr, labelValue);
      const n = result && arithmetic(result);
      value = n === null || n === undefined ? null : canonical(n, item.format);
    } else if (visible) value = item.value ?? null;
    busy[i] = false;
    const out = { visible, value };
    done[i] = out;
    return out;
  };

  return items.map((_, i) => outcome(i));
}
