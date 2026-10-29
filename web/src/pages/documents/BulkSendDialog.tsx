import { Sparkles, Upload } from 'lucide-react';
import { useState } from 'react';
import { useNavigate } from 'react-router';
import { Field, Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectTrigger, SelectValue } from '@/components/ui/select';
import { type BulkPreviewOutput, type ColumnMapping, call, errorMessage, useApi } from '@/lib/api';
import { msg, t } from '@/lib/i18n';
import { FormError } from '../auth/AuthLayout';

const ACCEPT =
  '.csv,.tsv,.txt,.xlsx,.xls,.ods,text/csv,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet,application/vnd.ms-excel';
const IGNORE = '__ignore';

async function toBase64(file: File) {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = '';
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary);
}

/** Send a template to every row of a spreadsheet: pick the file, check the column mapping, send. */
/** The server labels mapping targets in English; they are translated here. */
export const TARGET_LABELS = [
  msg('Title'),
  msg('Your Reference'),
  msg('Name'),
  msg('Email'),
  msg('Mobile'),
  msg('Company'),
  msg('Personal Number'),
  msg('Language'),
];

export function BulkSendDialog({
  templateId,
  templateTitle,
  open,
  onOpenChange,
}: {
  templateId: string;
  templateTitle: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const navigate = useNavigate();
  const ai = useApi('ai.status', undefined, { enabled: open });
  const [file, setFile] = useState<{ name: string; base64: string }>();
  const [preview, setPreview] = useState<BulkPreviewOutput>();
  const [mapping, setMapping] = useState<ColumnMapping[]>([]);
  const [titlePattern, setTitlePattern] = useState('');
  const [busy, setBusy] = useState<'read' | 'ai' | 'check' | 'send'>();
  const [error, setError] = useState<string>();

  const reset = () => {
    setFile(undefined);
    setPreview(undefined);
    setMapping([]);
    setTitlePattern('');
    setError(undefined);
  };
  const run = async (step: typeof busy, fn: () => Promise<void>) => {
    setBusy(step);
    setError(undefined);
    try {
      await fn();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(undefined);
    }
  };
  const read = (picked: File | undefined, useAi = false) =>
    picked &&
    run(useAi ? 'ai' : 'read', async () => {
      const next = { name: picked.name, base64: await toBase64(picked) };
      const out = await call('bulk.preview', { templateId, fileName: next.name, contentBase64: next.base64, useAi });
      setFile(next);
      setPreview(out);
      setMapping(out.mapping);
    });
  const rematch = () =>
    file &&
    run('ai', async () => {
      const out = await call('bulk.preview', { templateId, fileName: file.name, contentBase64: file.base64, useAi: true });
      setPreview(out);
      setMapping(out.mapping);
    });
  const send = () =>
    file &&
    run('send', async () => {
      const bulk = await call('bulk.start', {
        templateId,
        fileName: file.name,
        contentBase64: file.base64,
        mapping,
        titlePattern: titlePattern.trim() || null,
      });
      onOpenChange(false);
      reset();
      navigate(`/bulk/${bulk.id}`);
    });

  const groups = preview
    ? [...new Set(preview.targets.map((t) => t.group))].map((g) => ({ group: g, targets: preview.targets.filter((t) => t.group === g) }))
    : [];
  const used = new Set(mapping.map((m) => m.target).filter(Boolean));
  const setTarget = (column: string, target: string) =>
    setMapping(mapping.map((m) => (m.column === column ? { ...m, target: target === IGNORE ? '' : target } : m)));
  const canAi = ai.data?.configured && ai.data.canUse;

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        if (!o) reset();
        onOpenChange(o);
      }}
    >
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{t('Bulk Send “{title}”', { title: templateTitle })}</DialogTitle>
          <DialogDescription>
            {t('Each row of a CSV or Excel file becomes its own document, sent from this template. The first row names the columns.')}
          </DialogDescription>
        </DialogHeader>
        <FormError message={error} />
        {!preview ? (
          <label className="flex cursor-pointer flex-col items-center gap-2 rounded-lg border border-dashed px-6 py-8 text-center hover:bg-accent/50">
            {busy ? <Spinner className="size-5" /> : <Upload className="size-5 text-muted-foreground" />}
            <span className="font-medium">{t('Choose a CSV or Excel File')}</span>
            <span className="text-xs text-muted-foreground">
              {t('Up to 1,000 rows and 5 MB. Columns are matched to the template by name.')}
            </span>
            <input type="file" accept={ACCEPT} className="sr-only" onChange={(e) => void read(e.target.files?.[0])} />
          </label>
        ) : (
          <div className="grid gap-4">
            <div className="flex flex-wrap items-center justify-between gap-2 text-sm">
              <span>
                <span className="font-medium">{file?.name}</span> ·{' '}
                {preview.rowCount === 1 ? t('1 row') : t('{count} rows', { count: preview.rowCount })}
                {preview.aiUsed && <span className="text-muted-foreground"> {t('· matched with AI')}</span>}
              </span>
              <div className="flex gap-2">
                {canAi && (
                  <Button type="button" variant="outline" size="sm" disabled={!!busy} onClick={() => void rematch()}>
                    {busy === 'ai' ? <Spinner /> : <Sparkles />} {t('Match with AI')}
                  </Button>
                )}
                <Button type="button" variant="ghost" size="sm" disabled={!!busy} onClick={reset}>
                  {t('Choose Another File')}
                </Button>
              </div>
            </div>
            <div className="overflow-x-auto rounded-md border">
              <table className="w-full text-sm">
                <thead className="bg-muted/50 text-start text-xs text-muted-foreground">
                  <tr>
                    <th className="px-3 py-2 font-medium">{t('Column')}</th>
                    <th className="px-3 py-2 font-medium">{t('First Row')}</th>
                    <th className="px-3 py-2 font-medium">{t('Fills In')}</th>
                  </tr>
                </thead>
                <tbody>
                  {mapping.map((m) => {
                    const index = preview.columns.indexOf(m.column);
                    return (
                      <tr key={m.column} className="border-t">
                        <td className="px-3 py-1.5 font-medium">{m.column}</td>
                        <td className="max-w-48 truncate px-3 py-1.5 text-muted-foreground">{preview.sampleRows[0]?.[index] ?? ''}</td>
                        <td className="px-3 py-1.5">
                          <Select value={m.target || IGNORE} onValueChange={(v) => setTarget(m.column, v)}>
                            <SelectTrigger size="sm" className="w-56" aria-label={t('What {column} fills in', { column: m.column })}>
                              <SelectValue />
                            </SelectTrigger>
                            <SelectContent>
                              <SelectItem value={IGNORE}>{t('Ignore')}</SelectItem>
                              {groups.map((g) => (
                                <SelectGroup key={g.group}>
                                  <SelectLabel>{g.group === 'Document' ? t('Document') : g.group}</SelectLabel>
                                  {g.targets.map((target) => (
                                    <SelectItem
                                      key={target.key}
                                      value={target.key}
                                      disabled={used.has(target.key) && target.key !== m.target}
                                    >
                                      {g.group === 'Document' ? t(target.label) : `${g.group}: ${t(target.label)}`}
                                    </SelectItem>
                                  ))}
                                </SelectGroup>
                              ))}
                            </SelectContent>
                          </Select>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
            {!used.has('title') && (
              <Field
                label={t('Document Titles')}
                htmlFor="title-pattern"
                hint={t(
                  'Put a column name in braces to use its value, e.g. {example}. Leave empty for the template title and the first party’s name.',
                  {
                    example: `${templateTitle} {${preview.columns[0] ?? ''}}`,
                  },
                )}
              >
                <Input
                  id="title-pattern"
                  value={titlePattern}
                  onChange={(e) => setTitlePattern(e.target.value)}
                  placeholder={`${templateTitle} - ${t('name')}`}
                />
              </Field>
            )}
            {preview.problems.length > 0 && (
              <div className="grid gap-1 rounded-md border border-destructive/40 bg-destructive/5 p-3 text-sm">
                <span className="font-medium text-destructive">{t('Fix these rows before sending')}</span>
                <ul className="grid gap-0.5 text-xs">
                  {preview.problems.map((p, i) => (
                    <li key={i}>
                      {t('Row {row}:', { row: p.row })} {p.message}
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        )}
        <DialogFooter>
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
            {t('Cancel')}
          </Button>
          {preview && (
            <Button type="button" disabled={!!busy || !used.size} onClick={() => void send()}>
              {busy === 'send' && <Spinner className="text-primary-foreground" />}{' '}
              {preview.rowCount === 1 ? t('Send 1 Document') : t('Send {count} Documents', { count: preview.rowCount })}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
