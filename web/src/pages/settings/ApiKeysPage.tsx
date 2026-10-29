import { KeyRound, Pencil, Plus, Trash2 } from 'lucide-react';
import { type FormEvent, useState } from 'react';
import { Link } from 'react-router';
import { ConfirmButton, CopyButton, EmptyState, Field, IconButton, Loading } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { type Output, type Permission, useApi, useApiMutation } from '@/lib/api';
import { formatDate, formatDateTime } from '@/lib/format';
import { msg, t, tx } from '@/lib/i18n';
import { useSession } from '@/lib/session';
import { FormError } from '../auth/AuthLayout';
import { SettingsCard } from './SettingsLayout';

const PERMISSIONS: [Permission, string][] = [
  ['documents.read', msg('Read your documents')],
  ['documents.create', msg('Create, send and manage your documents')],
  ['documents.read_all', msg('Read every document')],
  ['documents.manage_all', msg('Manage every document')],
  ['templates.manage', msg('Manage templates')],
  ['users.read', msg('List users')],
  ['audit.read', msg('Read the audit trail')],
  ['webhooks.manage', msg('Manage webhooks')],
  ['org.read', msg('Read organisation details')],
];

type ApiKey = Output<'api_keys.list'>[number];

const parseIps = (text: string) =>
  text
    .split(/[\s,]+/)
    .map((s) => s.trim())
    .filter(Boolean);

function NameField({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  return (
    <Field label={t('Name')} htmlFor="key-name">
      <Input
        id="key-name"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={t('CRM integration')}
        maxLength={100}
        required
        autoFocus
      />
    </Field>
  );
}

function PermissionsField({ value, onChange }: { value: Permission[]; onChange: (value: Permission[]) => void }) {
  const { can } = useSession();
  return (
    <div className="grid gap-2">
      {PERMISSIONS.filter(([p]) => can(p)).map(([p, label]) => (
        <label key={p} className="flex items-center gap-2.5">
          <Checkbox checked={value.includes(p)} onCheckedChange={(c) => onChange(c ? [...value, p] : value.filter((x) => x !== p))} />
          <span>{t(label)}</span>
        </label>
      ))}
    </div>
  );
}

function IpsField({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  return (
    <Field label={t('Allowed IPs')} htmlFor="key-ips" hint={t('Addresses or CIDR ranges; empty for any')}>
      <Input id="key-ips" value={value} onChange={(e) => onChange(e.target.value)} placeholder="203.0.113.0/24" className="font-mono" />
    </Field>
  );
}

function EditDialog({ apiKey, onOpenChange }: { apiKey: ApiKey | null; onOpenChange: (open: boolean) => void }) {
  const { can } = useSession();
  const [name, setName] = useState('');
  const [permissions, setPermissions] = useState<Permission[]>([]);
  const [ips, setIps] = useState('');
  const [error, setError] = useState<string>();
  const [loaded, setLoaded] = useState<string | null>(null);
  if (apiKey && loaded !== apiKey.id) {
    setLoaded(apiKey.id);
    setName(apiKey.name);
    setPermissions(apiKey.permissions as Permission[]);
    setIps(apiKey.allowedIps.join(', '));
    setError(undefined);
  }
  const dropped = apiKey?.permissions.filter((p) => !can(p as Permission)) ?? [];
  const update = useApiMutation('api_keys.update', {
    invalidates: ['api_keys.'],
    silent: true,
    onSuccess: () => close(false),
  });
  const close = (open: boolean) => {
    onOpenChange(open);
    if (!open) setLoaded(null);
  };
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!apiKey) return;
    setError(undefined);
    update.mutate(
      {
        id: apiKey.id,
        name: name.trim(),
        permissions: permissions.filter((p) => can(p)),
        allowedIps: parseIps(ips),
      },
      { onError: (e) => setError(e.message) },
    );
  };
  return (
    <Dialog open={!!apiKey} onOpenChange={close}>
      <DialogContent className="sm:max-w-lg">
        <form onSubmit={submit} className="grid gap-4">
          <DialogHeader>
            <DialogTitle>{t('Edit API Key')}</DialogTitle>
            <DialogDescription>{t("Changes apply at once. The key itself stays the same; it can't be shown again.")}</DialogDescription>
          </DialogHeader>
          <FormError message={error} />
          <NameField value={name} onChange={setName} />
          <PermissionsField value={permissions} onChange={setPermissions} />
          {dropped.length > 0 && (
            <p className="text-xs text-muted-foreground">
              {t('Your role no longer allows {permissions}, so saving removes it from the key.', { permissions: dropped.join(', ') })}
            </p>
          )}
          <IpsField value={ips} onChange={setIps} />
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => close(false)}>
              {t('Cancel')}
            </Button>
            <Button type="submit" disabled={update.isPending || !permissions.some((p) => can(p))}>
              {t('Save Changes')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function CreateDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const [name, setName] = useState('');
  const [permissions, setPermissions] = useState<Permission[]>(['documents.read', 'documents.create']);
  const [days, setDays] = useState('365');
  const [ips, setIps] = useState('');
  const [key, setKey] = useState<string | null>(null);
  const [error, setError] = useState<string>();
  const create = useApiMutation('api_keys.create', { invalidates: ['api_keys.'], silent: true, onSuccess: (out) => setKey(out.key) });
  const close = (open: boolean) => {
    onOpenChange(open);
    if (!open) {
      setKey(null);
      setName('');
      setError(undefined);
    }
  };
  const submit = (e: FormEvent) => {
    e.preventDefault();
    setError(undefined);
    create.mutate(
      {
        name: name.trim(),
        permissions,
        expiresInDays: days ? Number(days) : null,
        allowedIps: parseIps(ips),
      },
      { onError: (e) => setError(e.message) },
    );
  };
  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent className="sm:max-w-lg">
        {key ? (
          <div className="grid gap-4">
            <DialogHeader>
              <DialogTitle>{t('Copy Your Key')}</DialogTitle>
              <DialogDescription>{t('This is the only time it is shown. Only a hash is stored.')}</DialogDescription>
            </DialogHeader>
            <div className="flex gap-2">
              <Input readOnly value={key} className="font-mono text-xs" onFocus={(e) => e.target.select()} />
              <CopyButton value={key} />
            </div>
            <p className="text-xs text-muted-foreground">
              {tx('Send it as {header}. The API is documented at {link}.', {
                header: <code>Authorization: Bearer …</code>,
                link: (
                  <a href="/api/v1/openapi.json" className="underline">
                    /api/v1/openapi.json
                  </a>
                ),
              })}
            </p>
            <DialogFooter>
              <Button onClick={() => close(false)}>{t('Done')}</Button>
            </DialogFooter>
          </div>
        ) : (
          <form onSubmit={submit} className="grid gap-4">
            <DialogHeader>
              <DialogTitle>{t('New API Key')}</DialogTitle>
              <DialogDescription>
                {t('A key acts as you, limited to what you tick here. It never gets more than your role allows.')}
              </DialogDescription>
            </DialogHeader>
            <FormError message={error} />
            <NameField value={name} onChange={setName} />
            <PermissionsField value={permissions} onChange={setPermissions} />
            <div className="grid gap-4 sm:grid-cols-2">
              <Field label={t('Expires After (Days)')} htmlFor="key-days" hint={t('Empty for no expiry')}>
                <Input id="key-days" type="number" min={1} max={3650} value={days} onChange={(e) => setDays(e.target.value)} />
              </Field>
              <IpsField value={ips} onChange={setIps} />
            </div>
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => close(false)}>
                {t('Cancel')}
              </Button>
              <Button type="submit" disabled={create.isPending || !permissions.length}>
                {t('Create Key')}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}

export function ApiKeysPage() {
  const keys = useApi('api_keys.list');
  const revoke = useApiMutation('api_keys.revoke', { invalidates: ['api_keys.'] });
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<ApiKey | null>(null);
  return (
    <SettingsCard
      help="api#api-keys"
      title={t('API Keys')}
      description={
        <>
          {tx(
            'For your own integrations. OpenSign speaks its own REST API at {rest}, and drop-in compatible Scrive ({scrive}) and DocuSign ({docusign}) APIs.',
            {
              rest: <code>/api/v1</code>,
              scrive: <code>/scrive/api/v2</code>,
              docusign: <code>/docusign/restapi/v2.1</code>,
            },
          )}{' '}
          {tx('Try calls in the {explorer} or read the {docs}.', {
            explorer: (
              <a href="/api-explorer" target="_blank" rel="noreferrer" className="text-primary hover:underline">
                {t('API Explorer')}
              </a>
            ),
            docs: (
              <Link to="/docs/api" className="text-primary hover:underline">
                {t('API guide')}
              </Link>
            ),
          })}
        </>
      }
    >
      <Button className="mb-4" onClick={() => setCreating(true)}>
        <Plus /> {t('New Key')}
      </Button>
      {keys.isPending ? (
        <Loading />
      ) : !keys.data?.length ? (
        <EmptyState icon={<KeyRound />} title={t('No API keys')} />
      ) : (
        <ul className="grid gap-2">
          {keys.data.map((k) => (
            <li key={k.id} className="flex items-center gap-3 rounded-md border px-3 py-2">
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="truncate font-medium">{k.name}</span>
                  <code className="text-xs text-muted-foreground">{k.prefix}…</code>
                </div>
                <div className="text-xs text-muted-foreground">
                  {k.permissions.join(', ')} ·{' '}
                  {k.lastUsedAt ? t('last used {date}', { date: formatDateTime(k.lastUsedAt) }) : t('never used')}
                  {k.expiresAt ? ` · ${t('expires {date}', { date: formatDate(k.expiresAt) })}` : ''}
                </div>
                <div className="text-xs text-muted-foreground">
                  {k.allowedIps.length
                    ? tx('from {ips}', { ips: <span className="font-mono">{k.allowedIps.join(', ')}</span> })
                    : t('from any IP address')}
                </div>
              </div>
              <div className="flex shrink-0">
                <IconButton label={t('Edit')} onClick={() => setEditing(k)}>
                  <Pencil />
                </IconButton>
                <ConfirmButton
                  label={t('Revoke')}
                  icon={<Trash2 />}
                  title={t('Revoke “{name}”?', { name: k.name })}
                  description={t('Anything using this key stops working at once.')}
                  onConfirm={() => revoke.mutate({ id: k.id })}
                />
              </div>
            </li>
          ))}
        </ul>
      )}
      <CreateDialog open={creating} onOpenChange={setCreating} />
      <EditDialog apiKey={editing} onOpenChange={(open) => !open && setEditing(null)} />
    </SettingsCard>
  );
}
