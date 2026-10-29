import { Check, Sparkles, Wand2, X } from 'lucide-react';
import { toast } from 'sonner';
import { Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu';
import type { FieldsFromFile, SuggestedField } from '@/lib/api';
import { t } from '@/lib/i18n';
import { kindLabel } from './fields';

/** The suggestions on one page, drawn dashed. A click accepts one; the cross dismisses it. */
export function SuggestionLayer({
  page,
  suggestions,
  onAccept,
  onDismiss,
}: {
  page: number;
  suggestions: SuggestedField[];
  onAccept: (s: SuggestedField) => void;
  onDismiss: (s: SuggestedField) => void;
}) {
  return (
    <div className="pointer-events-none absolute inset-0">
      {suggestions
        .filter((s) => s.page === page)
        .map((s) => (
          <div
            key={`${s.x}:${s.y}:${s.kind}`}
            className="group pointer-events-auto absolute flex items-center rounded-[3px] border border-dashed border-primary bg-primary/10 text-[10px] leading-none text-primary"
            style={{ left: `${s.x * 100}%`, top: `${s.y * 100}%`, width: `${s.width * 100}%`, height: `${s.height * 100}%` }}
          >
            <button
              type="button"
              className="flex h-full min-w-0 flex-1 items-center gap-1 px-1 text-start"
              title={t('Add this field')}
              onClick={() => onAccept(s)}
            >
              {s.source === 'ai' && <Sparkles className="size-2.5 shrink-0" />}
              <span className="truncate font-medium">{s.label ? `${kindLabel(s.kind)} · ${s.label}` : kindLabel(s.kind)}</span>
            </button>
            <button
              type="button"
              aria-label={t('Dismiss')}
              onClick={() => onDismiss(s)}
              className="absolute -top-2 -end-2 hidden size-4 items-center justify-center rounded-full border bg-background text-foreground shadow-sm group-hover:flex"
            >
              <X className="size-2.5" />
            </button>
          </div>
        ))}
    </div>
  );
}

/** Shown above the pages while there are suggestions to go through. */
export function SuggestionBar({
  count,
  note,
  onAcceptAll,
  onDismissAll,
}: {
  count: number;
  note?: string | null;
  onAcceptAll: () => void;
  onDismissAll: () => void;
}) {
  return (
    <div className="mb-3 flex flex-wrap items-center gap-2 rounded-lg border border-primary/30 bg-primary/5 px-3 py-2">
      <Wand2 className="size-4 text-primary" />
      <span className="me-auto">
        {count === 1 ? t('1 suggested field. Click one to add it.') : t('{n} suggested fields. Click one to add it.', { n: count })}
        {note && <span className="block text-xs text-muted-foreground">{note}</span>}
      </span>
      <Button size="sm" onClick={onAcceptAll}>
        <Check /> {t('Accept All')}
      </Button>
      <Button size="sm" variant="ghost" onClick={onDismissAll}>
        {t('Dismiss All')}
      </Button>
    </div>
  );
}

/** The Detect Fields button, with an AI option when an AI model may read document text. */
export function DetectButton({ busy, aiReady, onDetect }: { busy: boolean; aiReady: boolean; onDetect: (ai: boolean) => void }) {
  const icon = busy ? <Spinner /> : <Wand2 />;
  if (!aiReady)
    return (
      <Button
        size="sm"
        variant="ghost"
        onClick={() => onDetect(false)}
        disabled={busy}
        title={t('Find places in the file that want a field')}
      >
        {icon} {t('Detect Fields')}
      </Button>
    );
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button size="sm" variant="ghost" disabled={busy}>
          {icon} {t('Detect Fields')}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onSelect={() => onDetect(false)}>
          <Wand2 /> {t('From Lines and Boxes')}
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => onDetect(true)}>
          <Sparkles /> {t('With AI Too')}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Tells the sender what an upload changed: active content removed, fields added from tags or a form. */
export function announceUpload(file: string, out: { removed: string[]; fields: FieldsFromFile }) {
  if (out.removed.length) toast.warning(t('Removed from {file}: {items}', { file, items: out.removed.join(', ') }));
  const { textTags, formFields, partiesAdded, problems } = out.fields;
  const added = [
    textTags && (textTags === 1 ? t('1 field from a text tag') : t('{n} fields from text tags', { n: textTags })),
    formFields && (formFields === 1 ? t('1 field from the PDF form') : t('{n} fields from the PDF form', { n: formFields })),
  ].filter(Boolean);
  if (added.length) {
    toast.success(t('Added {fields} from {file}', { fields: added.join(', '), file }), {
      description: partiesAdded.length ? t('New parties: {names}. Add their details.', { names: partiesAdded.join(', ') }) : undefined,
    });
  }
  if (problems.length)
    toast.warning(t('Some tags or form fields in {file} were not used', { file }), { description: problems.slice(0, 5).join(' ') });
}
