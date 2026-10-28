import { describe, expect, it } from 'vitest';
import type { FieldRow, PartyView } from '@/lib/api';
import { clamp, FIELD_KINDS, partyColor, toEditable, toInput } from './fields';

describe('fields', () => {
  it('keeps every default field on the page', () => {
    for (const k of FIELD_KINDS) {
      expect(k.width).toBeGreaterThan(0);
      expect(k.width).toBeLessThan(1);
      expect(k.height).toBeLessThan(1);
    }
  });

  it('clamps positions to the page', () => {
    expect(clamp(-0.2)).toBe(0);
    expect(clamp(1.4)).toBe(1);
    expect(clamp(0.5, 0, 0.3)).toBe(0.3);
  });

  it('colors parties by position and the sender grey', () => {
    const parties = [{ id: 'a' }, { id: 'b' }] as PartyView[];
    expect(partyColor(parties, 'a')).not.toBe(partyColor(parties, 'b'));
    expect(partyColor(parties, null)).toBe('#71717a');
  });

  it('round-trips stored fields to inputs without their ids', () => {
    const row = {
      id: 'f1',
      kind: 'text',
      label: 'Ref',
      partyId: 'a',
      page: 2,
      x: 0.1,
      y: 0.2,
      width: 0.3,
      height: 0.04,
      required: true,
      value: null,
    } as FieldRow;
    const [input] = toInput(toEditable([row]));
    expect(input).toMatchObject({ kind: 'text', partyId: 'a', page: 2, x: 0.1, y: 0.2 });
    expect(input).not.toHaveProperty('key');
    expect(input).not.toHaveProperty('id');
  });
});
