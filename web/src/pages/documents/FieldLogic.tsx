import { Plus, X } from 'lucide-react';
import { useRef } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Textarea } from '@/components/ui/textarea';
import type { Condition, ConditionOp } from '@/lib/api';
import { dependencies, evaluate, formulaError, MAX_CONDITIONS, takesValue } from '@/lib/field-logic';
import { display } from '@/lib/field-values';
import { msg, t } from '@/lib/i18n';
import { type EditableField, kindLabel } from './fields';

type Target = { label: string; kind: string; options: string[] };

/** The fields another field can look at: one entry per label, a radio group counted once. */
function targets(fields: EditableField[], self: EditableField): Target[] {
  const out: Target[] = [];
  for (const f of fields) {
    const label = (f.label ?? '').trim();
    if (!label || f.key === self.key || (self.groupKey && f.groupKey === self.groupKey)) continue;
    const known = out.find((o) => o.label === label);
    if (known) known.options = [...new Set([...known.options, ...(f.options ?? [])])];
    else out.push({ label, kind: f.kind, options: f.options ?? [] });
  }
  return out;
}

const OP_LABELS: Record<ConditionOp, string> = {
  equals: msg('Is'),
  not_equals: msg('Is Not'),
  filled: msg('Is Filled In'),
  empty: msg('Is Empty'),
  contains: msg('Contains'),
  greater: msg('Is More Than'),
  less: msg('Is Less Than'),
};

function opsFor(kind: string): ConditionOp[] {
  switch (kind) {
    case 'checkbox':
      return ['filled', 'empty'];
    case 'multiselect':
      return ['contains', 'filled', 'empty'];
    case 'number':
    case 'formula':
    case 'date_input':
      return ['equals', 'not_equals', 'greater', 'less', 'filled', 'empty'];
    case 'select':
    case 'radio':
      return ['equals', 'not_equals', 'filled', 'empty'];
    default:
      return ['equals', 'not_equals', 'contains', 'filled', 'empty'];
  }
}

function opLabel(op: ConditionOp, kind: string) {
  if (kind === 'checkbox') return op === 'filled' ? t('Is Ticked') : t('Is Not Ticked');
  if (kind === 'date_input') {
    if (op === 'greater') return t('Is After');
    if (op === 'less') return t('Is Before');
  }
  return t(OP_LABELS[op]);
}

/** "Show only when…": rules on other fields that must all hold for this field to be shown. */
export function ConditionsEditor({
  field,
  fields,
  onChange,
}: {
  field: EditableField;
  fields: EditableField[];
  onChange: (conditions: Condition[]) => void;
}) {
  const options = targets(fields, field);
  const conditions = field.conditions ?? [];
  const set = (i: number, patch: Partial<Condition>) => onChange(conditions.map((c, j) => (j === i ? { ...c, ...patch } : c)));

  return (
    <div className="grid gap-2">
      <div>
        <Label className="text-xs text-muted-foreground">{t('Show Only When')}</Label>
        {!conditions.length && <p className="text-xs text-muted-foreground">{t('Always shown.')}</p>}
      </div>
      {conditions.map((c, i) => {
        const target = options.find((o) => o.label === c.field.trim());
        const kind = target?.kind ?? 'text';
        const ops = opsFor(kind);
        return (
          <div key={i} className="grid gap-1.5 rounded-md border p-2">
            <div className="flex items-center gap-1.5">
              <Select
                value={target ? target.label : ''}
                onValueChange={(label) => {
                  const next = options.find((o) => o.label === label);
                  const op = opsFor(next?.kind ?? 'text').includes(c.op) ? c.op : opsFor(next?.kind ?? 'text')[0]!;
                  set(i, { field: label, op, value: null });
                }}
              >
                <SelectTrigger size="sm" className="min-w-0 flex-1" aria-label={t('Field')}>
                  <SelectValue placeholder={c.field || t('Choose a Field')} />
                </SelectTrigger>
                <SelectContent>
                  {options.map((o) => (
                    <SelectItem key={o.label} value={o.label}>
                      {o.label} <span className="text-muted-foreground">· {kindLabel(o.kind)}</span>
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Button
                variant="ghost"
                size="icon"
                className="size-7 shrink-0"
                aria-label={t('Remove Condition')}
                onClick={() => onChange(conditions.filter((_, j) => j !== i))}
              >
                <X />
              </Button>
            </div>
            <div className="grid grid-cols-2 gap-1.5">
              <Select
                value={c.op}
                onValueChange={(op) => set(i, { op: op as ConditionOp, value: takesValue(op as ConditionOp) ? c.value : null })}
              >
                <SelectTrigger size="sm" className="w-full" aria-label={t('Rule')}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {ops.map((op) => (
                    <SelectItem key={op} value={op}>
                      {opLabel(op, kind)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              {takesValue(c.op) &&
                (target?.options.length ? (
                  <Select value={c.value ?? ''} onValueChange={(value) => set(i, { value })}>
                    <SelectTrigger size="sm" className="w-full" aria-label={t('Value')}>
                      <SelectValue placeholder={t('Choose')} />
                    </SelectTrigger>
                    <SelectContent>
                      {target.options.map((o) => (
                        <SelectItem key={o} value={o}>
                          {o}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                ) : (
                  <Input
                    className="h-8"
                    aria-label={t('Value')}
                    type={kind === 'date_input' ? 'date' : 'text'}
                    inputMode={kind === 'number' || kind === 'formula' ? 'decimal' : undefined}
                    value={c.value ?? ''}
                    onChange={(e) => set(i, { value: e.target.value })}
                  />
                ))}
            </div>
          </div>
        );
      })}
      {options.length > 0 && conditions.length < MAX_CONDITIONS && (
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            const first = options[0]!;
            onChange([...conditions, { field: first.label, op: opsFor(first.kind)[0]!, value: null }]);
          }}
        >
          <Plus /> {t('Add a Condition')}
        </Button>
      )}
      {!options.length && <p className="text-xs text-muted-foreground">{t('Give other fields a label to use them in conditions.')}</p>}
    </div>
  );
}

/** The expression of a formula field, with a list of labels to insert and its result from the current values. */
export function FormulaEditor({
  field,
  fields,
  onChange,
}: {
  field: EditableField;
  fields: EditableField[];
  onChange: (formula: string) => void;
}) {
  const area = useRef<HTMLTextAreaElement>(null);
  const formula = field.formula ?? '';
  const labels = targets(fields, field).filter((o) => o.kind !== 'signature' && o.kind !== 'image' && o.kind !== 'file');
  const error = formula.trim() ? formulaError(formula) : null;
  const unknown = error ? [] : dependencies({ formula }).filter((l) => !labels.some((o) => o.label === l));
  const index = fields.findIndex((f) => f.key === field.key);
  const result =
    !error && !unknown.length && formula.trim() ? evaluate(fields.map((f) => ({ ...f, label: f.label ?? '' })))[index]?.value : null;

  const insert = (label: string) => {
    const el = area.current;
    const at = el?.selectionStart ?? formula.length;
    const end = el?.selectionEnd ?? at;
    const piece = `{${label}}`;
    onChange(formula.slice(0, at) + piece + formula.slice(end));
    requestAnimationFrame(() => {
      el?.focus();
      el?.setSelectionRange(at + piece.length, at + piece.length);
    });
  };

  return (
    <div className="grid gap-1.5">
      <Label htmlFor="field-formula" className="text-xs text-muted-foreground">
        {t('Formula')}
      </Label>
      <Textarea
        ref={area}
        id="field-formula"
        rows={3}
        dir="ltr"
        className="font-mono text-xs"
        placeholder="{Hours} * {Rate} * 1.25"
        value={formula}
        aria-invalid={!!error || unknown.length > 0 || undefined}
        onChange={(e) => onChange(e.target.value)}
      />
      {labels.length > 0 && (
        <Select value="" onValueChange={insert}>
          <SelectTrigger size="sm" className="w-full">
            <SelectValue placeholder={t('Insert a Field')} />
          </SelectTrigger>
          <SelectContent>
            {labels.map((o) => (
              <SelectItem key={o.label} value={o.label}>
                {o.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      )}
      {error ? (
        <p className="text-xs text-destructive">{error}</p>
      ) : unknown.length ? (
        <p className="text-xs text-destructive">{t('No field is labelled {label}.', { label: unknown[0]! })}</p>
      ) : (
        <p className="text-xs text-muted-foreground">
          {t('Use + − × ÷, brackets, round, min, max, sum and if. Fields go in braces.')}
          {result != null && ` ${t('Now: {value}', { value: display('formula', field.format, result) })}`}
        </p>
      )}
    </div>
  );
}
