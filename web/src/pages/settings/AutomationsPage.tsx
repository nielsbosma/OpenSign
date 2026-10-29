import { History, Plug, Plus, Workflow, Zap } from 'lucide-react';
import { type FormEvent, useEffect, useState } from 'react';
import { Link, useNavigate, useParams, useSearchParams } from 'react-router';
import { toast } from 'sonner';
import { ConfirmButton, EmptyState, Field, Loading, Spinner, StatusDot } from '@/components/common';
import { SearchSelect } from '@/components/SearchSelect';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Textarea } from '@/components/ui/textarea';
import { type AutomationView, call, type Output, useApi, useApiMutation } from '@/lib/api';
import { formatDateTime } from '@/lib/format';
import { msg, t } from '@/lib/i18n';
import { FormError } from '../auth/AuthLayout';
import { SettingsGroup } from './AdminSettingsPage';
import { SettingsCard } from './SettingsLayout';
import { DeliveryLog } from './WebhookDeliveriesPage';

type Recipe = { label: string; app: string; search: string; tool: string; event: string; text: Record<string, string> };

const SIGNED = 'Signed: {{document.title}} by {{parties}}. {{document.url}}';

const RECIPES: Recipe[] = [
  {
    label: msg('Save the Signed PDF to Google Drive'),
    app: 'googledrive',
    search: 'upload file',
    tool: 'GOOGLEDRIVE_UPLOAD_FILE',
    event: 'document.completed',
    text: {},
  },
  {
    label: msg('Save the Signed PDF to Dropbox'),
    app: 'dropbox',
    search: 'upload file',
    tool: 'DROPBOX_UPLOAD_FILE',
    event: 'document.completed',
    text: { path: '/OpenSign/{{document.title}}.pdf' },
  },
  {
    label: msg('Post to a Slack Channel'),
    app: 'slack',
    search: 'send message',
    tool: 'SLACK_SEND_MESSAGE',
    event: 'document.completed',
    text: { markdown_text: SIGNED, text: SIGNED },
  },
  {
    label: msg('Update a HubSpot Deal'),
    app: 'hubspot',
    search: 'update deal',
    tool: 'HUBSPOT_UPDATE_DEAL',
    event: 'document.completed',
    text: { dealId: '{{document.reference}}' },
  },
  {
    label: msg('Add a Row to Google Sheets'),
    app: 'googlesheets',
    search: 'append row',
    tool: 'GOOGLESHEETS_SPREADSHEETS_VALUES_APPEND',
    event: 'document.completed',
    text: {},
  },
];

const EVENT_LABELS: Record<string, string> = {
  'document.created': msg('A document is created'),
  'document.sent': msg('A document is sent'),
  'document.viewed': msg('A party opens their link'),
  'document.signed': msg('A party signs'),
  'document.completed': msg('Everyone has signed'),
  'document.declined': msg('A party declines'),
  'document.canceled': msg('A document is canceled'),
  'document.expired': msg('A document expires'),
};

type Schema = { type?: string; description?: string; file_uploadable?: boolean; enum?: unknown[] };

function isFile(name: string, s: Schema) {
  return !!s.file_uploadable || (s.type === 'object' && /file/i.test(name));
}

/** Form text back to what the tool expects: numbers, booleans and JSON where the schema says so, placeholders as they are. */
function toArgument(text: string, s: Schema): unknown {
  const v = text.trim();
  if (v.startsWith('{{') && v.endsWith('}}') && v.indexOf('{{', 2) < 0) return v;
  if ((s.type === 'number' || s.type === 'integer') && v !== '' && !Number.isNaN(Number(v))) return Number(v);
  if (s.type === 'boolean' && (v === 'true' || v === 'false')) return v === 'true';
  if ((s.type === 'array' || s.type === 'object') && /^[[{]/.test(v)) {
    try {
      return JSON.parse(v);
    } catch {
      return text;
    }
  }
  return text;
}

function fromArgument(value: unknown) {
  if (value === undefined || value === null) return '';
  return typeof value === 'string' ? value : JSON.stringify(value);
}

function Placeholders({ placeholders }: { placeholders: { name: string; description: string }[] }) {
  return (
    <details className="rounded-md border px-3 py-2 text-sm">
      <summary className="cursor-pointer font-medium">{t('Placeholders')}</summary>
      <p className="mt-2 text-muted-foreground text-xs">
        {t('Write these in any input. A field is {fields}, a party {party}. Click one to copy it.', {
          fields: '{{fields.Label}}',
          party: '{{parties.0.email}}',
        })}
      </p>
      <ul className="mt-2 grid gap-1">
        {placeholders.map((p) => (
          <li key={p.name} className="flex flex-wrap items-baseline gap-x-2">
            <button
              type="button"
              className="font-mono text-primary text-xs hover:underline"
              dir="ltr"
              onClick={async () => {
                await navigator.clipboard.writeText(`{{${p.name}}}`);
                toast.success(t('Copied'));
              }}
            >
              {`{{${p.name}}}`}
            </button>
            <span className="text-muted-foreground text-xs">{p.description}</span>
          </li>
        ))}
      </ul>
    </details>
  );
}

function IdChecklist({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: { id: string; name: string }[];
  value: string[];
  onChange: (ids: string[]) => void;
}) {
  if (!options.length) return null;
  return (
    <Field label={label} hint={t('None ticked: all of them.')}>
      <div className="grid max-h-36 gap-1.5 overflow-y-auto rounded-md border px-3 py-2">
        {options.map((o) => (
          <label key={o.id} className="flex items-center gap-2.5 text-sm">
            <Checkbox
              checked={value.includes(o.id)}
              onCheckedChange={(c) => onChange(c ? [...value, o.id] : value.filter((x) => x !== o.id))}
            />
            <span className="truncate">{o.name}</span>
          </label>
        ))}
      </div>
    </Field>
  );
}

type Connection = Output<'automations.connections'>[number];

function RuleDialog({
  open,
  onOpenChange,
  rule,
  recipe,
  connections,
  onConnect,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  rule?: AutomationView;
  recipe?: Recipe;
  connections: Connection[];
  onConnect: (app: string) => void;
}) {
  const status = useApi('automations.status');
  const templates = useApi('documents.list', { templates: true, limit: 100 }, { enabled: open });
  const folders = useApi('folders.list', { scope: 'documents' }, { enabled: open });
  const app = rule?.app ?? recipe?.app;
  const usable = connections.filter((c) => c.status === 'ACTIVE' && (!app || c.app === app));
  const [name, setName] = useState('');
  const [event, setEvent] = useState('document.completed');
  const [connectionId, setConnectionId] = useState('');
  const [tool, setTool] = useState('');
  const [args, setArgs] = useState<Record<string, string>>({});
  const [templateIds, setTemplateIds] = useState<string[]>([]);
  const [folderIds, setFolderIds] = useState<string[]>([]);
  const [error, setError] = useState<string>();
  const connection = connections.find((c) => c.id === connectionId);
  const tools = useApi(
    'automations.tools',
    { app: connection?.app ?? '', search: recipe && !rule ? recipe.search : null, limit: 100 },
    { enabled: open && !!connection },
  );
  const detail = useApi('automations.tool', { tool }, { enabled: open && !!tool });
  const properties = (detail.data?.inputSchema as { properties?: Record<string, Schema> })?.properties ?? {};
  const required: string[] = (detail.data?.inputSchema as { required?: string[] })?.required ?? [];

  // biome-ignore lint/correctness/useExhaustiveDependencies: start fresh each time the dialog opens
  useEffect(() => {
    if (!open) return;
    setError(undefined);
    setName(rule?.name ?? (recipe ? t(recipe.label) : ''));
    setEvent(rule?.event ?? recipe?.event ?? 'document.completed');
    setConnectionId(rule?.connectionId ?? usable[0]?.id ?? '');
    setTool(rule?.tool ?? recipe?.tool ?? '');
    setTemplateIds(rule?.templateIds ?? []);
    setFolderIds(rule?.folderIds ?? []);
    setArgs(Object.fromEntries(Object.entries((rule?.arguments as Record<string, unknown>) ?? {}).map(([k, v]) => [k, fromArgument(v)])));
  }, [open]);

  // A recipe fills in what it knows once the tool's real inputs are known.
  // biome-ignore lint/correctness/useExhaustiveDependencies: only when a new tool's schema arrives
  useEffect(() => {
    if (rule || !detail.data) return;
    const filled: Record<string, string> = {};
    for (const [key, s] of Object.entries(properties)) {
      if (isFile(key, s) && event === 'document.completed') filled[key] = '{{sealedPdf}}';
      else if (recipe?.text[key]) filled[key] = recipe.text[key];
    }
    setArgs((current) => ({ ...filled, ...current }));
  }, [detail.data]);

  const save = useApiMutation(rule ? 'automations.update' : 'automations.create', {
    invalidates: ['automations.'],
    silent: true,
    onSuccess: () => {
      toast.success(t('Saved'));
      onOpenChange(false);
    },
  });

  const submit = (e: FormEvent) => {
    e.preventDefault();
    setError(undefined);
    const argumentsOut = Object.fromEntries(
      Object.entries(args)
        .filter(([k, v]) => k in properties && v.trim() !== '')
        .map(([k, v]) => [k, toArgument(v, properties[k] ?? {})]),
    );
    const input = { name, event, connectionId, tool, arguments: argumentsOut, templateIds, folderIds, active: rule?.active ?? true };
    save.mutate(rule ? { id: rule.id, ...input } : input, { onError: (e) => setError(e.message) });
  };

  const toolOptions = (tools.data ?? []).map((x) => ({ value: x.slug, label: x.name || x.slug, detail: x.slug }));
  if (tool && !toolOptions.some((o) => o.value === tool)) toolOptions.unshift({ value: tool, label: tool, detail: '' });

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90dvh] overflow-y-auto sm:max-w-2xl">
        <form onSubmit={submit} className="grid gap-4">
          <DialogHeader>
            <DialogTitle>{rule ? t('Edit Automation') : t('New Automation')}</DialogTitle>
            <DialogDescription>{t('When something happens to a document, OpenSign runs a tool in a connected app.')}</DialogDescription>
          </DialogHeader>
          <FormError message={error} />
          <Field label={t('Name')} htmlFor="rule-name">
            <Input id="rule-name" value={name} onChange={(e) => setName(e.target.value)} required maxLength={200} />
          </Field>
          <Field label={t('When')}>
            <Select value={event} onValueChange={setEvent}>
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {(status.data?.events ?? Object.keys(EVENT_LABELS)).map((ev) => (
                  <SelectItem key={ev} value={ev}>
                    {t(EVENT_LABELS[ev] ?? ev)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field label={t('App')}>
            {usable.length ? (
              <Select
                value={connectionId}
                onValueChange={(v) => {
                  setConnectionId(v);
                  if (connections.find((c) => c.id === v)?.app !== connection?.app) setTool('');
                }}
              >
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {usable.map((c) => (
                    <SelectItem key={c.id} value={c.id}>
                      {c.app} <span className="font-mono text-muted-foreground text-xs">{c.id}</span>
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : (
              <div className="flex flex-wrap items-center gap-3 rounded-md border border-dashed px-3 py-2 text-sm">
                <span className="text-muted-foreground">{app ? t('Connect {app} first.', { app }) : t('Connect an app first.')}</span>
                {app && (
                  <Button type="button" size="sm" variant="outline" onClick={() => onConnect(app)}>
                    <Plug /> {t('Connect')}
                  </Button>
                )}
              </div>
            )}
          </Field>
          {connection && (
            <Field label={t('Tool')} hint={detail.data?.description?.slice(0, 300)}>
              <SearchSelect
                value={tool}
                onChange={(v) => {
                  setTool(v);
                  setArgs({});
                }}
                options={toolOptions}
                placeholder={tools.isPending ? t('Loading…') : t('Choose what the app should do')}
                searchPlaceholder={t('Search tools…')}
              />
            </Field>
          )}
          {tool && detail.isPending && <Loading />}
          {detail.data && (
            <div className="grid gap-3 rounded-md border bg-muted/30 px-3 py-3">
              <div className="font-medium text-sm">{t('Inputs')}</div>
              {Object.keys(properties).length === 0 && <p className="text-muted-foreground text-sm">{t('This tool takes no inputs.')}</p>}
              {Object.entries(properties).map(([key, s]) => {
                const id = `arg-${key}`;
                const label = (
                  <span className="font-mono text-xs" dir="ltr">
                    {key}
                    {required.includes(key) && <span className="text-destructive"> *</span>}
                  </span>
                );
                const hint = s.description?.slice(0, 240);
                if (isFile(key, s)) {
                  return (
                    <Field key={key} label={label} hint={hint}>
                      <Select value={args[key] || 'none'} onValueChange={(v) => setArgs({ ...args, [key]: v === 'none' ? '' : v })}>
                        <SelectTrigger className="w-full">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="none">{t('Nothing')}</SelectItem>
                          <SelectItem value="{{sealedPdf}}">{t('The sealed PDF')}</SelectItem>
                        </SelectContent>
                      </Select>
                    </Field>
                  );
                }
                const long = s.type === 'string' && /text|body|message|description|content/i.test(key);
                return (
                  <Field key={key} label={label} htmlFor={id} hint={hint}>
                    {long ? (
                      <Textarea id={id} rows={3} value={args[key] ?? ''} onChange={(e) => setArgs({ ...args, [key]: e.target.value })} />
                    ) : (
                      <Input
                        id={id}
                        value={args[key] ?? ''}
                        onChange={(e) => setArgs({ ...args, [key]: e.target.value })}
                        placeholder={s.type && s.type !== 'string' ? s.type : undefined}
                      />
                    )}
                  </Field>
                );
              })}
              {status.data && <Placeholders placeholders={status.data.placeholders} />}
            </div>
          )}
          <IdChecklist
            label={t('Only for These Templates')}
            options={(templates.data ?? []).map((x) => ({ id: x.id, name: x.title }))}
            value={templateIds}
            onChange={setTemplateIds}
          />
          <IdChecklist
            label={t('Only in These Folders')}
            options={(folders.data ?? []).map((x) => ({ id: x.id, name: x.name }))}
            value={folderIds}
            onChange={setFolderIds}
          />
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              {t('Cancel')}
            </Button>
            <Button type="submit" disabled={save.isPending || !connectionId || !tool || !name.trim()}>
              {save.isPending && <Spinner />} {rule ? t('Save') : t('Create Automation')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function ConnectDialog({
  open,
  onOpenChange,
  onConnect,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConnect: (app: string) => void;
}) {
  const [search, setSearch] = useState('');
  const [query, setQuery] = useState('');
  useEffect(() => {
    const timer = setTimeout(() => setQuery(search.trim()), 300);
    return () => clearTimeout(timer);
  }, [search]);
  const apps = useApi('automations.apps', { search: query || null, limit: 30 }, { enabled: open });
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t('Connect an App')}</DialogTitle>
          <DialogDescription>{t('You sign in to the app at Composio, and come back here when it is connected.')}</DialogDescription>
        </DialogHeader>
        <Input value={search} onChange={(e) => setSearch(e.target.value)} placeholder={t('Search apps…')} autoFocus />
        {apps.isPending ? (
          <Loading />
        ) : !apps.data?.length ? (
          <p className="py-6 text-center text-muted-foreground text-sm">{t('Nothing found.')}</p>
        ) : (
          <ul className="grid gap-1.5">
            {apps.data.map((a) => (
              <li key={a.slug} className="flex items-center gap-3 rounded-md border px-3 py-2">
                <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-muted font-medium text-sm">
                  {(a.name || a.slug).slice(0, 1).toUpperCase()}
                </span>
                <div className="min-w-40 flex-1">
                  <div className="truncate font-medium text-sm">{a.name || a.slug}</div>
                  <div className="truncate text-muted-foreground text-xs">{a.description ?? a.categories.join(', ')}</div>
                </div>
                <Button size="sm" variant="outline" onClick={() => onConnect(a.slug)}>
                  {t('Connect')}
                </Button>
              </li>
            ))}
          </ul>
        )}
      </DialogContent>
    </Dialog>
  );
}

function TestDialog({ rule, onOpenChange }: { rule: AutomationView | null; onOpenChange: (open: boolean) => void }) {
  const navigate = useNavigate();
  const documents = useApi(
    'documents.list',
    { limit: 100, status: rule?.event === 'document.completed' ? 'completed' : null },
    { enabled: !!rule },
  );
  const [documentId, setDocumentId] = useState('');
  const test = useApiMutation('automations.test', {
    invalidates: ['automations.'],
    onSuccess: () => {
      toast.success(t('Test run queued'));
      onOpenChange(false);
      if (rule) navigate(`/settings/automations/${rule.id}`);
    },
  });
  return (
    <Dialog open={!!rule} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t('Test Automation')}</DialogTitle>
          <DialogDescription>{t("Runs the tool now with one document's data. The app really receives it.")}</DialogDescription>
        </DialogHeader>
        <Field label={t('Document')}>
          <SearchSelect
            value={documentId}
            onChange={setDocumentId}
            options={(documents.data ?? []).map((d) => ({ value: d.id, label: d.title, detail: formatDateTime(d.createdAt) }))}
            searchPlaceholder={t('Search documents…')}
          />
        </Field>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t('Cancel')}
          </Button>
          <Button disabled={!documentId || test.isPending || !rule} onClick={() => rule && test.mutate({ id: rule.id, documentId })}>
            {test.isPending && <Spinner />} {t('Run Test')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ruleInput(rule: AutomationView, active: boolean) {
  return {
    id: rule.id,
    name: rule.name,
    event: rule.event,
    connectionId: rule.connectionId,
    tool: rule.tool,
    arguments: rule.arguments,
    templateIds: rule.templateIds,
    folderIds: rule.folderIds,
    active,
  };
}

function Automations() {
  const [params, setParams] = useSearchParams();
  const connections = useApi('automations.connections');
  const rules = useApi('automations.list');
  const [connecting, setConnecting] = useState(false);
  const [editing, setEditing] = useState<{ rule?: AutomationView; recipe?: Recipe } | null>(null);
  const [testing, setTesting] = useState<AutomationView | null>(null);
  const [starting, setStarting] = useState<string | null>(null);
  const disconnect = useApiMutation('automations.disconnect', {
    invalidates: ['automations.'],
    onSuccess: (out) =>
      toast.success(out.paused ? t('Disconnected. Automations paused: {count}', { count: out.paused }) : t('Disconnected')),
  });
  const update = useApiMutation('automations.update', { invalidates: ['automations.'] });
  const remove = useApiMutation('automations.delete', { invalidates: ['automations.'] });

  // biome-ignore lint/correctness/useExhaustiveDependencies: once, when Composio sends the admin back
  useEffect(() => {
    const app = params.get('connected');
    if (!app) return;
    toast.success(t('{app} is connected', { app }));
    setParams({}, { replace: true });
  }, []);

  const connect = async (app: string) => {
    setStarting(app);
    try {
      const out = await call('automations.connect', { app });
      window.location.assign(out.redirectUrl);
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
      setStarting(null);
    }
  };

  const list = connections.data ?? [];
  return (
    <>
      <SettingsCard
        help="automations#connecting-apps"
        title={t('Connected Apps')}
        description={t('Accounts this OpenSign can use through Composio. They belong to the organisation, not to one person.')}
      >
        <Button className="mb-4" onClick={() => setConnecting(true)} disabled={!!starting}>
          {starting ? <Spinner /> : <Plus />} {t('Connect an App')}
        </Button>
        {connections.isPending ? (
          <Loading />
        ) : connections.error ? (
          <p className="text-destructive text-sm">{connections.error.message}</p>
        ) : !list.length ? (
          <EmptyState icon={<Plug />} title={t('No apps connected')} />
        ) : (
          <ul className="grid grid-cols-1 gap-2">
            {list.map((c) => (
              <li key={c.id} className="flex flex-wrap items-center gap-3 rounded-md border px-3 py-2">
                <StatusDot tone={c.status === 'ACTIVE' ? 'ok' : c.status === 'INITIATED' ? 'warn' : 'error'} />
                <div className="min-w-40 flex-1">
                  <div className="truncate font-medium text-sm">{c.app}</div>
                  <div className="truncate text-muted-foreground text-xs" dir="ltr">
                    {c.id}
                  </div>
                </div>
                <Badge variant="outline">{c.status}</Badge>
                {c.status !== 'ACTIVE' && (
                  <Button size="sm" variant="outline" onClick={() => connect(c.app)}>
                    {t('Reconnect')}
                  </Button>
                )}
                <span className="text-muted-foreground text-xs">{t('Automations: {count}', { count: c.automations })}</span>
                <ConfirmButton
                  label={t('Disconnect')}
                  title={t('Disconnect this app?')}
                  description={t('The account is removed from Composio, and automations that use it are paused.')}
                  onConfirm={() => disconnect.mutate({ id: c.id })}
                />
              </li>
            ))}
          </ul>
        )}
      </SettingsCard>
      <SettingsCard
        help="automations#rules"
        title={t('Automations')}
        description={t('Each automation runs one tool when a document event happens, with retries. Every run is in its log.')}
      >
        <div className="mb-4 flex flex-wrap gap-2">
          <Button onClick={() => setEditing({})}>
            <Plus /> {t('New Automation')}
          </Button>
        </div>
        <div className="mb-5 grid gap-2">
          <div className="text-muted-foreground text-xs">{t('Start from a recipe')}</div>
          <div className="flex flex-wrap gap-2">
            {RECIPES.map((r) => (
              <Button
                key={r.tool}
                variant="outline"
                size="sm"
                className="h-auto min-h-8 max-w-full whitespace-normal text-start"
                onClick={() => setEditing({ recipe: r })}
              >
                <Zap /> {t(r.label)}
              </Button>
            ))}
          </div>
        </div>
        {rules.isPending ? (
          <Loading />
        ) : !rules.data?.length ? (
          <EmptyState icon={<Workflow />} title={t('No automations')} />
        ) : (
          <ul className="grid grid-cols-1 gap-2">
            {rules.data.map((r) => (
              <li key={r.id} className="flex flex-wrap items-center gap-3 rounded-md border px-3 py-2">
                <StatusDot
                  tone={
                    !r.active
                      ? 'off'
                      : r.lastStatus === 'failed'
                        ? 'error'
                        : r.lastStatus === 'pending'
                          ? 'warn'
                          : r.lastStatus
                            ? 'ok'
                            : 'off'
                  }
                />
                <div className="min-w-40 flex-1">
                  <div className="truncate font-medium text-sm">{r.name}</div>
                  <div className="truncate text-muted-foreground text-xs">
                    {t(EVENT_LABELS[r.event] ?? r.event)} · {r.app} · <span dir="ltr">{r.tool}</span>
                    {r.lastError ? ` · ${r.lastError}` : r.lastRunAt ? ` · ${formatDateTime(r.lastRunAt)}` : ''}
                  </div>
                </div>
                {r.failed > 0 && (
                  <Link to={`/settings/automations/${r.id}`} className="shrink-0 text-destructive text-xs hover:underline">
                    {t('Failed: {count}', { count: r.failed })}
                  </Link>
                )}
                <Switch
                  checked={r.active}
                  aria-label={t('Active')}
                  onCheckedChange={(active) => update.mutate(ruleInput(r, active))}
                  disabled={update.isPending}
                />
                <Button variant="ghost" size="sm" asChild>
                  <Link to={`/settings/automations/${r.id}`}>
                    <History /> {t('Runs')}
                  </Link>
                </Button>
                <Button variant="ghost" size="sm" onClick={() => setTesting(r)}>
                  {t('Test')}
                </Button>
                <Button variant="ghost" size="sm" onClick={() => setEditing({ rule: r })}>
                  {t('Edit')}
                </Button>
                <ConfirmButton
                  label={t('Delete')}
                  title={t('Delete this automation?')}
                  description={t('Its run history goes with it. The connected app stays.')}
                  onConfirm={() => remove.mutate({ id: r.id })}
                />
              </li>
            ))}
          </ul>
        )}
      </SettingsCard>
      <ConnectDialog open={connecting} onOpenChange={setConnecting} onConnect={connect} />
      <RuleDialog
        open={!!editing}
        onOpenChange={(o) => !o && setEditing(null)}
        rule={editing?.rule}
        recipe={editing?.recipe}
        connections={list}
        onConnect={connect}
      />
      <TestDialog rule={testing} onOpenChange={(o) => !o && setTesting(null)} />
    </>
  );
}

export function AutomationsPage() {
  const status = useApi('automations.status');
  return (
    <div className="grid grid-cols-1 gap-6">
      <SettingsGroup group="automations" />
      {status.data?.enabled && status.data.keySet && <Automations />}
    </div>
  );
}

export function AutomationRunsPage() {
  const { id = '' } = useParams();
  const rules = useApi('automations.list');
  const rule = rules.data?.find((r) => r.id === id);
  if (rules.isPending) return <Loading />;
  if (!rule) {
    return (
      <SettingsCard title={t('Runs')}>
        <EmptyState icon={<History />} title={t('This automation no longer exists.')}>
          <Link to="/settings/automations" className="text-primary hover:underline">
            {t('All Automations')}
          </Link>
        </EmptyState>
      </SettingsCard>
    );
  }
  return (
    <DeliveryLog
      kind="automation"
      owner={id}
      failed={rule.failed}
      help="automations#run-log"
      title={t('Runs')}
      subtitle={`${rule.name} · ${rule.app} · ${rule.tool}`}
      description={t('Every run of this automation in the last 30 days, with each attempt and what Composio answered.')}
      back={{ to: '/settings/automations', label: t('All Automations') }}
    />
  );
}
