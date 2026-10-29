import { describe, expect, it } from 'vitest';
import type { Condition } from './api';
import { evaluate, formulaError } from './field-logic';
import cases from './field-logic.cases.json';

type Doc = {
  name: string;
  fields: { label: string; kind: string; value?: string; conditions?: Condition[]; formula?: string; format?: string }[];
  visible: boolean[];
  values: (string | null)[];
};

describe('field logic', () => {
  it.each(cases.documents as Doc[])('$name', (doc) => {
    const out = evaluate(doc.fields);
    expect(out.map((o) => o.visible)).toEqual(doc.visible);
    expect(out.map((o) => o.value)).toEqual(doc.values);
  });

  it.each(cases.invalid)('refuses $formula', (c) => {
    expect(formulaError(c.formula)).not.toBeNull();
  });
});
