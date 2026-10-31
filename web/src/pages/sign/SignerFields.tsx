import { CheckCircle2, ChevronDown, ImagePlus, Paperclip, X } from 'lucide-react';
import { useRef, useState } from 'react';
import { Spinner } from '@/components/common';
import { Checkbox } from '@/components/ui/checkbox';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { call, errorMessage, type FieldKind, type FieldRow, uploadFieldFile } from '@/lib/api';
import { evaluate, type Outcome } from '@/lib/field-logic';
import { display, type Invalid, normalize, picked } from '@/lib/field-values';
import { cn } from '@/lib/utils';
import { type Texts, useTexts } from './texts';

/** Kinds the signer fills in on the page. */
export const INPUT_KINDS = [
  'text',
  'initials',
  'company',
  'checkbox',
  'number',
  'select',
  'multiselect',
  'radio',
  'phone',
  'date_input',
  'image',
  'file',
];

export type Upload = { name: string; preview?: string };

/** What the signer starts from: the sender's defaults, and uploads already made. */
export function initialValues(own: FieldRow[], company?: string | null) {
  const values: Record<string, string> = {};
  const uploads: Record<string, Upload> = {};
  for (const f of own) {
    if (f.kind === 'image' || f.kind === 'file') {
      if (f.value) uploads[f.id] = { name: f.value };
    } else if (INPUT_KINDS.includes(f.kind) && !f.readOnly) {
      values[f.id] = f.value ?? (f.kind === 'company' ? (company ?? '') : '');
    }
  }
  return { values, uploads };
}

export function fieldName(f: FieldRow, t: Texts) {
  if (f.label) return f.label;
  switch (f.kind) {
    case 'initials':
      return t.initials;
    case 'company':
      return t.company;
    case 'checkbox':
      return t.checkbox;
    case 'number':
      return t.numberField;
    case 'phone':
      return t.phoneField;
    case 'date_input':
      return t.dateField;
    case 'image':
      return t.imageField;
    case 'file':
      return t.fileField;
    case 'select':
    case 'multiselect':
    case 'radio':
      return t.choose;
    default:
      return t.text;
  }
}

function invalidText(f: FieldRow, error: Invalid, t: Texts) {
  const own = f.validation?.message?.trim();
  if (own) return own;
  switch (error.code) {
    case 'number':
      return t.invalidNumber;
    case 'phone':
      return t.invalidPhone;
    case 'date':
      return t.invalidDate;
    case 'option':
      return t.invalidOption;
    case 'tooShort':
      return t.tooShort(error.n);
    case 'tooLong':
      return t.tooLong(error.n);
    case 'below':
      return t.below(display(f.kind, f.format, error.limit));
    case 'above':
      return t.above(display(f.kind, f.format, error.limit));
  }
}

/**
 * Which fields are shown and what each formula comes to, from what the party has entered so far.
 * The server works it out again when they sign.
 */
export function outcomes(
  fields: FieldRow[],
  party: { id: string; name: string; email?: string | null },
  values: Record<string, string>,
  uploads: Record<string, Upload>,
): Map<string, Outcome> {
  const today = new Date().toISOString().slice(0, 10);
  const items = fields.map((f) => {
    let value = f.value ?? null;
    if (f.partyId === party.id) {
      if (f.kind === 'name') value = party.name;
      else if (f.kind === 'email') value = party.email ?? null;
      else if (f.kind === 'date') value = today;
      else if (f.kind === 'signature') value = 'signed';
      else if (f.kind === 'image' || f.kind === 'file') value = uploads[f.id]?.name ?? null;
      else if (f.id in values) {
        const raw = values[f.id]!;
        if (f.kind === 'checkbox') value = raw === 'true' ? 'true' : 'false';
        else if (f.kind === 'radio') value = raw || null;
        else {
          const checked = normalize(f.kind as FieldKind, f, raw);
          value = checked.ok ? checked.value : null;
        }
      }
    }
    return { label: f.label, kind: f.kind, value, conditions: f.conditions, formula: f.formula, format: f.format };
  });
  const out = evaluate(items);
  return new Map(fields.map((f, i) => [f.id, out[i]!]));
}

/** What is wrong with each of the party's fields, in their language. The server checks the same again. */
export function problems(
  own: FieldRow[],
  values: Record<string, string>,
  uploads: Record<string, Upload>,
  t: Texts,
  shown: (f: FieldRow) => boolean = () => true,
) {
  const out: Record<string, string> = {};
  for (const f of own) {
    if (f.readOnly || !INPUT_KINDS.includes(f.kind) || f.kind === 'radio' || !shown(f)) continue;
    const value = values[f.id] ?? '';
    if (f.kind === 'image' || f.kind === 'file') {
      if (f.required && !uploads[f.id]) out[f.id] = t.uploadFor(fieldName(f, t));
    } else if (f.kind === 'checkbox') {
      if (f.required && value !== 'true') out[f.id] = t.fillIn(fieldName(f, t));
    } else {
      const checked = normalize(f.kind as FieldKind, f, value);
      if (!checked.ok) out[f.id] = invalidText(f, checked.error, t);
      else if (f.required && checked.value === null) out[f.id] = t.fillIn(fieldName(f, t));
    }
  }
  const groups = new Set(own.filter((f) => f.kind === 'radio' && f.groupKey && shown(f)).map((f) => f.groupKey!));
  for (const group of groups) {
    const buttons = own.filter((f) => f.groupKey === group && shown(f));
    const chosen = buttons.some((b) => (b.readOnly ? b.value : values[b.id]));
    if (!chosen && buttons.some((b) => b.required)) out[buttons[0]!.id] = t.chooseFor(fieldName(buttons[0]!, t));
  }
  return out;
}

const boxClass = 'pointer-events-auto absolute rounded-[3px] border bg-primary/10 text-[11px] text-zinc-900 outline-none focus:bg-white';

/** One of the party's fields, drawn over its place on the page. */
export function SignerField({
  field: f,
  fields,
  style,
  values,
  onValues,
  uploads,
  onUpload,
  problem,
  showProblem,
  onTouch,
  token,
  session,
  preview,
}: {
  field: FieldRow;
  fields: FieldRow[];
  style: React.CSSProperties;
  values: Record<string, string>;
  onValues: (values: Record<string, string>) => void;
  uploads: Record<string, Upload>;
  onUpload: (id: string, upload: Upload | null) => void;
  problem?: string;
  showProblem: boolean;
  onTouch: (id: string) => void;
  token: string;
  session: string | null;
  preview?: boolean;
}) {
  const t = useTexts();
  const value = values[f.id] ?? '';
  const set = (v: string) => onValues({ ...values, [f.id]: v });
  const name = fieldName(f, t);
  const flagged = showProblem && !!problem;
  const border = flagged ? 'border-destructive ring-1 ring-destructive' : 'border-primary';
  const id = `field-${f.id}`;
  const hint = flagged && (
    <span className="pointer-events-none absolute top-full start-0 z-10 mt-0.5 max-w-64 rounded bg-destructive px-1.5 py-0.5 text-[10px] leading-tight text-white">
      {problem}
    </span>
  );
  const wrap = (child: React.ReactNode) => (
    <div className="pointer-events-none absolute" style={style}>
      {child}
      {hint}
    </div>
  );
  const fill = { left: 0, top: 0, width: '100%', height: '100%' };

  if (f.readOnly) {
    return (
      <div
        id={id}
        className="absolute flex items-center rounded-[3px] border border-dashed border-zinc-400 px-1 text-[11px] text-zinc-800"
        style={style}
      >
        <span className="truncate">
          {f.kind === 'checkbox'
            ? f.value === 'true'
              ? '✓'
              : ''
            : f.kind === 'radio'
              ? f.value
                ? '●'
                : '○'
              : display(f.kind, f.format, f.value ?? '')}
        </span>
      </div>
    );
  }

  switch (f.kind) {
    case 'checkbox':
      return wrap(
        <label className="pointer-events-auto absolute flex items-center justify-center" style={fill}>
          <Checkbox id={id} checked={value === 'true'} onCheckedChange={(c) => set(c ? 'true' : 'false')} aria-label={name} />
        </label>,
      );
    case 'radio':
      return wrap(
        <input
          id={id}
          type="radio"
          name={`radio-${f.groupKey}`}
          checked={!!value}
          aria-label={`${name}: ${f.options[0] ?? ''}`}
          title={f.options[0]}
          onChange={() => {
            const next = { ...values };
            for (const b of fields.filter((b) => b.groupKey === f.groupKey)) next[b.id] = '';
            next[f.id] = f.options[0] ?? '';
            onValues(next);
            onTouch(f.id);
          }}
          className={cn(
            'pointer-events-auto absolute m-0 cursor-pointer appearance-none rounded-full border bg-primary/10 checked:bg-primary checked:shadow-[inset_0_0_0_2px_white]',
            border,
          )}
          style={fill}
        />,
      );
    case 'select':
      return wrap(
        <select
          id={id}
          aria-label={name}
          value={value}
          onChange={(e) => set(e.target.value)}
          onBlur={() => onTouch(f.id)}
          className={cn(boxClass, border, 'px-0.5')}
          style={fill}
        >
          <option value="">{f.label || t.choose}</option>
          {f.options.map((o) => (
            <option key={o} value={o}>
              {o}
            </option>
          ))}
        </select>,
      );
    case 'multiselect': {
      const chosen = picked(value);
      return wrap(
        <Popover onOpenChange={(open) => !open && onTouch(f.id)}>
          <PopoverTrigger asChild>
            <button
              id={id}
              type="button"
              aria-label={name}
              className={cn(boxClass, border, 'flex items-center gap-1 px-1 text-start')}
              style={fill}
            >
              <span className={cn('min-w-0 flex-1 truncate', !chosen.length && 'text-zinc-500')}>
                {chosen.length ? chosen.join(', ') : f.label || t.choose}
              </span>
              <ChevronDown className="size-3 shrink-0" />
            </button>
          </PopoverTrigger>
          <PopoverContent className="grid w-60 gap-2 p-3" align="start">
            {f.label && <div className="text-sm font-medium">{f.label}</div>}
            {f.options.map((o) => (
              <label key={o} className="flex items-center gap-2 text-sm">
                <Checkbox
                  checked={chosen.includes(o)}
                  onCheckedChange={(c) => {
                    const next = f.options.filter((x) => (x === o ? c === true : chosen.includes(x)));
                    set(next.length ? JSON.stringify(next) : '');
                  }}
                />
                {o}
              </label>
            ))}
            <div className="text-xs text-muted-foreground">{t.selectedCount(chosen.length)}</div>
          </PopoverContent>
        </Popover>,
      );
    }
    case 'image':
    case 'file':
      return wrap(
        <UploadBox
          field={f}
          id={id}
          name={name}
          border={border}
          style={fill}
          upload={uploads[f.id]}
          onUpload={onUpload}
          token={token}
          session={session}
          disabled={preview}
        />,
      );
    default: {
      const kind = f.kind;
      const min = kind === 'date_input' ? (f.validation?.min ?? undefined) : undefined;
      const max = kind === 'date_input' ? (f.validation?.max ?? undefined) : undefined;
      return wrap(
        <input
          id={id}
          dir={kind === 'phone' || kind === 'number' || kind === 'date_input' ? 'ltr' : 'auto'}
          type={kind === 'date_input' ? 'date' : kind === 'phone' ? 'tel' : 'text'}
          inputMode={kind === 'number' ? 'decimal' : undefined}
          autoComplete={kind === 'phone' ? 'tel' : kind === 'company' ? 'organization' : 'off'}
          min={min}
          max={max}
          maxLength={f.validation?.maxLength ?? undefined}
          aria-label={name}
          aria-invalid={flagged || undefined}
          placeholder={kind === 'phone' ? '+46 70 123 45 67' : name}
          value={value}
          onFocus={() => kind === 'phone' && !value && set('+')}
          onChange={(e) => set(e.target.value)}
          onBlur={() => {
            if (kind === 'phone' && value === '+') set('');
            onTouch(f.id);
          }}
          className={cn(boxClass, border, 'px-1')}
          style={fill}
        />,
      );
    }
  }
}

function UploadBox({
  field,
  id,
  name,
  border,
  style,
  upload,
  onUpload,
  token,
  session,
  disabled,
}: {
  field: FieldRow;
  id: string;
  name: string;
  border: string;
  style: React.CSSProperties;
  upload?: Upload;
  onUpload: (id: string, upload: Upload | null) => void;
  token: string;
  session: string | null;
  disabled?: boolean;
}) {
  const t = useTexts();
  const input = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const image = field.kind === 'image';
  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    setError(undefined);
    try {
      await fn();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  const pick = (file: File | undefined) => {
    if (!file) return;
    void run(async () => {
      const row = await uploadFieldFile(token, session, field.id, file);
      onUpload(field.id, { name: row.value ?? file.name, preview: image ? URL.createObjectURL(file) : undefined });
    });
  };
  const remove = () =>
    run(async () => {
      await call('signing.remove_field_file', { token, session, fieldId: field.id });
      onUpload(field.id, null);
    });

  return (
    <div
      className={cn(
        'pointer-events-auto absolute flex items-center overflow-hidden rounded-[3px] border bg-primary/10 text-[11px] text-zinc-900',
        border,
      )}
      style={style}
    >
      <input
        ref={input}
        type="file"
        hidden
        accept={image ? 'image/jpeg,image/png' : 'application/pdf,image/jpeg,image/png'}
        onChange={(e) => {
          pick(e.target.files?.[0]);
          e.target.value = '';
        }}
      />
      {upload?.preview && <img src={upload.preview} alt="" className="absolute inset-0 size-full object-contain" />}
      <button
        id={id}
        type="button"
        disabled={disabled || busy}
        onClick={() => input.current?.click()}
        title={error ?? upload?.name ?? name}
        aria-label={upload ? `${name}: ${upload.name}` : image ? t.uploadImage : t.uploadFieldFile}
        className={cn(
          'relative flex min-w-0 flex-1 items-center gap-1 self-stretch px-1 text-start',
          upload?.preview && 'bg-white/60 opacity-0 hover:opacity-100 focus:opacity-100',
        )}
      >
        {busy ? (
          <Spinner className="size-3" />
        ) : upload ? (
          <CheckCircle2 className="size-3 shrink-0 text-success" />
        ) : image ? (
          <ImagePlus className="size-3 shrink-0" />
        ) : (
          <Paperclip className="size-3 shrink-0" />
        )}
        <span className={cn('truncate', error && 'text-destructive')}>
          {error ?? upload?.name ?? (field.label || (image ? t.uploadImage : t.uploadFieldFile))}
        </span>
      </button>
      {upload && !disabled && (
        <button
          type="button"
          onClick={remove}
          disabled={busy}
          aria-label={t.removeFile}
          className="relative flex shrink-0 items-center self-stretch bg-white/70 px-0.5"
        >
          <X className="size-3" />
        </button>
      )}
    </div>
  );
}
