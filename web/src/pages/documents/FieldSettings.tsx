import { Plus, Trash2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Textarea } from '@/components/ui/textarea';
import type { PartyView, Validation } from '@/lib/api';
import { DATE_FORMATS, display, NUMBER_FORMATS, picked, takesPattern } from '@/lib/field-values';
import { msg, t } from '@/lib/i18n';
import { ConditionsEditor, FormulaEditor } from './FieldLogic';
import { type EditableField, kindLabel, renameReferences, SENDER_KINDS, VALUE_KINDS } from './fields';

/** Patterns offered in the editor. They work the same in the browser and on the server. */
const PRESETS: { key: string; label: string; pattern: string }[] = [
  { key: 'email', label: msg('Email Address'), pattern: '[^@\\s]+@[^@\\s]+\\.[^@\\s]+' },
  { key: 'digits', label: msg('Digits Only'), pattern: '[0-9]+' },
  { key: 'postcode', label: msg('Swedish Postcode'), pattern: '[0-9]{3} ?[0-9]{2}' },
  { key: 'pnr', label: msg('Swedish Personal Number'), pattern: '(19|20)?[0-9]{6}[-+]?[0-9]{4}' },
  { key: 'orgnr', label: msg('Swedish Organisation Number'), pattern: '[0-9]{6}-?[0-9]{4}' },
];

const NONE = '_none';
const CUSTOM = '_custom';

/** Settings for the field selected in the editor: who fills it in, its options, checks and format. */
export function FieldSettings({
  field,
  fields,
  parties,
  onChange,
  onRemove,
}: {
  field: EditableField;
  fields: EditableField[];
  parties: PartyView[];
  onChange: (next: EditableField[]) => void;
  onRemove: () => void;
}) {
  const kind = field.kind;
  const group = kind === 'radio' && field.groupKey ? fields.filter((f) => f.groupKey === field.groupKey) : [field];
  const patchAll = (patch: Partial<EditableField>) =>
    onChange(fields.map((f) => (group.some((g) => g.key === f.key) ? { ...f, ...patch } : f)));
  const patch = (p: Partial<EditableField>) => onChange(fields.map((f) => (f.key === field.key ? { ...f, ...p } : f)));
  const validation: Validation = field.validation ?? {};
  const setValidation = (p: Partial<Validation>) => {
    const next = Object.fromEntries(Object.entries({ ...validation, ...p }).filter(([, v]) => v !== null && v !== '' && v !== undefined));
    patch({ validation: Object.keys(next).length ? next : null });
  };
  const signers = parties.filter((p) => p.role !== 'viewer');
  const forSender = !field.partyId;
  const automatic = kind === 'name' || kind === 'email' || kind === 'date';
  const valueKind = VALUE_KINDS.includes(kind);

  return (
    <section className="grid gap-3 rounded-lg border p-3">
      <div className="flex items-center justify-between gap-2">
        <h3 className="font-medium">{t('{field} Field', { field: kindLabel(kind) })}</h3>
        <Button variant="ghost" size="icon" className="size-7" aria-label={t('Remove Field')} onClick={onRemove}>
          <Trash2 />
        </Button>
      </div>

      <Row label={t('Filled In By')}>
        <Select
          value={field.partyId ?? NONE}
          onValueChange={(v) => patchAll({ partyId: v === NONE ? null : v, readOnly: v === NONE ? false : field.readOnly })}
        >
          <SelectTrigger size="sm" className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {signers.map((p) => (
              <SelectItem key={p.id} value={p.id}>
                {p.name}
              </SelectItem>
            ))}
            {SENDER_KINDS.includes(kind) && <SelectItem value={NONE}>{t('Me, now (prefilled)')}</SelectItem>}
          </SelectContent>
        </Select>
      </Row>

      <Row label={kind === 'radio' ? t('Group Label') : t('Label')} htmlFor="field-label">
        <Input
          id="field-label"
          value={field.label ?? ''}
          onChange={(e) => {
            const label = e.target.value;
            const renamed = renameReferences(fields, field.label ?? '', label);
            onChange(renamed.map((f) => (group.some((g) => g.key === f.key) ? { ...f, label } : f)));
          }}
        />
      </Row>

      {(kind === 'select' || kind === 'multiselect') && <OptionsEditor field={field} onChange={patch} />}

      {kind === 'formula' && (
        <>
          <FormulaEditor field={field} fields={fields} onChange={(formula) => patch({ formula })} />
          <FormatPicker
            value={field.format}
            formats={NUMBER_FORMATS}
            none={t('As Computed')}
            sample={(f) => display('number', f, '1234.5')}
            onChange={(format) => patch({ format })}
          />
        </>
      )}

      {kind === 'radio' && (
        <>
          <Row label={t('Option of This Button')} htmlFor="field-option">
            <Input
              id="field-option"
              value={field.options?.[0] ?? ''}
              onChange={(e) => patch({ options: [e.target.value], value: field.value ? e.target.value : null })}
            />
          </Row>
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              const last = group[group.length - 1]!;
              const y = Math.min(1 - last.height, last.y + last.height * 1.8);
              const next: EditableField = {
                ...last,
                key: crypto.randomUUID(),
                y,
                value: null,
                options: [t('Option {n}', { n: group.length + 1 })],
              };
              onChange([...fields, next]);
            }}
          >
            <Plus /> {t('Add a Button to the Group')}
          </Button>
          <ToggleRow
            label={t('Chosen by Default')}
            checked={!!field.value}
            onChange={(on) =>
              onChange(
                fields.map((f) =>
                  f.key === field.key
                    ? { ...f, value: on ? (f.options?.[0] ?? null) : null }
                    : group.some((g) => g.key === f.key) && on
                      ? { ...f, value: null }
                      : f,
                ),
              )
            }
          />
        </>
      )}

      {valueKind && kind !== 'radio' && (
        <DefaultValue field={field} label={forSender ? t('Value') : t('Default Value')} onChange={(value) => patch({ value })} />
      )}

      {!automatic && !forSender && kind !== 'formula' && (
        <ToggleRow
          label={kind === 'radio' ? t('Answer Required') : t('Required')}
          checked={field.required !== false}
          onChange={(required) => (kind === 'radio' ? patchAll : patch)({ required })}
        />
      )}
      {valueKind && !forSender && (
        <ToggleRow
          label={t('Read-Only')}
          hint={t('The signer sees the default value but cannot change it.')}
          checked={!!field.readOnly}
          onChange={(readOnly) => (kind === 'radio' ? patchAll : patch)({ readOnly })}
        />
      )}

      {takesPattern(kind) && !field.readOnly && (
        <>
          <PatternPicker validation={validation} onChange={setValidation} />
          <div className="grid grid-cols-2 gap-2">
            <Row label={t('Min Length')} htmlFor="field-minlength">
              <NumberInput id="field-minlength" value={validation.minLength} onChange={(minLength) => setValidation({ minLength })} />
            </Row>
            <Row label={t('Max Length')} htmlFor="field-maxlength">
              <NumberInput id="field-maxlength" value={validation.maxLength} onChange={(maxLength) => setValidation({ maxLength })} />
            </Row>
          </div>
        </>
      )}

      {kind === 'number' && (
        <>
          <div className="grid grid-cols-2 gap-2">
            <Row label={t('Smallest')} htmlFor="field-min">
              <Input
                id="field-min"
                inputMode="decimal"
                value={validation.min ?? ''}
                onChange={(e) => setValidation({ min: e.target.value.trim() })}
              />
            </Row>
            <Row label={t('Largest')} htmlFor="field-max">
              <Input
                id="field-max"
                inputMode="decimal"
                value={validation.max ?? ''}
                onChange={(e) => setValidation({ max: e.target.value.trim() })}
              />
            </Row>
          </div>
          <FormatPicker
            value={field.format}
            formats={NUMBER_FORMATS}
            none={t('As Typed')}
            sample={(f) => display('number', f, '1234.5')}
            onChange={(format) => patch({ format })}
          />
        </>
      )}

      {kind === 'date_input' && (
        <div className="grid grid-cols-2 gap-2">
          <Row label={t('Earliest')} htmlFor="field-min">
            <Input id="field-min" type="date" value={validation.min ?? ''} onChange={(e) => setValidation({ min: e.target.value })} />
          </Row>
          <Row label={t('Latest')} htmlFor="field-max">
            <Input id="field-max" type="date" value={validation.max ?? ''} onChange={(e) => setValidation({ max: e.target.value })} />
          </Row>
        </div>
      )}
      {(kind === 'date_input' || kind === 'date') && (
        <FormatPicker
          value={field.format}
          formats={DATE_FORMATS.slice(1)}
          none={display('date', null, '2026-12-31')}
          sample={(f) => display('date', f, '2026-12-31')}
          onChange={(format) => patch({ format })}
        />
      )}

      {(takesPattern(kind) || kind === 'number' || kind === 'date_input') && !field.readOnly && !forSender && (
        <Row label={t('Error Text')} htmlFor="field-message">
          <Input
            id="field-message"
            value={validation.message ?? ''}
            placeholder={t('Shown when a check fails')}
            onChange={(e) => setValidation({ message: e.target.value })}
          />
        </Row>
      )}
      <ConditionsEditor field={field} fields={fields} onChange={(conditions) => patchAll({ conditions })} />
    </section>
  );
}

function Row({ label, htmlFor, children }: { label: string; htmlFor?: string; children: React.ReactNode }) {
  return (
    <div className="grid gap-1.5">
      <Label htmlFor={htmlFor} className="text-xs text-muted-foreground">
        {label}
      </Label>
      {children}
    </div>
  );
}

function ToggleRow({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (on: boolean) => void;
}) {
  return (
    <label className="flex items-center justify-between gap-3">
      <span>
        <span className="block">{label}</span>
        {hint && <span className="block text-xs text-muted-foreground">{hint}</span>}
      </span>
      <Switch checked={checked} onCheckedChange={onChange} />
    </label>
  );
}

function NumberInput({ id, value, onChange }: { id: string; value?: number | null; onChange: (v: number | null) => void }) {
  return (
    <Input
      id={id}
      type="number"
      min={0}
      max={1000}
      value={value ?? ''}
      onChange={(e) => onChange(e.target.value === '' ? null : Math.max(0, Math.round(Number(e.target.value))))}
    />
  );
}

/** One option per line; kept as typed while editing so blank lines and spaces do not jump away. */
function OptionsEditor({ field, onChange }: { field: EditableField; onChange: (p: Partial<EditableField>) => void }) {
  const [text, setText] = useState((field.options ?? []).join('\n'));
  // biome-ignore lint/correctness/useExhaustiveDependencies: reset only when another field is selected
  useEffect(() => setText((field.options ?? []).join('\n')), [field.key]);
  return (
    <Row label={t('Options, One per Line')} htmlFor="field-options">
      <Textarea
        id="field-options"
        rows={4}
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          const options = [
            ...new Set(
              e.target.value
                .split('\n')
                .map((o) => o.trim())
                .filter(Boolean),
            ),
          ];
          const value =
            field.kind === 'multiselect'
              ? JSON.stringify(picked(field.value).filter((v) => options.includes(v)))
              : field.value && options.includes(field.value)
                ? field.value
                : null;
          onChange({ options, value: value === '[]' ? null : value });
        }}
      />
    </Row>
  );
}

function DefaultValue({ field, label, onChange }: { field: EditableField; label: string; onChange: (value: string | null) => void }) {
  const value = field.value ?? '';
  const options = field.options ?? [];
  let input: React.ReactNode;
  switch (field.kind) {
    case 'checkbox':
      return <ToggleRow label={t('Ticked by Default')} checked={value === 'true'} onChange={(on) => onChange(on ? 'true' : null)} />;
    case 'select':
      input = (
        <Select value={value || NONE} onValueChange={(v) => onChange(v === NONE ? null : v)}>
          <SelectTrigger size="sm" className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={NONE}>{t('None')}</SelectItem>
            {options.map((o) => (
              <SelectItem key={o} value={o}>
                {o}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      );
      break;
    case 'multiselect': {
      const chosen = picked(value);
      input = (
        <div className="grid gap-1">
          {options.map((o) => (
            <label key={o} className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={chosen.includes(o)}
                onChange={(e) => {
                  const next = options.filter((x) => (x === o ? e.target.checked : chosen.includes(x)));
                  onChange(next.length ? JSON.stringify(next) : null);
                }}
              />
              {o}
            </label>
          ))}
        </div>
      );
      break;
    }
    case 'date_input':
      input = <Input id="field-value" type="date" value={value} onChange={(e) => onChange(e.target.value || null)} />;
      break;
    default:
      input = (
        <Input
          id="field-value"
          type={field.kind === 'phone' ? 'tel' : 'text'}
          inputMode={field.kind === 'number' ? 'decimal' : undefined}
          placeholder={field.kind === 'phone' ? '+46 70 123 45 67' : undefined}
          value={value}
          onChange={(e) => onChange(e.target.value || null)}
        />
      );
  }
  return (
    <Row label={label} htmlFor="field-value">
      {input}
    </Row>
  );
}

function PatternPicker({ validation, onChange }: { validation: Validation; onChange: (p: Partial<Validation>) => void }) {
  const preset = !validation.pattern ? NONE : (PRESETS.find((p) => p.pattern === validation.pattern)?.key ?? CUSTOM);
  const [custom, setCustom] = useState(preset === CUSTOM);
  const shown = custom && preset === NONE ? CUSTOM : preset;
  return (
    <>
      <Row label={t('Allowed Format')}>
        <Select
          value={shown}
          onValueChange={(v) => {
            setCustom(v === CUSTOM);
            if (v === NONE) onChange({ pattern: null });
            else if (v !== CUSTOM) onChange({ pattern: PRESETS.find((p) => p.key === v)!.pattern });
          }}
        >
          <SelectTrigger size="sm" className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={NONE}>{t('Anything')}</SelectItem>
            {PRESETS.map((p) => (
              <SelectItem key={p.key} value={p.key}>
                {t(p.label)}
              </SelectItem>
            ))}
            <SelectItem value={CUSTOM}>{t('Custom Pattern')}</SelectItem>
          </SelectContent>
        </Select>
      </Row>
      {shown === CUSTOM && (
        <Row label={t('Regular Expression')} htmlFor="field-pattern">
          <Input
            id="field-pattern"
            className="font-mono"
            value={validation.pattern ?? ''}
            placeholder="[A-Z]{2}[0-9]{4}"
            onChange={(e) => onChange({ pattern: e.target.value })}
          />
        </Row>
      )}
    </>
  );
}

function FormatPicker({
  value,
  formats,
  none,
  sample,
  onChange,
}: {
  value?: string | null;
  formats: string[];
  none: string;
  sample: (format: string | null) => string;
  onChange: (format: string | null) => void;
}) {
  const known = !value || value === 'YYYY-MM-DD' || formats.includes(value);
  return (
    <Row label={t('Format')}>
      <Select value={value && value !== 'YYYY-MM-DD' ? value : NONE} onValueChange={(v) => onChange(v === NONE ? null : v)}>
        <SelectTrigger size="sm" className="w-full">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={NONE}>{none}</SelectItem>
          {!known && <SelectItem value={value!}>{sample(value!)}</SelectItem>}
          {formats.map((f) => (
            <SelectItem key={f} value={f}>
              {sample(f)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </Row>
  );
}
