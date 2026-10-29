import { describe, expect, it } from 'vitest';
import type { FieldKind, Validation } from './api';
import { display, normalize } from './field-values';
import cases from './field-values.cases.json';

type Case = {
  kind: string;
  options?: string[];
  validation?: Validation;
  format?: string;
  input: string;
  stored?: string | null;
  error?: string;
  shown?: string;
};

describe('field values', () => {
  it.each(cases.values as Case[])('$kind $input', (c) => {
    const got = normalize(c.kind as FieldKind, c, c.input);
    if (c.error) expect(got.ok ? null : got.error.code).toBe(c.error);
    else expect(got).toEqual({ ok: true, value: c.stored ?? null });
  });

  it.each(cases.formats as Case[])('shows $input as $shown', (c) => {
    expect(display(c.kind, c.format, c.input)).toBe(c.shown);
  });
});
