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
  { kind: 'number', label: msg('Number'), width: 0.16, height: 0.032 },
  { kind: 'select', label: msg('Dropdown'), width: 0.2, height: 0.032 },
  { kind: 'multiselect', label: msg('Multi-Select'), width: 0.24, height: 0.032 },
  { kind: 'radio', label: msg('Radio'), width: 0.022, height: 0.016 },
  { kind: 'phone', label: msg('Phone'), width: 0.2, height: 0.032 },
  { kind: 'date_input', label: msg('Date Picker'), width: 0.16, height: 0.032 },
  { kind: 'image', label: msg('Image'), width: 0.2, height: 0.09 },
  { kind: 'file', label: msg('File'), width: 0.24, height: 0.032 },
  { kind: 'formula', label: msg('Formula'), width: 0.16, height: 0.032 },
];

/** What the sender can fill in themselves before sending. */
export const SENDER_KINDS: FieldKind[] = [
  'text',
  'date',
  'checkbox',
  'name',
  'company',
  'email',
  'number',
  'select',
  'multiselect',
  'phone',
  'date_input',
  'formula',
];

/** Kinds whose value the signer types or picks, so they can have a default and be read-only. */
export const VALUE_KINDS: FieldKind[] = ['text', 'company', 'checkbox', 'number', 'select', 'multiselect', 'radio', 'phone', 'date_input'];

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
    options: f.options,
    validation: Object.keys(f.validation ?? {}).length ? f.validation : null,
    format: f.format ?? null,
    groupKey: f.groupKey ?? null,
    readOnly: f.readOnly,
    conditions: f.conditions ?? [],
    formula: f.formula ?? null,
  }));
}

export function toInput(fields: EditableField[]): FieldInput[] {
  return fields.map(({ key: _key, ...f }) => f);
}

/** Renames a label in every condition and formula that uses it, so references follow a renamed field. */
export function renameReferences(fields: EditableField[], from: string, to: string): EditableField[] {
  const old = from.trim();
  const next = to.trim();
  const owners = new Set(fields.filter((f) => (f.label ?? '').trim() === old).map((f) => f.groupKey ?? f.key));
  if (!old || !next || old === next || owners.size > 1) return fields;
  const braces = new RegExp(`\\{\\s*${old.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\}`, 'g');
  return fields.map((f) => {
    const uses = f.conditions?.some((c) => c.field.trim() === old) || braces.test(f.formula ?? '');
    braces.lastIndex = 0;
    if (!uses) return f;
    return {
      ...f,
      conditions: f.conditions?.map((c) => (c.field.trim() === old ? { ...c, field: next } : c)),
      formula: f.formula?.replace(braces, () => `{${next}}`),
    };
  });
}

/** A short random key that ties radio buttons into a group. */
export const newGroupKey = () => `g${crypto.randomUUID().slice(0, 8)}`;

export const clamp = (v: number, min = 0, max = 1) => Math.min(max, Math.max(min, v));
