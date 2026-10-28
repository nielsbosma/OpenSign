import {
  ArrowLeft,
  Download,
  HandHelping,
  Link2,
  MoreHorizontal,
  Paperclip,
  Pencil,
  Plus,
  Send,
  Trash2,
  UserPlus,
  Users,
} from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router';
import { toast } from 'sonner';
import { ConfirmButton, CopyButton, EmptyState, Spinner } from '@/components/common';
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
  type SignerLink,
  uploadFile,
  useApi,
  useApiMutation,
} from '@/lib/api';
import { AUTH_LABELS, DELIVERY_LABELS, formatBytes, formatDateTime, LANGUAGES, PARTY_STATUS_LABELS, saveBytes } from '@/lib/format';
import { useSession } from '@/lib/session';
import { cn } from '@/lib/utils';
import { ActivityPanel } from './ActivityPanel';
import { BulkSendDialog } from './BulkSendDialog';
import { StatusBadge } from './DocumentsPage';
import { FieldLayer } from './FieldLayer';
import { clamp, type EditableField, FIELD_KINDS, partyColor, toEditable, toInput } from './fields';
import { FileDrop } from './NewDocumentDialog';
import { PartyDialog, partyToInput } from './PartyDialog';
import { Checks, ReviewPanel } from './ReviewPanel';

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
  const save = useApiMutation('documents.set_fields', { invalidates: ['documents.check'] });

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
      timer.current = setTimeout(() => save.mutate({ documentId: doc.id, fields: toInput(next) }), 500);
    },
    [doc, editable, save],
  );
  const update = (next: EditableField[], persist = true) => {
    setFields(next);
    if (persist) commit(next);
  };
  const reload = (doc: DocumentView) => setFields(toEditable(doc.fields));
  return { fields, update, reload, saving: save.isPending };
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
}: {
  party: PartyView;
  parties: PartyView[];
  editable: boolean;
  pending: boolean;
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
        <span className="mt-1 size-2.5 shrink-0 rounded-full" style={{ backgroundColor: partyColor(parties, party.id) }} />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate font-medium">{party.name}</span>
            {!editable && (
              <span
                className={cn(
                  'ml-auto shrink-0 text-xs',
                  party.status === 'signed' ? 'text-success' : party.status === 'declined' ? 'text-destructive' : 'text-muted-foreground',
                )}
              >
                {PARTY_STATUS_LABELS[party.status] ?? party.status}
              </span>
            )}
          </div>
          <div className="truncate text-xs text-muted-foreground">
            {[party.email, party.phone, party.company].filter(Boolean).join(' · ') || 'No contact details'}
          </div>
          <div className="mt-1 text-xs text-muted-foreground">
            {party.role === 'viewer' ? 'Receives a copy' : party.role === 'approver' ? 'Approves' : 'Signs'} ·{' '}
            {DELIVERY_LABELS[party.delivery]?.replace(/ \(.*\)$/, '')} · {AUTH_LABELS[party.authToSign]}
            {party.authToView !== 'none' && ` · ${AUTH_LABELS[party.authToView]} to view`}
          </div>
          {party.declineReason && <div className="mt-1 text-xs text-destructive">“{party.declineReason}”</div>}
          {attachments.length > 0 && (
            <ul className="mt-1.5 grid gap-0.5 text-xs">
              {attachments.map((a) => (
                <li key={a.id} className="flex items-center gap-1.5">
                  <Paperclip className="size-3 shrink-0 text-muted-foreground" />
                  <span className="min-w-0 truncate">
                    {a.name}
                    {!a.required && <span className="text-muted-foreground"> (optional)</span>}
                  </span>
                  {a.fileId ? (
                    <button
                      type="button"
                      className="ml-auto shrink-0 underline"
                      onClick={() =>
                        fetchPdf(`/api/documents/${documentId}/attachments/${a.id}`)
                          .then((bytes) => saveBytes(bytes, a.fileName ?? 'attachment', a.contentType ?? 'application/octet-stream'))
                          .catch((e) => toast.error(errorMessage(e)))
                      }
                    >
                      Download
                    </button>
                  ) : (
                    <span className="ml-auto shrink-0 text-muted-foreground">Not uploaded</span>
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>
        {editable && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="icon-sm" aria-label={`Options for ${party.name}`}>
                <MoreHorizontal />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onSelect={onEdit}>
                <Pencil /> Edit
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={onRemove} className="text-destructive">
                <Trash2 /> Remove
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
                <CopyButton value={link.url} label="Copy Link" />
                <Button variant="ghost" size="sm" onClick={() => setLink(null)}>
                  Hide
                </Button>
              </div>
            </div>
          ) : (
            <>
              {party.delivery === 'in_person' && (
                <Button size="sm" disabled={inPerson.isPending} onClick={() => inPerson.mutate({ documentId, partyId: party.id })}>
                  <HandHelping /> Sign in Person
                </Button>
              )}
              <Button
                variant="outline"
                size="sm"
                disabled={signingLink.isPending}
                onClick={() => signingLink.mutate({ documentId, partyId: party.id, rotate: false })}
              >
                <Link2 /> Signing Link
              </Button>
              <ConfirmButton
                label="New Link"
                title="Replace the signing link?"
                description={`${party.name}'s earlier links stop working. Use this when a link reached the wrong person.`}
                confirmLabel="Replace Link"
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
      toast.success('Sent for signing');
    },
  });
  const blocked = checks.data?.some((c) => c.severity === 'error');
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Send for Signing</DialogTitle>
          <DialogDescription>
            The document is locked once sent. Parties who get their link by email or SMS are notified
            {doc.sequential ? ' in signing order' : ' now'}.
          </DialogDescription>
        </DialogHeader>
        {checks.data ? <Checks checks={checks.data} /> : <Spinner />}
        <div className="grid gap-2">
          <Label htmlFor="expires">Expires</Label>
          <Input
            id="expires"
            type="date"
            value={expires}
            onChange={(e) => setExpires(e.target.value)}
            min={new Date().toISOString().slice(0, 10)}
          />
          <p className="text-xs text-muted-foreground">Leave empty for the default from Settings.</p>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            disabled={blocked || send.isPending || !checks.data}
            onClick={() => send.mutate({ id: doc.id, expiresAt: expires ? new Date(`${expires}T23:59:59`).toISOString() : null })}
          >
            {send.isPending ? <Spinner className="text-primary-foreground" /> : <Send />} Send
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
          <DialogTitle>Hand Over the Signing Links</DialogTitle>
          <DialogDescription>
            These parties get their link from you. Each link works only for that person; send it privately.
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
          <Button onClick={onClose}>Done</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function DetailsSection({ doc }: { doc: DocumentView }) {
  const update = useApiMutation('documents.update', { invalidates: ['documents.'] });
  const [message, setMessage] = useState(doc.message);
  const [externalId, setExternalId] = useState(doc.externalId ?? '');
  const [reminder, setReminder] = useState(String(doc.reminderDays ?? 0));
  return (
    <Section title="Details">
      <div className="grid gap-1.5">
        <Label htmlFor="message">Message to Parties</Label>
        <Textarea
          id="message"
          rows={3}
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          onBlur={() => message !== doc.message && update.mutate({ id: doc.id, message })}
          placeholder="Shown in the invitation and on the signing page"
        />
      </div>
      <div className="grid grid-cols-2 gap-3">
        <div className="grid gap-1.5">
          <Label htmlFor="language">Language for Signers</Label>
          <Select value={doc.language} onValueChange={(language) => update.mutate({ id: doc.id, language })}>
            <SelectTrigger id="language" className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {LANGUAGES.map(([code, name]) => (
                <SelectItem key={code} value={code}>
                  {name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="grid gap-1.5">
          <Label htmlFor="reminders">Remind Every (Days)</Label>
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
          <Label htmlFor="external">Your Reference</Label>
          <Input
            id="external"
            value={externalId}
            onChange={(e) => setExternalId(e.target.value)}
            onBlur={() => externalId !== (doc.externalId ?? '') && update.mutate({ id: doc.id, externalId })}
            className="font-mono"
          />
        </div>
      </div>
      <label className="flex items-center justify-between gap-3">
        <span>
          <span className="block">Sign in Order</span>
          <span className="block text-xs text-muted-foreground">Parties are invited one signing-order group at a time.</span>
        </span>
        <Switch checked={doc.sequential} onCheckedChange={(sequential) => update.mutate({ id: doc.id, sequential })} />
      </label>
    </Section>
  );
}

function EditableTitle({ doc, editable }: { doc: DocumentView; editable: boolean }) {
  const [title, setTitle] = useState(doc.title);
  const update = useApiMutation('documents.update', { invalidates: ['documents.'] });
  useEffect(() => setTitle(doc.title), [doc.title]);
  if (!editable) return <h1 className="truncate text-xl font-semibold tracking-tight">{doc.title}</h1>;
  return (
    <input
      aria-label="Title"
      value={title}
      onChange={(e) => setTitle(e.target.value)}
      onBlur={() => title.trim() && title !== doc.title && update.mutate({ id: doc.id, title: title.trim() })}
      onKeyDown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
      className="w-full min-w-0 truncate rounded-md bg-transparent px-1 -mx-1 text-xl font-semibold tracking-tight outline-none hover:bg-accent focus:bg-accent"
    />
  );
}

export function DocumentPage() {
  const { id = '' } = useParams();
  const navigate = useNavigate();
  const query = useApi('documents.get', { id });
  const authMethods = useApi('documents.auth_methods');
  const doc = query.data;
  const draft = !!doc && doc.status === 'draft' && doc.canEdit;
  const pending = !!doc && doc.status === 'pending';
  const { fields, update, saving } = useFields(doc, draft);
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

  const setParties = useApiMutation('documents.set_parties', { invalidates: ['documents.'] });
  const removeFile = useApiMutation('documents.remove_file', { invalidates: ['documents.'] });
  const remind = useApiMutation('documents.remind', { onSuccess: () => toast.success('Reminders sent') });
  const cancel = useApiMutation('documents.cancel', { invalidates: ['documents.'], onSuccess: () => setCanceling(false) });
  const remove = useApiMutation('documents.delete', {
    invalidates: ['documents.list'],
    onSuccess: () => navigate(doc?.isTemplate ? '/templates' : '/documents'),
  });
  const fromTemplate = useApiMutation('documents.create', { onSuccess: (d) => navigate(`/documents/${d.id}`) });

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
  // biome-ignore lint/correctness/useExhaustiveDependencies: only when the parties change
  useEffect(() => {
    if (doc && placingFor === null) setPlacingFor(doc.parties.find((p) => p.role !== 'viewer')?.id ?? null);
  }, [doc?.parties.length]);

  if (query.isPending) return <Spinner />;
  if (!doc) return <EmptyState title="This document is not available">It may have been deleted, or it was not shared with you.</EmptyState>;

  const saveParties = (next: PartyInput[]) => setParties.mutate({ documentId: doc.id, parties: next });
  const allParties = () => doc.parties.map((p) => partyToInput(p));

  const place = (page: number, x: number, y: number) => {
    if (!draft) return;
    if (!armed) return setSelected(null);
    const spec = FIELD_KINDS.find((k) => k.kind === armed)!;
    const field: EditableField = {
      key: crypto.randomUUID(),
      kind: armed,
      page,
      partyId: placingFor,
      required: armed !== 'checkbox',
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
      for (const file of files) {
        const out = await uploadFile(doc.id, file);
        if (out.removed.length) toast.warning(`Removed from ${file.name}: ${out.removed.join(', ')}`);
      }
      await query.refetch();
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

  return (
    <div className="grid gap-5">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <Button asChild variant="ghost" size="icon" aria-label="Back">
          <Link to={doc.isTemplate ? '/templates' : '/documents'}>
            <ArrowLeft />
          </Link>
        </Button>
        <div className="min-w-0 flex-1">
          <EditableTitle doc={doc} editable={doc.canEdit && (draft || doc.isTemplate)} />
          <div className="mt-0.5 flex items-center gap-2 text-xs text-muted-foreground">
            {!doc.isTemplate && <StatusBadge status={doc.status} />}
            <span>{doc.sentAt ? `Sent ${formatDateTime(doc.sentAt)}` : `Created ${formatDateTime(doc.createdAt)}`}</span>
            {doc.expiresAt && pending && <span>· Expires {formatDateTime(doc.expiresAt)}</span>}
            {saving && <span>· Saving…</span>}
          </div>
        </div>
        <div className="flex flex-wrap gap-2">
          {doc.isTemplate && (
            <Button onClick={() => fromTemplate.mutate({ title: doc.title, templateId: doc.id })} disabled={fromTemplate.isPending}>
              <Plus /> Use Template
            </Button>
          )}
          {doc.isTemplate && can('documents.create') && (
            <Button variant="outline" onClick={() => setBulkSending(true)}>
              <Users /> Bulk Send
            </Button>
          )}
          {draft && !doc.isTemplate && (
            <Button onClick={() => setSending(true)} disabled={!doc.files.length}>
              <Send /> Send
            </Button>
          )}
          {pending && doc.canEdit && (
            <Button variant="outline" onClick={() => remind.mutate({ id: doc.id })} disabled={remind.isPending}>
              Remind
            </Button>
          )}
          {doc.status === 'completed' && (
            <Button onClick={() => download('signed')}>
              <Download /> Signed PDF
            </Button>
          )}
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="outline" size="icon" aria-label="More">
                <MoreHorizontal />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              {doc.status !== 'draft' && (
                <DropdownMenuItem onSelect={() => download('original')}>
                  <Download /> Original PDF
                </DropdownMenuItem>
              )}
              {pending && doc.canEdit && (
                <DropdownMenuItem onSelect={() => setCanceling(true)} className="text-destructive">
                  Cancel Signing
                </DropdownMenuItem>
              )}
              {doc.canEdit && !pending && (
                <>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem onSelect={() => setDeleting(true)} className="text-destructive">
                    <Trash2 /> Delete
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
            <div className="sticky top-0 z-10 mb-3 flex flex-wrap items-center gap-1.5 rounded-lg border bg-background/95 p-2 backdrop-blur">
              <select
                aria-label="Fields for"
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
                <option value="">Me, now (prefilled)</option>
              </select>
              {FIELD_KINDS.filter((k) => placingFor || ['text', 'date', 'checkbox', 'name', 'company', 'email'].includes(k.kind)).map(
                (k) => (
                  <Button
                    key={k.kind}
                    size="sm"
                    variant={armed === k.kind ? 'default' : 'ghost'}
                    onClick={() => setArmed(armed === k.kind ? null : k.kind)}
                    style={armed === k.kind ? undefined : { color: partyColor(doc.parties, placingFor) }}
                  >
                    {k.label}
                  </Button>
                ),
              )}
              <span className="ml-auto text-xs text-muted-foreground">
                {armed ? 'Click on the page to place it' : 'Pick a field, then click the page'}
              </span>
            </div>
          )}
          {draft && selectedField && !selectedField.partyId && (
            <div className="mb-3 flex items-center gap-2">
              <Label htmlFor="prefill" className="shrink-0">
                Prefilled Value
              </Label>
              <Input
                id="prefill"
                value={selectedField.value ?? ''}
                onChange={(e) => update(fields.map((f) => (f.key === selected ? { ...f, value: e.target.value } : f)))}
              />
            </div>
          )}
          {!doc.files.length ? (
            draft || doc.isTemplate ? (
              <FileDrop onFiles={upload} busy={uploading} />
            ) : (
              <EmptyState title="No files" />
            )
          ) : pdf.pages ? (
            <div className={cn(armed && '[&_canvas]:cursor-crosshair')}>
              <PdfPages
                pages={pdf.pages}
                onPageClick={draft ? place : undefined}
                // The sealed copy has the values drawn into it.
                overlay={(page) =>
                  doc.status !== 'completed' && (
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
                  )
                }
              />
            </div>
          ) : (
            <PdfLoading error={pdf.error} />
          )}
        </div>

        <aside className="min-w-0">
          <Tabs defaultValue="setup">
            <TabsList className="w-full">
              <TabsTrigger value="setup">{draft ? 'Setup' : 'Parties'}</TabsTrigger>
              <TabsTrigger value="review">Review</TabsTrigger>
              <TabsTrigger value="activity">Activity</TabsTrigger>
            </TabsList>
            <TabsContent value="setup" className="mt-4 grid gap-6">
              <Section
                title="Parties"
                action={
                  draft && (
                    <Button variant="ghost" size="sm" onClick={() => setPartyDialog({ open: true, party: null })}>
                      <UserPlus /> Add
                    </Button>
                  )
                }
              >
                {doc.parties.length === 0 && <p className="text-muted-foreground">Add the people who sign, approve or receive a copy.</p>}
                {doc.parties.map((p) => (
                  <PartyCard
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
                <Section title="Files">
                  <ul className="grid gap-1">
                    {doc.files.map((f) => (
                      <li key={f.id} className="flex items-center gap-2">
                        <span className="min-w-0 flex-1 truncate">{f.name}</span>
                        <span className="text-xs text-muted-foreground">{formatBytes(f.sizeBytes)}</span>
                        <Button
                          variant="ghost"
                          size="icon-sm"
                          aria-label={`Remove ${f.name}`}
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
              {doc.status === 'completed' && doc.sealedSha256 && (
                <Section title="Seal">
                  <p className="text-muted-foreground">
                    Sealed by OpenSign with a trusted timestamp. Anyone can check the file at{' '}
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
            <TabsContent value="activity" className="mt-4">
              <ActivityPanel document={doc} />
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
        onSave={(p) => saveParties(partyDialog.party ? allParties().map((x) => (x.id === p.id ? p : x)) : [...allParties(), p])}
      />
      <SendDialog doc={doc} open={sending} onOpenChange={setSending} onSent={setLinks} />
      {doc.isTemplate && <BulkSendDialog templateId={doc.id} templateTitle={doc.title} open={bulkSending} onOpenChange={setBulkSending} />}
      <LinksDialog links={links} onClose={() => setLinks([])} />
      <Dialog open={canceling} onOpenChange={setCanceling}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Cancel Signing?</DialogTitle>
            <DialogDescription>Signing links stop working and parties who were notified are told. This cannot be undone.</DialogDescription>
          </DialogHeader>
          <Textarea
            value={cancelReason}
            onChange={(e) => setCancelReason(e.target.value)}
            placeholder="Reason (optional, kept in the activity log)"
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => setCanceling(false)}>
              Keep Signing
            </Button>
            <Button
              variant="destructive"
              disabled={cancel.isPending}
              onClick={() => cancel.mutate({ id: doc.id, reason: cancelReason.trim() || null })}
            >
              Cancel Signing
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <Dialog open={deleting} onOpenChange={setDeleting}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete {doc.isTemplate ? 'Template' : 'Document'}?</DialogTitle>
            <DialogDescription>“{doc.title}” is deleted for everyone. This cannot be undone.</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleting(false)}>
              Keep It
            </Button>
            <Button variant="destructive" disabled={remove.isPending} onClick={() => remove.mutate({ id: doc.id })}>
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
