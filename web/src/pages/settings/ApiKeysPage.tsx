import { KeyRound, Plus } from 'lucide-react';
import { type FormEvent, useState } from 'react';
import { ConfirmButton, CopyButton, EmptyState, Field, Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { type Permission, useApi, useApiMutation } from '@/lib/api';
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

function CreateDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const { can } = useSession();
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
        allowedIps: ips
          .split(/[\s,]+/)
          .map((s) => s.trim())
          .filter(Boolean),
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
            <Field label={t('Name')} htmlFor="key-name">
              <Input
                id="key-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={t('CRM integration')}
                required
                autoFocus
              />
            </Field>
            <div className="grid gap-2">
              {PERMISSIONS.filter(([p]) => can(p)).map(([p, label]) => (
                <label key={p} className="flex items-center gap-2.5">
                  <Checkbox
                    checked={permissions.includes(p)}
                    onCheckedChange={(c) => setPermissions(c ? [...permissions, p] : permissions.filter((x) => x !== p))}
                  />
                  <span>{t(label)}</span>
                </label>
              ))}
            </div>
            <div className="grid gap-4 sm:grid-cols-2">
              <Field label={t('Expires After (Days)')} htmlFor="key-days" hint={t('Empty for no expiry')}>
                <Input id="key-days" type="number" min={1} max={3650} value={days} onChange={(e) => setDays(e.target.value)} />
              </Field>
              <Field label={t('Allowed IPs')} htmlFor="key-ips" hint={t('Addresses or CIDR ranges; empty for any')}>
                <Input
                  id="key-ips"
                  value={ips}
                  onChange={(e) => setIps(e.target.value)}
                  placeholder="203.0.113.0/24"
                  className="font-mono"
                />
              </Field>
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
  return (
    <SettingsCard
      title={t('API Keys')}
      description={tx(
        'For your own integrations. OpenSign speaks its own REST API at {rest}, and drop-in compatible Scrive ({scrive}) and DocuSign ({docusign}) APIs.',
        {
          rest: <code>/api/v1</code>,
          scrive: <code>/scrive/api/v2</code>,
          docusign: <code>/docusign/restapi/v2.1</code>,
        },
      )}
    >
      <Button className="mb-4" onClick={() => setCreating(true)}>
        <Plus /> {t('New Key')}
      </Button>
      {keys.isPending ? (
        <Spinner />
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
              </div>
              <ConfirmButton
                label={t('Revoke')}
                title={t('Revoke “{name}”?', { name: k.name })}
                description={t('Anything using this key stops working at once.')}
                onConfirm={() => revoke.mutate({ id: k.id })}
              />
            </li>
          ))}
        </ul>
      )}
      <CreateDialog open={creating} onOpenChange={setCreating} />
    </SettingsCard>
  );
}
