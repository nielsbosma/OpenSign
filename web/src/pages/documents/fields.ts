import type { FieldInput, FieldKind, FieldRow, PartyView } from '@/lib/api';
import { msg, t } from '@/lib/i18n';

export type EditableField = FieldInput & { key: string };

export const FIELD_KINDS: { kind: FieldKind; label: string; width: number; height: number }[] = [
  { kind: 'signature', label: msg('Signature'), width: 0.26, height: 0.06 },
  { kind: 'initials', label: msg('Initials'), width: 0.09, height: 0.045 },
  { kind: 'name', label: msg('Name'), width: 0.24, height: 0.032 },
  { kind: 'date', label: msg('Date'), width: 0.16, height: 0.032 },
  { kind: 'text', label: msg('Text'), width: 0.24, height: 0.032 },
  { kind: 'email', label: msg('Email'), width: 0.24, height: 0.032 },
  { kind: 'company', label: msg('Company'), width: 0.24, height: 0.032 },
  { kind: 'checkbox', label: msg('Checkbox'), width: 0.028, height: 0.02 },
];

export const kindLabel = (kind: string) => t(FIELD_KINDS.find((k) => k.kind === kind)?.label ?? kind);

/** One color per party, stable by position. The sender's own fields are grey. */
const PALETTE = ['#4f46e5', '#0891b2', '#c026d3', '#ea580c', '#16a34a', '#db2777', '#ca8a04', '#2563eb'];

export function partyColor(parties: PartyView[], partyId?: string | null) {
  const index = parties.findIndex((p) => p.id === partyId);
  return index < 0 ? '#71717a' : PALETTE[index % PALETTE.length]!;
}

export function toEditable(fields: FieldRow[]): EditableField[] {
  return fields.map((f) => ({
    key: f.id,
    kind: f.kind as FieldKind,
    label: f.label || null,
    page: f.page,
    partyId: f.partyId ?? null,
    required: f.required,
    value: f.value ?? null,
    x: f.x,
    y: f.y,
    width: f.width,
    height: f.height,
  }));
}

export function toInput(fields: EditableField[]): FieldInput[] {
  return fields.map(({ key: _key, ...f }) => f);
}

export const clamp = (v: number, min = 0, max = 1) => Math.min(max, Math.max(min, v));
