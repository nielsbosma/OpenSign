import { Eye, History, RotateCcw, Save, X } from 'lucide-react';
import { useMemo, useState } from 'react';
import { Link } from 'react-router';
import { toast } from 'sonner';
import { EmptyState, HelpLink, Loading } from '@/components/common';
import { PdfLoading, PdfPages, usePdf } from '@/components/PdfPages';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Textarea } from '@/components/ui/textarea';
import {
  type Change,
  type DocumentView,
  type FieldKind,
  fetchPdf,
  type PartyView,
  type TemplateVersionSummary,
  useApi,
  useApiMutation,
} from '@/lib/api';
import { formatDateTime } from '@/lib/format';
import { labels, msg, t } from '@/lib/i18n';
import { cn } from '@/lib/utils';
import { StatusBadge } from './DocumentsPage';
import { FieldLayer } from './FieldLayer';
import { type EditableField, FIELD_KINDS, kindLabel } from './fields';

const SETTING_LABELS = labels({
  title: msg('Title'),
  message: msg('Message'),
  language: msg('Language'),
  sequential: msg('Sign in Order'),
  reminderDays: msg('Reminders'),
  reviewLinks: msg('AI Review by Signers'),
  fileOrder: msg('File order'),
});

const DETAIL_LABELS = labels({
  name: msg('name'),
  role: msg('role'),
  email: msg('email'),
  phone: msg('phone'),
  personalNumber: msg('personal number'),
  company: msg('company'),
  signOrder: msg('signing order'),
  delivery: msg('delivery'),
  authToView: msg('identification to view'),
  authToSign: msg('identification to sign'),
  language: msg('language'),
  attachments: msg('requested attachments'),
  position: msg('position'),
  required: msg('required'),
  value: msg('value'),
  options: msg('options'),
  validation: msg('validation'),
  format: msg('format'),
  group: msg('radio group'),
  readOnly: msg('read-only'),
  conditions: msg('conditions'),
  formula: msg('formula'),
});

function itemName(c: Change) {
  if (c.item === 'setting') return SETTING_LABELS[c.name] ?? c.name;
  if (c.item === 'field') {
    const name = FIELD_KINDS.some((k) => k.kind === c.name) ? kindLabel(c.name) : c.name;
    return c.party ? t('{field} for {name}', { field: name, name: c.party }) : name;
  }
  return c.name;
}

function changeText(c: Change) {
  const name = itemName(c);
  const details = c.details.map((d) => DETAIL_LABELS[d] ?? d).join(', ');
  if (c.item === 'setting') return t('{name} changed', { name });
  if (c.item === 'file') return c.change === 'added' ? t('File {name} added', { name }) : t('File {name} removed', { name });
  if (c.item === 'party') {
    if (c.change === 'added') return t('Party {name} added', { name });
    if (c.change === 'removed') return t('Party {name} removed', { name });
    return t('Party {name}: {details} changed', { name, details });
  }
  if (c.change === 'added') return t('Field {name} added', { name });
  if (c.change === 'removed') return t('Field {name} removed', { name });
  return t('Field {name}: {details} changed', { name, details });
}

export function Changes({ changes }: { changes: Change[] | undefined }) {
  if (!changes) return <Loading />;
  if (!changes.length) return <p className="text-muted-foreground">{t('No differences.')}</p>;
  return (
    <ul className="grid gap-1">
      {changes.map((c, i) => (
        <li key={i} className="flex items-start gap-2">
          <span
            className={cn(
              'mt-1.5 size-2 shrink-0 rounded-full',
              c.change === 'added' ? 'bg-success' : c.change === 'removed' ? 'bg-destructive' : 'bg-warning',
            )}
          />
          <span className="min-w-0">{changeText(c)}</span>
        </li>
      ))}
    </ul>
  );
}

function versionLabel(v: TemplateVersionSummary) {
  return t('Version {number}', { number: v.number });
}

function VersionMeta({ version }: { version: TemplateVersionSummary }) {
  return (
    <span className="text-xs text-muted-foreground">
      {version.createdByName
        ? t('{date} by {name}', { date: formatDateTime(version.createdAt), name: version.createdByName })
        : formatDateTime(version.createdAt)}
    </span>
  );
}

function SaveVersion({ templateId, onSaved }: { templateId: string; onSaved: () => void }) {
  const [note, setNote] = useState('');
  const save = useApiMutation('templates.save_version', {
    invalidates: ['templates.'],
    onSuccess: (v) => {
      setNote('');
      toast.success(t('Saved as version {number}', { number: v.number }));
      onSaved();
    },
  });
  return (
    <div className="grid gap-2">
      <Textarea
        value={note}
        onChange={(e) => setNote(e.target.value)}
        placeholder={t('What changed, for whoever reads the history (optional)')}
      />
      <div>
        <Button onClick={() => save.mutate({ templateId, note: note.trim() || null })} disabled={save.isPending}>
          <Save /> {t('Save Version')}
        </Button>
      </div>
    </div>
  );
}

function VersionDetail({
  doc,
  number,
  previous,
  onPreview,
  onRestored,
}: {
  doc: DocumentView;
  number: number;
  previous: number | null;
  onPreview: () => void;
  onRestored: (doc: DocumentView) => void;
}) {
  const version = useApi('templates.version', { templateId: doc.id, number });
  const changes = useApi('templates.compare', { templateId: doc.id, from: previous, to: number }, { enabled: previous !== null });
  const [confirming, setConfirming] = useState(false);
  const restore = useApiMutation('templates.restore', {
    invalidates: ['templates.', 'documents.'],
    onSuccess: (restored) => {
      setConfirming(false);
      toast.success(t('Restored version {number}', { number }));
      onRestored(restored);
    },
  });
  const v = version.data;
  if (!v) return <Loading />;
  return (
    <div className="grid gap-5">
      <div className="grid gap-1">
        <div className="flex flex-wrap items-center gap-2">
          <h3 className="text-base font-semibold">{versionLabel(v)}</h3>
          {v.restoredFrom && <Badge variant="outline">{t('Restored from version {number}', { number: v.restoredFrom })}</Badge>}
        </div>
        <VersionMeta version={v} />
        {v.note && <p className="whitespace-pre-wrap">{v.note}</p>}
      </div>
      <div className="flex flex-wrap gap-2">
        <Button variant="outline" onClick={onPreview} disabled={!v.content.files.length}>
          <Eye /> {t('Preview')}
        </Button>
        {doc.canEdit && (
          <Button variant="outline" onClick={() => setConfirming(true)}>
            <RotateCcw /> {t('Restore')}
          </Button>
        )}
      </div>
      <section className="grid gap-2">
        <h4 className="font-medium">{t('Contents')}</h4>
        <p className="text-muted-foreground">
          {t('{title} · Files: {files} · Parties: {parties} · Fields: {fields}', {
            title: v.content.title,
            files: v.content.files.length,
            parties: v.content.parties.length,
            fields: v.content.fields.length,
          })}
        </p>
      </section>
      {previous !== null && (
        <section className="grid gap-2">
          <h4 className="font-medium">{t('Changes From Version {number}', { number: previous })}</h4>
          <Changes changes={changes.data?.changes} />
        </section>
      )}
      <section className="grid gap-2">
        <h4 className="font-medium">{t('Documents From This Version')}</h4>
        {v.documents.length ? (
          <ul className="grid gap-1">
            {v.documents.map((d) => (
              <li key={d.id} className="flex items-center gap-2">
                <Link to={`/documents/${d.id}`} className="min-w-0 flex-1 truncate underline-offset-4 hover:underline">
                  {d.title}
                </Link>
                <StatusBadge status={d.status} />
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-muted-foreground">{t('None you can see.')}</p>
        )}
      </section>
      <Dialog open={confirming} onOpenChange={setConfirming}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('Restore Version {number}?', { number })}</DialogTitle>
            <DialogDescription>
              {t(
                'The template gets the files, parties, fields and settings it had then. The current state is kept as a version first, so you can go back.',
              )}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirming(false)}>
              {t('Cancel')}
            </Button>
            <Button disabled={restore.isPending} onClick={() => restore.mutate({ templateId: doc.id, number })}>
              {t('Restore')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/** The template's versions: what changed in each, the documents made from it, preview and restore. */
export function TemplateHistoryDialog({
  doc,
  open,
  onOpenChange,
  onRestored,
}: {
  doc: DocumentView;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onRestored: (doc: DocumentView) => void;
}) {
  const versions = useApi('templates.versions', { templateId: doc.id }, { enabled: open });
  const list = versions.data?.versions ?? [];
  const changed = versions.data?.changed ?? false;
  const [selected, setSelected] = useState<number | 'current' | null>(null);
  const [previewing, setPreviewing] = useState<number | null>(null);
  const shown = selected ?? (changed || !list.length ? 'current' : list[0]!.number);
  const unsaved = useApi(
    'templates.compare',
    { templateId: doc.id, from: list[0]?.number ?? null, to: null },
    { enabled: open && shown === 'current' && list.length > 0 },
  );
  const previous = (n: number) => list.find((v) => v.number < n)?.number ?? null;

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        onOpenChange(o);
        if (!o) setSelected(null);
      }}
    >
      <DialogContent className="flex max-h-[90svh] flex-col sm:max-w-4xl">
        <DialogHeader>
          <div className="flex items-center justify-between gap-2 pe-6">
            <DialogTitle>{t('Version History')}</DialogTitle>
            <HelpLink page="templates#versions" />
          </div>
          <DialogDescription>
            {t('Each document records the version it was made from. A version is saved automatically when a changed template is used.')}
          </DialogDescription>
        </DialogHeader>
        {versions.isPending ? (
          <Loading />
        ) : (
          <div className="grid min-h-0 flex-1 gap-4 overflow-hidden sm:grid-cols-[240px_minmax(0,1fr)]">
            <ul className="grid max-h-48 content-start gap-1 overflow-y-auto sm:max-h-none">
              {(changed || !list.length) && (
                <li>
                  <button
                    type="button"
                    onClick={() => setSelected('current')}
                    className={cn('w-full rounded-md px-3 py-2 text-start hover:bg-accent', shown === 'current' && 'bg-accent')}
                  >
                    <span className="block font-medium">{t('Current')}</span>
                    <span className="block text-xs text-muted-foreground">{t('Not saved as a version yet')}</span>
                  </button>
                </li>
              )}
              {list.map((v) => (
                <li key={v.number}>
                  <button
                    type="button"
                    onClick={() => setSelected(v.number)}
                    className={cn('w-full rounded-md px-3 py-2 text-start hover:bg-accent', shown === v.number && 'bg-accent')}
                  >
                    <span className="flex items-center gap-2 font-medium">
                      {versionLabel(v)}
                      {v === list[0] && !changed && <Badge variant="secondary">{t('Current')}</Badge>}
                    </span>
                    <VersionMeta version={v} />
                    {v.note && <span className="block truncate text-xs">{v.note}</span>}
                    {v.documentCount > 0 && (
                      <span className="block text-xs text-muted-foreground">
                        {v.documentCount === 1 ? t('1 document') : t('{count} documents', { count: v.documentCount })}
                      </span>
                    )}
                  </button>
                </li>
              ))}
            </ul>
            <div className="min-h-0 overflow-y-auto pe-1">
              {shown === 'current' ? (
                <div className="grid gap-5">
                  <h3 className="text-base font-semibold">{t('Current')}</h3>
                  {list.length > 0 && (
                    <section className="grid gap-2">
                      <h4 className="font-medium">{t('Changes Since Version {number}', { number: list[0]!.number })}</h4>
                      <Changes changes={unsaved.data?.changes} />
                    </section>
                  )}
                  {doc.canEdit ? (
                    <SaveVersion templateId={doc.id} onSaved={() => setSelected(null)} />
                  ) : (
                    <p className="text-muted-foreground">{t('Someone who can edit the template can save this as a version.')}</p>
                  )}
                </div>
              ) : (
                <VersionDetail
                  key={shown}
                  doc={doc}
                  number={shown}
                  previous={previous(shown)}
                  onPreview={() => setPreviewing(shown)}
                  onRestored={(restored) => {
                    setSelected(null);
                    onRestored(restored);
                  }}
                />
              )}
            </div>
          </div>
        )}
      </DialogContent>
      {previewing !== null && <VersionPreview doc={doc} number={previewing} onClose={() => setPreviewing(null)} />}
    </Dialog>
  );
}

function asParty(name: string, index: number): PartyView {
  return { id: `party-${index}`, name, role: 'signer' } as PartyView;
}

function VersionPreview({ doc, number, onClose }: { doc: DocumentView; number: number; onClose: () => void }) {
  const version = useApi('templates.version', { templateId: doc.id, number });
  const url = `/api/documents/${doc.id}/preview?version=${number}`;
  const loader = useMemo(() => () => fetchPdf(url), [url]);
  const pdf = usePdf(loader, url);
  const content = version.data?.content;
  const parties = useMemo(() => content?.parties.map((p, i) => asParty(p.name, i)) ?? [], [content]);
  const fields: EditableField[] = useMemo(
    () =>
      content?.fields.map((f, i) => ({
        ...f,
        key: String(i),
        kind: f.kind as FieldKind,
        label: f.label || null,
        partyId: f.party === null || f.party === undefined ? null : `party-${f.party}`,
        validation: null,
        conditions: [],
      })) ?? [],
    [content],
  );
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent
        showCloseButton={false}
        className="top-0 left-0 block h-svh max-w-none translate-x-0 translate-y-0 gap-0 overflow-y-auto rounded-none border-0 p-0 sm:max-w-none"
      >
        <div className="sticky top-0 z-10 flex items-center gap-3 border-b bg-background px-4 py-2.5 shadow-sm">
          <History className="size-4 text-muted-foreground" />
          <DialogTitle className="text-sm font-semibold">
            {content ? t('{title}, version {number}', { title: content.title, number }) : t('Version {number}', { number })}
          </DialogTitle>
          <DialogDescription className="min-w-0 flex-1 text-xs">
            {t('The template as it was. Nothing can be changed here.')}
          </DialogDescription>
          <Button variant="ghost" size="icon" aria-label={t('Close')} onClick={onClose}>
            <X />
          </Button>
        </div>
        <div className="mx-auto max-w-4xl p-4">
          {!content?.files.length && version.data ? (
            <EmptyState title={t('No files')} />
          ) : pdf.pages ? (
            <PdfPages
              pages={pdf.pages}
              overlay={(page) => (
                <FieldLayer
                  page={page}
                  fields={fields}
                  parties={parties}
                  editable={false}
                  selected={[]}
                  onSelect={() => {}}
                  onChange={() => {}}
                  onRemove={() => {}}
                />
              )}
            />
          ) : (
            <PdfLoading error={pdf.error} />
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
