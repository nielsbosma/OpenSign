import { KeyRound, Plus } from 'lucide-react';
import { type FormEvent, useState } from 'react';
import { ConfirmButton, CopyButton, EmptyState, Field, Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { type Permission, useApi, useApiMutation } from '@/lib/api';
import { formatDate, formatDateTime } from '@/lib/format';
import { useSession } from '@/lib/session';
import { FormError } from '../auth/AuthLayout';
import { SettingsCard } from './SettingsLayout';

const PERMISSIONS: [Permission, string][] = [
  ['documents.read', 'Read your documents'],
  ['documents.create', 'Create, send and manage your documents'],
  ['documents.read_all', 'Read every document'],
  ['documents.manage_all', 'Manage every document'],
  ['templates.manage', 'Manage templates'],
  ['users.read', 'List users'],
  ['audit.read', 'Read the audit trail'],
  ['webhooks.manage', 'Manage webhooks'],
  ['org.read', 'Read organisation details'],
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
              <DialogTitle>Copy Your Key</DialogTitle>
              <DialogDescription>This is the only time it is shown. Only a hash is stored.</DialogDescription>
            </DialogHeader>
            <div className="flex gap-2">
              <Input readOnly value={key} className="font-mono text-xs" onFocus={(e) => e.target.select()} />
              <CopyButton value={key} />
            </div>
            <p className="text-xs text-muted-foreground">
              Send it as <code>Authorization: Bearer …</code>. The API is documented at{' '}
              <a href="/api/v1/openapi.json" className="underline">
                /api/v1/openapi.json
              </a>
              .
            </p>
            <DialogFooter>
              <Button onClick={() => close(false)}>Done</Button>
            </DialogFooter>
          </div>
        ) : (
          <form onSubmit={submit} className="grid gap-4">
            <DialogHeader>
              <DialogTitle>New API Key</DialogTitle>
              <DialogDescription>
                A key acts as you, limited to what you tick here. It never gets more than your role allows.
              </DialogDescription>
            </DialogHeader>
            <FormError message={error} />
            <Field label="Name" htmlFor="key-name">
              <Input
                id="key-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="CRM integration"
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
                  <span>{label}</span>
                </label>
              ))}
            </div>
            <div className="grid gap-4 sm:grid-cols-2">
              <Field label="Expires After (Days)" htmlFor="key-days" hint="Empty for no expiry">
                <Input id="key-days" type="number" min={1} max={3650} value={days} onChange={(e) => setDays(e.target.value)} />
              </Field>
              <Field label="Allowed IPs" htmlFor="key-ips" hint="Addresses or CIDR ranges; empty for any">
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
                Cancel
              </Button>
              <Button type="submit" disabled={create.isPending || !permissions.length}>
                Create Key
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
      title="API Keys"
      description={
        <>
          For your own integrations. OpenSign speaks its own REST API at <code>/api/v1</code>, and drop-in compatible Scrive (
          <code>/scrive/api/v2</code>) and DocuSign (<code>/docusign/restapi/v2.1</code>) APIs.
        </>
      }
    >
      <Button className="mb-4" onClick={() => setCreating(true)}>
        <Plus /> New Key
      </Button>
      {keys.isPending ? (
        <Spinner />
      ) : !keys.data?.length ? (
        <EmptyState icon={<KeyRound />} title="No API keys" />
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
                  {k.permissions.join(', ')} · {k.lastUsedAt ? `last used ${formatDateTime(k.lastUsedAt)}` : 'never used'}
                  {k.expiresAt ? ` · expires ${formatDate(k.expiresAt)}` : ''}
                </div>
              </div>
              <ConfirmButton
                label="Revoke"
                title={`Revoke “${k.name}”?`}
                description="Anything using this key stops working at once."
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
