import {
  Archive,
  ArchiveRestore,
  ArrowLeft,
  ChevronDown,
  ChevronUp,
  Copy,
  Download,
  Eye,
  Folder,
  HandHelping,
  History,
  Link2,
  MoreHorizontal,
  Paperclip,
  Pencil,
  Plus,
  RefreshCw,
  Scale,
  Send,
  Trash2,
  UserPlus,
  Users,
} from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router';
import { toast } from 'sonner';
import { ConfirmButton, CopyButton, EmptyState, Loading, Spinner } from '@/components/common';
import { PdfLoading, PdfPages, usePdf } from '@/components/PdfPages';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Textarea } from '@/components/ui/textarea';
import {
  type AttachmentRequest,
  type DocumentView,
  errorMessage,
  type FieldKind,
  fetchPdf,
  type PartyInput,
  type PartyView,
  replaceFile,
  type SignerLink,
  type SuggestedField,
  uploadFile,
  useApi,
  useApiMutation,
} from '@/lib/api';
import {
  AUTH_LABELS,
  DELIVERY_LABELS,
  formatBytes,
  formatDate,
  formatDateTime,
  languageName,
  languages,
  PARTY_STATUS_LABELS,
  saveBytes,
} from '@/lib/format';
import { t } from '@/lib/i18n';
import { useSession } from '@/lib/session';
import { cn } from '@/lib/utils';
import { ActivityPanel } from './ActivityPanel';
import { BulkSendDialog } from './BulkSendDialog';
import { StatusBadge } from './DocumentsPage';
import { ExportDataSubmenu } from './ExportData';
import { FieldLayer } from './FieldLayer';
import { FieldSettings } from './FieldSettings';
import { folderPath, MoveToSubmenu } from './Folders';
import { clamp, type EditableField, FIELD_KINDS, kindLabel, newGroupKey, partyColor, SENDER_KINDS, toEditable, toInput } from './fields';
import { FileDrop } from './NewDocumentDialog';
import { PartyDialog, partyToInput } from './PartyDialog';
import { PartyPhoto } from './PartyPhoto';
import { Checks, ReviewPanel } from './ReviewPanel';
import { SigningPreviewDialog } from './SigningPreviewDialog';
import { announceUpload, DetectButton, SuggestionBar, SuggestionLayer } from './SuggestedFields';
import { TemplateHistoryDialog } from './TemplateHistory';

function Section({ title, action, children }: { title: string; action?: React.ReactNode; children: React.ReactNode }) {
  return (
    <section className="grid gap-2.5">
      <div className="flex items-center justify-between">
        <h3 className="font-medium">{title}</h3>
        {action}
      </div>
      {children}
    </section>
  );
}

function useFields(doc: DocumentView | undefined, editable: boolean) {
  const [fields, setFields] = useState<EditableField[]>([]);
  const loaded = useRef<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const save = useApiMutation('documents.set_fields', { invalidates: ['documents.check', 'templates.versions'] });

  useEffect(() => {
    if (doc && loaded.current !== doc.id) {
      loaded.current = doc.id;
      setFields(toEditable(doc.fields));
    }
  }, [doc]);

  const commit = useCallback(
    (next: EditableField[]) => {
      if (!doc || !editable) return;
      clearTimeout(timer.current);
      timer.current = setTimeout(() => {
        timer.current = undefined;
        save.mutate({ documentId: doc.id, fields: toInput(next) });
      }, 500);
    },
    [doc, editable, save],
  );
  const update = (next: EditableField[], persist = true) => {
    setFields(next);
    if (persist) commit(next);
  };
  const reload = (doc: DocumentView) => setFields(toEditable(doc.fields));
  const flush = async () => {
    if (!timer.current || !doc || !editable) return;
    clearTimeout(timer.current);
    timer.current = undefined;
    await save.mutateAsync({ documentId: doc.id, fields: toInput(fields) });
  };
  return { fields, update, reload, flush, saving: save.isPending };
}

function PartyCard({
  party,
  parties,
  editable,
  pending,
  documentId,
  attachments,
  onEdit,
  onRemove,
  step,
  onMove,
}: {
  party: PartyView;
  parties: PartyView[];
  editable: boolean;
  pending: boolean;
  /** The party's step when signing in order. */
  step?: number;
  /** Moves the party one step earlier (-1) or later (1); absent at that end of the list. */
  onMove?: { up?: () => void; down?: () => void };
  documentId: string;
  attachments: AttachmentRequest[];
  onEdit: () => void;
  onRemove: () => void;
}) {
  const [link, setLink] = useState<SignerLink | null>(null);
  const signingLink = useApiMutation('documents.signing_link', { onSuccess: setLink });
  const inPerson = useApiMutation('documents.sign_in_person', { onSuccess: (l) => window.location.assign(l.url) });
  const done = party.status === 'signed' || party.status === 'declined';
  return (
    <div className="rounded-md border p-3">
      <div className="flex items-start gap-2.5">
        {step ? (
          <span
            className="flex size-5 shrink-0 items-center justify-center rounded-full font-semibold text-[11px] text-white"
            style={{ backgroundColor: partyColor(parties, party.id) }}
            title={`Step ${step} in the signing order`}
          >
            {step}
          </span>
        ) : (
          <span className="mt-1 size-2.5 shrink-0 rounded-full" style={{ backgroundColor: partyColor(parties, party.id) }} />
        )}
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate font-medium">{party.name}</span>
            {!editable && (
              <span
                className={cn(
                  'ms-auto shrink-0 text-xs',
                  party.status === 'signed' ? 'text-success' : party.status === 'declined' ? 'text-destructive' : 'text-muted-foreground',
                )}
              >
                {PARTY_STATUS_LABELS[party.status] ?? party.status}
              </span>
            )}
          </div>
          <div className="truncate text-xs text-muted-foreground">
            {[party.email, party.phone, party.company].filter(Boolean).join(' · ') || t('No contact details')}
          </div>
          <div className="mt-1 text-xs text-muted-foreground">
            {party.role === 'viewer' ? t('Receives a copy') : party.role === 'approver' ? t('Approves') : t('Signs')} ·{' '}
            {DELIVERY_LABELS[party.delivery]?.replace(/ \(.*\)$/, '').replace(/（.*）$/, '')} · {AUTH_LABELS[party.authToSign]}
            {party.authToView !== 'none' && ` · ${t('{method} to view', { method: AUTH_LABELS[party.authToView] ?? party.authToView })}`}
            {party.language && ` · ${languageName(party.language)}`}
            {party.webcamPhoto && ` · ${t('Webcam photo')}`}
          </div>
          {party.declineReason && <div className="mt-1 text-xs text-destructive">“{party.declineReason}”</div>}
          {attachments
            .filter((a) => a.kind === 'photo')
            .map((a) => (
              <PartyPhoto key={a.id} documentId={documentId} request={a} />
            ))}
          {attachments.some((a) => a.kind !== 'photo') && (
            <ul className="mt-1.5 grid gap-0.5 text-xs">
              {attachments
                .filter((a) => a.kind !== 'photo')
                .map((a) => (
                  <li key={a.id} className="flex items-center gap-1.5">
                    <Paperclip className="size-3 shrink-0 text-muted-foreground" />
                    <span className="min-w-0 truncate">
                      {a.name}
                      {!a.required && <span className="text-muted-foreground"> {t('(optional)')}</span>}
                    </span>
                    {a.fileId ? (
                      <button
                        type="button"
                        className="ms-auto shrink-0 underline"
                        onClick={() =>
                          fetchPdf(`/api/documents/${documentId}/attachments/${a.id}`)
                            .then((bytes) => saveBytes(bytes, a.fileName ?? 'attachment', a.contentType ?? 'application/octet-stream'))
                            .catch((e) => toast.error(errorMessage(e)))
                        }
                      >
                        {t('Download')}
                      </button>
                    ) : (
                      <span className="ms-auto shrink-0 text-muted-foreground">{t('Not uploaded')}</span>
                    )}
                  </li>
                ))}
            </ul>
          )}
        </div>
        {editable && onMove && (
          <div className="-my-1 flex flex-col">
            <Button
              variant="ghost"
              size="icon-sm"
              className="h-5"
              disabled={!onMove.up}
              onClick={onMove.up}
              aria-label={`Move ${party.name} earlier`}
            >
              <ChevronUp />
            </Button>
            <Button
              variant="ghost"
              size="icon-sm"
              className="h-5"
              disabled={!onMove.down}
              onClick={onMove.down}
              aria-label={`Move ${party.name} later`}
            >
              <ChevronDown />
            </Button>
          </div>
        )}
        {editable && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="icon-sm" aria-label={t('Options for {name}', { name: party.name })}>
                <MoreHorizontal />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onSelect={onEdit}>
                <Pencil /> {t('Edit')}
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={onRemove} className="text-destructive">
                <Trash2 /> {t('Remove')}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        )}
      </div>
      {pending && !done && party.role !== 'viewer' && (
        <div className="mt-2 flex flex-wrap gap-2">
          {link ? (
            <div className="grid w-full gap-1.5">
              <Input readOnly value={link.url} className="font-mono text-xs" onFocus={(e) => e.target.select()} />
              <div className="flex gap-2">
                <CopyButton value={link.url} label={t('Copy Link')} />
                <Button variant="ghost" size="sm" onClick={() => setLink(null)}>
                  {t('Hide')}
                </Button>
              </div>
            </div>
          ) : (
            <>
              {party.delivery === 'in_person' && (
                <Button size="sm" disabled={inPerson.isPending} onClick={() => inPerson.mutate({ documentId, partyId: party.id })}>
                  <HandHelping /> {t('Sign in Person')}
                </Button>
              )}
              <Button
                variant="outline"
                size="sm"
                disabled={signingLink.isPending}
                onClick={() => signingLink.mutate({ documentId, partyId: party.id, rotate: false })}
              >
                <Link2 /> {t('Signing Link')}
              </Button>
              <ConfirmButton
                label={t('New Link')}
                title={t('Replace the signing link?')}
                description={t('The earlier links for {name} stop working. Use this when a link reached the wrong person.', {
                  name: party.name,
                })}
                confirmLabel={t('Replace Link')}
                onConfirm={() => signingLink.mutate({ documentId, partyId: party.id, rotate: true })}
              />
            </>
          )}
        </div>
      )}
    </div>
  );
}

function SendDialog({
  doc,
  open,
  onOpenChange,
  onSent,
}: {
  doc: DocumentView;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSent: (links: SignerLink[]) => void;
}) {
  const checks = useApi('documents.check', { id: doc.id }, { enabled: open, staleTime: 0 });
  const [expires, setExpires] = useState('');
  const send = useApiMutation('documents.send', {
    invalidates: ['documents.'],
    onSuccess: (out) => {
      onOpenChange(false);
      onSent(out.links);
      toast.success(t('Sent for signing'));
    },
  });
  const blocked = checks.data?.some((c) => c.severity === 'error');
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('Send for Signing')}</DialogTitle>
          <DialogDescription>
            {doc.sequential
              ? t('The document is locked once sent. Parties who get their link by email or SMS are notified in signing order.')
              : t('The document is locked once sent. Parties who get their link by email or SMS are notified now.')}
          </DialogDescription>
        </DialogHeader>
        {checks.data ? <Checks checks={checks.data} /> : <Spinner />}
        <div className="grid gap-2">
          <Label htmlFor="expires">{t('Expires')}</Label>
          <Input
            id="expires"
            type="date"
            value={expires}
            onChange={(e) => setExpires(e.target.value)}
            min={new Date().toISOString().slice(0, 10)}
          />
          <p className="text-xs text-muted-foreground">{t('Leave empty for the default from Settings.')}</p>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t('Cancel')}
          </Button>
          <Button
            disabled={blocked || send.isPending || !checks.data}
            onClick={() => send.mutate({ id: doc.id, expiresAt: expires ? new Date(`${expires}T23:59:59`).toISOString() : null })}
          >
            {send.isPending ? <Spinner className="text-primary-foreground" /> : <Send />} {t('Send')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function LinksDialog({ links, onClose }: { links: SignerLink[]; onClose: () => void }) {
  return (
    <Dialog open={links.length > 0} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{t('Hand Over the Signing Links')}</DialogTitle>
          <DialogDescription>
            {t('These parties get their link from you. Each link works only for that person; send it privately.')}
          </DialogDescription>
        </DialogHeader>
        <div className="grid gap-3">
          {links.map((l) => (
            <div key={l.partyId} className="grid gap-1.5">
              <Label>{l.name}</Label>
              <div className="flex gap-2">
                <Input readOnly value={l.url} className="font-mono text-xs" onFocus={(e) => e.target.select()} />
                <CopyButton value={l.url} />
              </div>
            </div>
          ))}
        </div>
        <DialogFooter>
          <Button onClick={onClose}>{t('Done')}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function DetailsSection({ doc }: { doc: DocumentView }) {
  const update = useApiMutation('documents.update', { invalidates: ['documents.', 'templates.versions'] });
  const [message, setMessage] = useState(doc.message);
  const [externalId, setExternalId] = useState(doc.externalId ?? '');
  const [reminder, setReminder] = useState(String(doc.reminderDays ?? 0));
  return (
    <Section title={t('Details')}>
      <div className="grid gap-1.5">
        <Label htmlFor="message">{t('Message to Parties')}</Label>
        <Textarea
          id="message"
          rows={3}
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          onBlur={() => message !== doc.message && update.mutate({ id: doc.id, message })}
          placeholder={t('Shown in the invitation and on the signing page')}
        />
      </div>
      <div className="grid grid-cols-2 gap-3">
        <div className="grid gap-1.5">
          <Label htmlFor="language">{t('Language for Signers')}</Label>
          <Select value={doc.language} onValueChange={(language) => update.mutate({ id: doc.id, language })}>
            <SelectTrigger id="language" className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {languages().map(([code, name]) => (
                <SelectItem key={code} value={code}>
                  {name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="grid gap-1.5">
          <Label htmlFor="reminders">{t('Remind Every (Days)')}</Label>
          <Input
            id="reminders"
            type="number"
            min={0}
            max={60}
            value={reminder}
            onChange={(e) => setReminder(e.target.value)}
            onBlur={() => update.mutate({ id: doc.id, reminderDays: Number(reminder) || 0 })}
          />
        </div>
        <div className="grid gap-1.5">
          <Label htmlFor="external">{t('Your Reference')}</Label>
          <Input
            id="external"
            value={externalId}
            onChange={(e) => setExternalId(e.target.value)}
            onBlur={() => externalId !== (doc.externalId ?? '') && update.mutate({ id: doc.id, externalId })}
            className="font-mono"
          />
        </div>
      </div>
    </Section>
  );
}

function EditableTitle({ doc, editable }: { doc: DocumentView; editable: boolean }) {
  const [title, setTitle] = useState(doc.title);
  const update = useApiMutation('documents.update', { invalidates: ['documents.', 'templates.versions'] });
  useEffect(() => setTitle(doc.title), [doc.title]);
  if (!editable) return <h1 className="truncate text-xl font-semibold tracking-tight">{doc.title}</h1>;
  return (
    <input
      aria-label={t('Title')}
      value={title}
      onChange={(e) => setTitle(e.target.value)}
      onBlur={() => title.trim() && title !== doc.title && update.mutate({ id: doc.id, title: title.trim() })}
      onKeyDown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
      className="w-full min-w-0 truncate rounded-md bg-transparent px-1 -mx-1 text-xl font-semibold tracking-tight outline-none hover:bg-accent focus:bg-accent"
    />
  );
}

const FIELD_DRAG_TYPE = 'application/x-opensign-field';

function ReplaceFileButton({ name, busy, onFile }: { name: string; busy: boolean; onFile: (file: File) => void }) {
  const input = useRef<HTMLInputElement>(null);
  return (
    <>
      <input
        ref={input}
        type="file"
        hidden
        accept=".pdf,.docx,.doc,.odt,.rtf,application/pdf"
        onChange={(e) => {
          const file = e.target.files?.[0];
          e.target.value = '';
          if (file) onFile(file);
        }}
      />
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={t('Replace {name}', { name })}
        title={t('Replace with a new version of the file. Fields stay where they are.')}
        disabled={busy}
        onClick={() => input.current?.click()}
      >
        <RefreshCw />
      </Button>
    </>
  );
}

function AiReviewSection({ doc }: { doc: DocumentView }) {
  const set = useApiMutation('documents.set_review_links', { invalidates: ['documents.', 'templates.versions'] });
  return (
    <Section title={t('AI Review by Signers')}>
      <label className="flex items-center justify-between gap-3 rounded-md bg-muted/50 px-3 py-2">
        <span>
          <span className="block">{t('Let Signers Share With Their AI')}</span>
          <span className="block text-xs text-muted-foreground">
            {t(
              'A read-only link for their own assistant, such as ChatGPT or Claude. It cannot sign, expires within days and every use is recorded in the audit trail. Turning this off revokes links already shared.',
            )}
          </span>
        </span>
        <Switch checked={doc.reviewLinks} disabled={set.isPending} onCheckedChange={(enabled) => set.mutate({ id: doc.id, enabled })} />
      </label>
    </Section>
  );
}

export function DocumentPage() {
  const { id = '' } = useParams();
  const navigate = useNavigate();
  const query = useApi('documents.get', { id });
  const authMethods = useApi('documents.auth_methods');
  const doc = query.data;
  const folders = useApi('folders.list', { scope: doc?.isTemplate ? 'templates' : 'documents' }, { enabled: !!doc });
  const draft = !!doc && doc.status === 'draft' && doc.canEdit;
  const pending = !!doc && doc.status === 'pending';
  const { fields, update, reload, flush, saving } = useFields(doc, draft);
  const [suggestions, setSuggestions] = useState<SuggestedField[]>([]);
  const [aiNote, setAiNote] = useState<string | null>(null);
  const aiStatus = useApi('ai.status', undefined, { staleTime: 5 * 60_000 });
  const [selected, setSelected] = useState<string | null>(null);
  const [armed, setArmed] = useState<FieldKind | null>(null);
  const [placingFor, setPlacingFor] = useState<string | null>(null);
  const [partyDialog, setPartyDialog] = useState<{ open: boolean; party: PartyView | null }>({ open: false, party: null });
  const [sending, setSending] = useState(false);
  const [bulkSending, setBulkSending] = useState(false);
  const { can } = useSession();
  const [links, setLinks] = useState<SignerLink[]>([]);
  const [uploading, setUploading] = useState(false);
  const [cancelReason, setCancelReason] = useState('');
  const [canceling, setCanceling] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [holding, setHolding] = useState(false);
  const [holdReason, setHoldReason] = useState('');
  const [showActivity, setShowActivity] = useState(false);
  const [previewing, setPreviewing] = useState(false);
  const [showHistory, setShowHistory] = useState(false);
  const versions = useApi('templates.versions', { templateId: id }, { enabled: !!doc?.isTemplate });

  const setParties = useApiMutation('documents.set_parties', { invalidates: ['documents.', 'templates.versions'] });
  const setSequential = useApiMutation('documents.update', { invalidates: ['documents.', 'templates.versions'] });
  const removeFile = useApiMutation('documents.remove_file', { invalidates: ['documents.', 'templates.versions'] });
  const detect = useApiMutation('documents.detect_fields', {
    onSuccess: (out) => {
      setSuggestions(out.suggestions);
      setAiNote(out.aiNote ?? null);
      if (!out.suggestions.length) toast.info(out.aiNote ?? t('No places for new fields were found'));
    },
  });
  const remind = useApiMutation('documents.remind', { onSuccess: () => toast.success(t('Reminders sent')) });
  const cancel = useApiMutation('documents.cancel', { invalidates: ['documents.'], onSuccess: () => setCanceling(false) });
  const remove = useApiMutation('documents.delete', {
    invalidates: ['documents.list'],
    onSuccess: () => navigate(doc?.isTemplate ? '/templates' : '/documents'),
  });
  const fromTemplate = useApiMutation('documents.create', {
    invalidates: ['templates.'],
    onSuccess: (d) => navigate(`/documents/${d.id}`),
  });
  const duplicate = useApiMutation('documents.create', {
    invalidates: ['documents.list', 'templates.'],
    onSuccess: (d) => {
      toast.success(t('Template duplicated'));
      navigate(`/documents/${d.id}`);
    },
  });
  const move = useApiMutation('documents.move', { invalidates: ['documents.', 'folders.'], onSuccess: () => toast.success(t('Moved')) });
  const archive = useApiMutation('documents.archive', {
    invalidates: ['documents.', 'folders.'],
    onSuccess: (_, input) => toast.success(input.archived ? t('Archived') : t('Restored from the archive')),
  });
  const legalHold = useApiMutation('documents.legal_hold', {
    invalidates: ['documents.'],
    onSuccess: (out) => {
      setHolding(false);
      setHoldReason('');
      toast.success(out.legalHold ? t('The document is on legal hold') : t('The legal hold is released'));
    },
  });

  const filesKey = doc ? `${doc.id}:${doc.status}:${doc.files.map((f) => f.id).join(',')}` : '';
  const pdfUrl = !doc?.files.length
    ? null
    : doc.status === 'draft'
      ? `/api/documents/${id}/preview`
      : `/api/documents/${id}/file?version=${doc.status === 'completed' ? 'signed' : 'original'}`;
  const loader = useMemo(() => (pdfUrl ? () => fetchPdf(pdfUrl) : null), [pdfUrl]);
  const pdf = usePdf(loader, filesKey);

  // biome-ignore lint/correctness/useExhaustiveDependencies: the server already unassigned them; mirror it locally
  useEffect(() => {
    if (!doc) return;
    const ids = new Set(doc.parties.map((p) => p.id));
    if (fields.some((f) => f.partyId && !ids.has(f.partyId))) {
      update(
        fields.map((f) => (f.partyId && !ids.has(f.partyId) ? { ...f, partyId: null } : f)),
        false,
      );
    }
  }, [doc?.parties]);
  useEffect(() => {
    if (doc && placingFor !== null && !doc.parties.some((p) => p.id === placingFor)) setPlacingFor(null);
  }, [doc, placingFor]);
  useEffect(() => {
    if (!draft || !selected) return;
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest('input, textarea, select, [contenteditable="true"], [role="dialog"]')) return;
      if (e.key === 'Escape') setSelected(null);
      else if (e.key === 'Delete' || e.key === 'Backspace') {
        e.preventDefault();
        update(fields.filter((f) => f.key !== selected));
        setSelected(null);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [draft, selected, fields, update]);
  // biome-ignore lint/correctness/useExhaustiveDependencies: only when the parties change
  useEffect(() => {
    if (doc && placingFor === null) setPlacingFor(doc.parties.find((p) => p.role !== 'viewer')?.id ?? null);
  }, [doc?.parties.length]);

  if (query.isPending) return <Loading />;
  if (!doc)
    return (
      <EmptyState title={t('This document is not available')}>{t('It may have been deleted, or it was not shared with you.')}</EmptyState>
    );

  const saveParties = (next: PartyInput[]) => setParties.mutate({ documentId: doc.id, parties: next });
  const allParties = () => doc.parties.map((p) => partyToInput(p));
  const signers = doc.parties.filter((p) => p.role !== 'viewer');
  const orderedParties = [...doc.parties].sort(
    (a, b) => Number(a.role === 'viewer') - Number(b.role === 'viewer') || a.signOrder - b.signOrder,
  );
  // Parties sharing a number sign together, so steps count distinct numbers.
  const steps = new Map([...new Set(signers.map((p) => p.signOrder))].sort((a, b) => a - b).map((order, i) => [order, i + 1]));
  const moves = (id: string) => {
    const order = orderedParties.filter((p) => p.role !== 'viewer').map((p) => p.id);
    const i = order.indexOf(id);
    const to = (j: number) => () => {
      const next = [...order];
      [next[i], next[j]] = [next[j]!, next[i]!];
      saveParties(allParties().map((p) => (p.id && next.includes(p.id) ? { ...p, signOrder: next.indexOf(p.id) + 1 } : p)));
    };
    return { up: i > 0 ? to(i - 1) : undefined, down: i < order.length - 1 ? to(i + 1) : undefined };
  };

  const place = (page: number, x: number, y: number, kind: FieldKind | null = armed) => {
    if (!draft) return;
    if (!kind) return setSelected(null);
    const spec = FIELD_KINDS.find((k) => k.kind === kind);
    if (!spec) return;
    const field: EditableField = {
      key: crypto.randomUUID(),
      kind,
      page,
      partyId: placingFor,
      required: kind !== 'checkbox' && kind !== 'formula',
      formula: kind === 'formula' ? '0' : null,
      options:
        kind === 'select' || kind === 'multiselect'
          ? [t('Option {n}', { n: 1 }), t('Option {n}', { n: 2 })]
          : kind === 'radio'
            ? [t('Option {n}', { n: 1 })]
            : [],
      groupKey: kind === 'radio' ? newGroupKey() : null,
      x: clamp(x - spec.width / 2, 0, 1 - spec.width),
      y: clamp(y - spec.height / 2, 0, 1 - spec.height),
      width: spec.width,
      height: spec.height,
    };
    update([...fields, field]);
    setSelected(field.key);
    setArmed(null);
  };

  const upload = async (files: File[]) => {
    setUploading(true);
    try {
      await flush();
      for (const file of files) announceUpload(file.name, await uploadFile(doc.id, file));
      const fresh = await query.refetch();
      if (fresh.data) reload(fresh.data);
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setUploading(false);
    }
  };

  const replace = async (fileId: string, file: File) => {
    setUploading(true);
    try {
      await flush();
      const out = await replaceFile(doc.id, fileId, file);
      announceUpload(file.name, out);
      if (out.moved.length)
        toast.warning(
          t('{count} fields were on pages the new file does not have and are now on its last page: {fields}', {
            count: out.moved.length,
            fields: out.moved.map((m) => m.label || kindLabel(m.kind)).join(', '),
          }),
          { duration: 15000 },
        );
      else if (!out.fields.textTags) toast.success(t('File replaced'));
      const next = await query.refetch();
      if (next.data) reload(next.data);
      void versions.refetch();
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setUploading(false);
    }
  };

  const download = async (version: 'signed' | 'original') => {
    try {
      const bytes = await fetchPdf(`/api/documents/${doc.id}/file?version=${version}`);
      saveBytes(bytes, `${doc.title}${version === 'original' ? ' (original)' : ''}.pdf`);
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  const selectedField = fields.find((f) => f.key === selected);

  const toField = (s: SuggestedField): EditableField | null => {
    const signer = doc.parties.find((p) => p.role === 'signer')?.id ?? null;
    const actor = doc.parties.find((p) => p.role !== 'viewer')?.id ?? null;
    const needsSigner = s.kind === 'signature' || s.kind === 'initials';
    const placing = doc.parties.find((p) => p.id === placingFor);
    const partyId = needsSigner
      ? placing?.role === 'signer'
        ? placing.id
        : signer
      : (placingFor ?? (SENDER_KINDS.includes(s.kind) ? null : actor));
    if (needsSigner && !partyId) return null;
    return {
      key: crypto.randomUUID(),
      kind: s.kind,
      label: s.label || null,
      page: s.page,
      partyId,
      required: s.kind !== 'checkbox',
      options: [],
      groupKey: null,
      x: s.x,
      y: s.y,
      width: s.width,
      height: s.height,
    };
  };
  const accept = (picked: SuggestedField[]) => {
    const added = picked.map(toField);
    if (added.some((f) => !f)) toast.warning(t('Add a signer first: signature and initials fields belong to a signer'));
    const ok = added.filter((f): f is EditableField => !!f);
    if (ok.length) update([...fields, ...ok]);
    setSuggestions((all) => all.filter((s) => !picked.includes(s) || !toField(s)));
    if (ok.length === 1) setSelected(ok[0]!.key);
  };

  return (
    <div className="grid gap-5">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <Button asChild variant="ghost" size="icon" aria-label={t('Back')}>
          <Link to={doc.isTemplate ? '/templates' : '/documents'}>
            <ArrowLeft className="rtl:rotate-180" />
          </Link>
        </Button>
        <div className="min-w-0 flex-1">
          <EditableTitle doc={doc} editable={doc.canEdit && (draft || doc.isTemplate)} />
          <div className="mt-0.5 flex items-center gap-2 text-xs text-muted-foreground">
            {!doc.isTemplate && <StatusBadge status={doc.status} />}
            <span>
              {doc.sentAt
                ? t('Sent {date}', { date: formatDateTime(doc.sentAt) })
                : t('Created {date}', { date: formatDateTime(doc.createdAt) })}
            </span>
            {doc.expiresAt && pending && <span>· Expires {formatDateTime(doc.expiresAt)}</span>}
            {doc.isTemplate && versions.data && (
              <button type="button" className="hover:text-foreground hover:underline" onClick={() => setShowHistory(true)}>
                ·{' '}
                {versions.data.versions.length === 0
                  ? t('No versions yet')
                  : versions.data.changed
                    ? t('Changed since version {number}', { number: versions.data.versions[0]!.number })
                    : t('Version {number}', { number: versions.data.versions[0]!.number })}
              </button>
            )}
            {doc.templateId && doc.templateTitle && (
              <span className="inline-flex min-w-0 items-center gap-1">
                ·{' '}
                <Link to={`/documents/${doc.templateId}`} className="truncate hover:text-foreground hover:underline">
                  {doc.templateVersion
                    ? t('From {template}, version {number}', { template: doc.templateTitle, number: doc.templateVersion })
                    : t('From {template}', { template: doc.templateTitle })}
                </Link>
              </span>
            )}
            {doc.legalHold && (
              <span className="inline-flex items-center gap-1 font-medium text-warning" title={doc.legalHoldReason ?? undefined}>
                · <Scale className="size-3.5" /> {t('Legal Hold')}
              </span>
            )}
            {doc.folderId && folders.data && (
              <span className="inline-flex min-w-0 items-center gap-1">
                · <Folder className="size-3.5 shrink-0" /> <span className="truncate">{folderPath(folders.data, doc.folderId)}</span>
              </span>
            )}
            {doc.archivedAt && (
              <span className="inline-flex items-center gap-1 font-medium">
                · <Archive className="size-3.5" /> {t('Archived')}
              </span>
            )}
            {doc.deletesAt && <span>· Kept until {formatDate(doc.deletesAt)}</span>}
            {saving && <span>{t('· Saving…')}</span>}
          </div>
        </div>
        <div className="flex flex-wrap gap-2">
          {doc.isTemplate && !doc.archivedAt && (
            <Button onClick={() => fromTemplate.mutate({ title: doc.title, templateId: doc.id })} disabled={fromTemplate.isPending}>
              <Plus /> {t('Use Template')}
            </Button>
          )}
          {doc.isTemplate && !doc.archivedAt && can('documents.create') && (
            <Button variant="outline" onClick={() => setBulkSending(true)}>
              <Users /> {t('Bulk Send')}
            </Button>
          )}
          {doc.files.length > 0 && doc.status !== 'completed' && (
            <Button
              variant="outline"
              onClick={() => {
                flush();
                setPreviewing(true);
              }}
              disabled={!doc.parties.length}
              title={doc.parties.length ? t('See the signing page as a party will') : t('Add a party to preview the signing page')}
            >
              <Eye /> {t('Preview')}
            </Button>
          )}
          {draft && !doc.isTemplate && (
            <Button onClick={() => setSending(true)} disabled={!doc.files.length}>
              <Send /> {t('Send')}
            </Button>
          )}
          {pending && doc.canEdit && (
            <Button variant="outline" onClick={() => remind.mutate({ id: doc.id })} disabled={remind.isPending}>
              {t('Remind')}
            </Button>
          )}
          {doc.status === 'completed' && (
            <Button onClick={() => download('signed')}>
              <Download /> {t('Signed PDF')}
            </Button>
          )}
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="outline" size="icon" aria-label={t('More')}>
                <MoreHorizontal />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onSelect={() => setShowActivity(true)}>
                <History /> {t('Activity')}
              </DropdownMenuItem>
              {doc.isTemplate && (
                <DropdownMenuItem
                  onSelect={() => {
                    flush();
                    setShowHistory(true);
                  }}
                >
                  <History /> {t('Version History')}
                </DropdownMenuItem>
              )}
              {doc.isTemplate && can('templates.manage') && (
                <DropdownMenuItem
                  disabled={duplicate.isPending}
                  onSelect={() => {
                    flush();
                    duplicate.mutate({
                      title: t('{title} (copy)', { title: doc.title }),
                      templateId: doc.id,
                      isTemplate: true,
                      folderId: doc.folderId ?? null,
                    });
                  }}
                >
                  <Copy /> {t('Duplicate')}
                </DropdownMenuItem>
              )}
              {doc.status !== 'draft' && (
                <DropdownMenuItem onSelect={() => download('original')}>
                  <Download /> {t('Original PDF')}
                </DropdownMenuItem>
              )}
              {can('documents.export') && (
                <ExportDataSubmenu filter={doc.isTemplate ? { templateId: doc.id, archived: null } : { ids: [doc.id], archived: null }} />
              )}
              {pending && doc.canEdit && (
                <DropdownMenuItem onSelect={() => setCanceling(true)} className="text-destructive">
                  {t('Cancel Signing')}
                </DropdownMenuItem>
              )}
              {doc.canEdit && folders.data && (
                <MoveToSubmenu
                  folders={folders.data}
                  current={doc.folderId ?? null}
                  onMove={(folderId) => move.mutate({ ids: [doc.id], folderId })}
                />
              )}
              {doc.canEdit && (doc.isTemplate || ['completed', 'declined', 'canceled', 'expired'].includes(doc.status)) && (
                <DropdownMenuItem onSelect={() => archive.mutate({ ids: [doc.id], archived: !doc.archivedAt })}>
                  {doc.archivedAt ? <ArchiveRestore /> : <Archive />} {doc.archivedAt ? t('Restore') : t('Archive')}
                </DropdownMenuItem>
              )}
              {!doc.isTemplate && can('documents.legal_hold') && (
                <DropdownMenuItem onSelect={() => (doc.legalHold ? legalHold.mutate({ id: doc.id, hold: false }) : setHolding(true))}>
                  <Scale /> {doc.legalHold ? t('Release Legal Hold') : t('Place on Legal Hold')}
                </DropdownMenuItem>
              )}
              {doc.canEdit && !pending && !doc.legalHold && (
                <>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem onSelect={() => setDeleting(true)} className="text-destructive">
                    <Trash2 /> {t('Delete')}
                  </DropdownMenuItem>
                </>
              )}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>

      <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_340px]">
        <div className="min-w-0">
          {draft && doc.files.length > 0 && (
            <div className="sticky top-3 z-10 mb-3 flex flex-wrap items-center gap-1.5 rounded-lg border bg-background/95 p-2 backdrop-blur">
              <select
                aria-label={t('Fields for')}
                value={placingFor ?? ''}
                onChange={(e) => setPlacingFor(e.target.value || null)}
                className="h-8 rounded-md border bg-background px-2 text-[13px]"
              >
                {doc.parties
                  .filter((p) => p.role !== 'viewer')
                  .map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                <option value="">{t('Me, now (prefilled)')}</option>
              </select>
              {FIELD_KINDS.filter((k) => placingFor || SENDER_KINDS.includes(k.kind)).map((k) => (
                <Button
                  key={k.kind}
                  size="sm"
                  variant={armed === k.kind ? 'default' : 'ghost'}
                  onClick={() => setArmed(armed === k.kind ? null : k.kind)}
                  draggable
                  onDragStart={(e) => {
                    e.dataTransfer.setData(FIELD_DRAG_TYPE, k.kind);
                    e.dataTransfer.effectAllowed = 'copy';
                  }}
                  className="cursor-grab active:cursor-grabbing"
                  style={armed === k.kind ? undefined : { color: partyColor(doc.parties, placingFor) }}
                >
                  {k.label}
                </Button>
              ))}
              <span className="ms-auto text-xs text-muted-foreground">
                {armed ? t('Click on the page to place it') : t('Drag a field onto the page, or pick one and click')}
              </span>
              <DetectButton
                busy={detect.isPending}
                aiReady={!!aiStatus.data?.configured && aiStatus.data.allowDocumentText && aiStatus.data.canUse}
                onDetect={async (ai) => {
                  await flush();
                  detect.mutate({ documentId: doc.id, ai });
                }}
              />
            </div>
          )}
          {!doc.files.length ? (
            draft || doc.isTemplate ? (
              <FileDrop onFiles={upload} busy={uploading} />
            ) : (
              <EmptyState title={t('No files')} />
            )
          ) : pdf.pages ? (
            <div className={cn(armed && '[&_canvas]:cursor-crosshair')}>
              {draft && suggestions.length > 0 && (
                <SuggestionBar
                  count={suggestions.length}
                  note={aiNote}
                  onAcceptAll={() => accept(suggestions)}
                  onDismissAll={() => setSuggestions([])}
                />
              )}
              <PdfPages
                pages={pdf.pages}
                onPageClick={draft ? place : undefined}
                dropType={draft ? FIELD_DRAG_TYPE : undefined}
                onPageDrop={(page, x, y, kind) => place(page, x, y, kind as FieldKind)}
                // The sealed copy has the values drawn into it.
                overlay={(page) =>
                  doc.status !== 'completed' && (
                    <>
                      <FieldLayer
                        page={page}
                        fields={fields}
                        parties={doc.parties}
                        editable={draft}
                        selected={selected}
                        onSelect={setSelected}
                        onChange={(key, patch, done) =>
                          update(
                            fields.map((f) => (f.key === key ? { ...f, ...patch } : f)),
                            done,
                          )
                        }
                        onRemove={(key) => {
                          update(fields.filter((f) => f.key !== key));
                          setSelected(null);
                        }}
                      />
                      {draft && (
                        <SuggestionLayer
                          page={page}
                          suggestions={suggestions}
                          onAccept={(s) => accept([s])}
                          onDismiss={(s) => setSuggestions((all) => all.filter((x) => x !== s))}
                        />
                      )}
                    </>
                  )
                }
              />
            </div>
          ) : (
            <PdfLoading error={pdf.error} />
          )}
        </div>

        <aside className="min-w-0 lg:sticky lg:top-3 lg:max-h-[calc(100svh-1.5rem)] lg:self-start lg:overflow-y-auto lg:pb-4 [scrollbar-width:thin]">
          {draft && selectedField && (
            <div className="mb-4">
              <FieldSettings
                key={selectedField.key}
                field={selectedField}
                fields={fields}
                parties={doc.parties}
                onChange={update}
                onRemove={() => {
                  update(fields.filter((f) => f.key !== selectedField.key));
                  setSelected(null);
                }}
              />
            </div>
          )}
          <Tabs defaultValue="setup">
            <TabsList className="w-full">
              <TabsTrigger value="setup">{draft ? t('Setup') : t('Parties')}</TabsTrigger>
              <TabsTrigger value="review">{t('Review')}</TabsTrigger>
            </TabsList>
            <TabsContent value="setup" className="mt-4 grid gap-6">
              <Section
                title={t('Parties')}
                action={
                  draft && (
                    <Button variant="ghost" size="sm" onClick={() => setPartyDialog({ open: true, party: null })}>
                      <UserPlus /> {t('Add')}
                    </Button>
                  )
                }
              >
                {doc.parties.length === 0 && (
                  <p className="text-muted-foreground">{t('Add the people who sign, approve or receive a copy.')}</p>
                )}
                {draft && signers.length > 1 && (
                  <label className="flex items-center justify-between gap-3 rounded-md bg-muted/50 px-3 py-2">
                    <span>
                      <span className="block">{t('Sign in Order')}</span>
                      <span className="block text-xs text-muted-foreground">
                        {doc.sequential
                          ? t(
                              'Each step is invited when the one before has signed. Use the arrows to reorder; give parties the same step under Edit to sign together.',
                            )
                          : t('Everyone is invited at once. Turn on to invite one step at a time.')}
                      </span>
                    </span>
                    <Switch
                      checked={doc.sequential}
                      onCheckedChange={(sequential) => {
                        setSequential.mutate({ id: doc.id, sequential });
                        // Everyone on the same step would still sign at once, so start from the listed order.
                        if (sequential && steps.size === 1) {
                          saveParties(
                            allParties().map((p) => ({ ...p, signOrder: signers.findIndex((s) => s.id === p.id) + 1 || p.signOrder })),
                          );
                        }
                      }}
                    />
                  </label>
                )}
                {(doc.sequential ? orderedParties : doc.parties).map((p) => (
                  <PartyCard
                    step={doc.sequential && p.role !== 'viewer' ? steps.get(p.signOrder) : undefined}
                    onMove={doc.sequential && p.role !== 'viewer' && signers.length > 1 ? moves(p.id) : undefined}
                    key={p.id}
                    party={p}
                    parties={doc.parties}
                    editable={draft}
                    pending={pending && doc.canEdit}
                    documentId={doc.id}
                    attachments={doc.attachments.filter((a) => a.partyId === p.id)}
                    onEdit={() => setPartyDialog({ open: true, party: p })}
                    onRemove={() => saveParties(allParties().filter((x) => x.id !== p.id))}
                  />
                ))}
              </Section>
              {(draft || doc.isTemplate) && doc.files.length > 0 && (
                <Section title={t('Files')}>
                  <ul className="grid gap-1">
                    {doc.files.map((f) => (
                      <li key={f.id} className="flex items-center gap-2">
                        <span className="min-w-0 flex-1 truncate">{f.name}</span>
                        <span className="text-xs text-muted-foreground">{formatBytes(f.sizeBytes)}</span>
                        <ReplaceFileButton name={f.name} busy={uploading} onFile={(file) => replace(f.id, file)} />
                        <Button
                          variant="ghost"
                          size="icon-sm"
                          aria-label={t('Remove {name}', { name: f.name })}
                          onClick={() => removeFile.mutate({ documentId: doc.id, fileId: f.id })}
                        >
                          <Trash2 />
                        </Button>
                      </li>
                    ))}
                  </ul>
                  <FileDrop onFiles={upload} busy={uploading} />
                </Section>
              )}
              {draft && <DetailsSection key={doc.id} doc={doc} />}
              {(draft || (pending && doc.canEdit)) && <AiReviewSection doc={doc} />}
              {doc.status === 'completed' && doc.sealedSha256 && (
                <Section title={t('Seal')}>
                  <p className="text-muted-foreground">
                    {doc.sealedWithTimestamp
                      ? t('Sealed by OpenSign with a trusted timestamp. Anyone can check the file at')
                      : t('Sealed by OpenSign. Anyone can check the file at')}{' '}
                    <a href="/verify" className="underline">
                      /verify
                    </a>
                    .
                  </p>
                  <code className="break-all text-xs text-muted-foreground">SHA-256 {doc.sealedSha256}</code>
                </Section>
              )}
            </TabsContent>
            <TabsContent value="review" className="mt-4">
              <ReviewPanel documentId={doc.id} draft={draft} />
            </TabsContent>
          </Tabs>
        </aside>
      </div>

      <PartyDialog
        open={partyDialog.open}
        onOpenChange={(open) => setPartyDialog({ ...partyDialog, open })}
        party={partyDialog.party ? partyToInput(partyDialog.party, doc.attachments) : null}
        maskedNumber={partyDialog.party?.personalNumber}
        sequential={doc.sequential}
        available={authMethods.data?.available ?? ['none', 'standard']}
        documentLanguage={doc.language}
        onSave={(p) => saveParties(partyDialog.party ? allParties().map((x) => (x.id === p.id ? p : x)) : [...allParties(), p])}
      />
      <SendDialog doc={doc} open={sending} onOpenChange={setSending} onSent={setLinks} />
      <SigningPreviewDialog doc={doc} open={previewing} onOpenChange={setPreviewing} />
      {doc.isTemplate && (
        <TemplateHistoryDialog
          doc={doc}
          open={showHistory}
          onOpenChange={setShowHistory}
          onRestored={(restored) => {
            reload(restored);
            setShowHistory(false);
          }}
        />
      )}
      {doc.isTemplate && <BulkSendDialog templateId={doc.id} templateTitle={doc.title} open={bulkSending} onOpenChange={setBulkSending} />}
      <LinksDialog links={links} onClose={() => setLinks([])} />
      <Dialog open={showActivity} onOpenChange={setShowActivity}>
        <DialogContent className="flex max-h-[85svh] flex-col sm:max-w-xl">
          <DialogHeader>
            <DialogTitle>{t('Activity')}</DialogTitle>
            <DialogDescription>{t('What happened to this document, and the full audit trail.')}</DialogDescription>
          </DialogHeader>
          <div className="-mx-6 min-h-0 overflow-y-auto px-6">
            <ActivityPanel document={doc} />
          </div>
        </DialogContent>
      </Dialog>
      <Dialog open={canceling} onOpenChange={setCanceling}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('Cancel Signing?')}</DialogTitle>
            <DialogDescription>
              {t('Signing links stop working and parties who were notified are told. This cannot be undone.')}
            </DialogDescription>
          </DialogHeader>
          <Textarea
            value={cancelReason}
            onChange={(e) => setCancelReason(e.target.value)}
            placeholder={t('Reason (optional, kept in the activity log)')}
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => setCanceling(false)}>
              {t('Keep Signing')}
            </Button>
            <Button
              variant="destructive"
              disabled={cancel.isPending}
              onClick={() => cancel.mutate({ id: doc.id, reason: cancelReason.trim() || null })}
            >
              {t('Cancel Signing')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <Dialog open={holding} onOpenChange={setHolding}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('Place on Legal Hold?')}</DialogTitle>
            <DialogDescription>
              {t(
                'Nobody can delete “{title}” and no retention rule touches it until the hold is released. Both are recorded in the audit trail.',
                {
                  title: doc.title,
                },
              )}
            </DialogDescription>
          </DialogHeader>
          <Textarea
            value={holdReason}
            onChange={(e) => setHoldReason(e.target.value)}
            placeholder={t('Reason, such as a case or matter reference')}
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => setHolding(false)}>
              {t('Cancel')}
            </Button>
            <Button
              disabled={legalHold.isPending || !holdReason.trim()}
              onClick={() => legalHold.mutate({ id: doc.id, hold: true, reason: holdReason.trim() })}
            >
              {t('Place Hold')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <Dialog open={deleting} onOpenChange={setDeleting}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{doc.isTemplate ? t('Delete Template?') : t('Delete Document?')}</DialogTitle>
            <DialogDescription>{t('“{title}” is deleted for everyone. This cannot be undone.', { title: doc.title })}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleting(false)}>
              {t('Keep It')}
            </Button>
            <Button variant="destructive" disabled={remove.isPending} onClick={() => remove.mutate({ id: doc.id })}>
              {t('Delete')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
