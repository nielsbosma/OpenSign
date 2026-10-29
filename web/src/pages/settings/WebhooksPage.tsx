import { History, Plus, Webhook } from 'lucide-react';
import { type FormEvent, useState } from 'react';
import { Link } from 'react-router';
import { toast } from 'sonner';
import { ConfirmButton, CopyButton, EmptyState, Field, Loading, StatusDot } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { useApi, useApiMutation } from '@/lib/api';
import { t, tx } from '@/lib/i18n';
import { FormError } from '../auth/AuthLayout';
import { SettingsCard } from './SettingsLayout';

const EVENTS = [
  'document.created',
  'document.sent',
  'document.viewed',
  'document.signed',
  'document.completed',
  'document.declined',
  'document.canceled',
  'document.expired',
];

function CreateDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const [url, setUrl] = useState('');
  const [events, setEvents] = useState<string[]>(['*']);
  const [secret, setSecret] = useState<string | null>(null);
  const [error, setError] = useState<string>();
  const create = useApiMutation('webhooks.create', { invalidates: ['webhooks.'], silent: true, onSuccess: (out) => setSecret(out.secret) });
  const close = (open: boolean) => {
    onOpenChange(open);
    if (!open) {
      setSecret(null);
      setUrl('');
      setError(undefined);
    }
  };
  const submit = (e: FormEvent) => {
    e.preventDefault();
    setError(undefined);
    create.mutate({ url: url.trim(), events }, { onError: (e) => setError(e.message) });
  };
  const all = events.includes('*');
  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent className="sm:max-w-lg">
        {secret ? (
          <div className="grid gap-4">
            <DialogHeader>
              <DialogTitle>{t('Signing Secret')}</DialogTitle>
              <DialogDescription>
                {tx('Check the {header} header of every delivery with this secret. It is shown only now.', {
                  header: <code>OpenSign-Signature</code>,
                })}
              </DialogDescription>
            </DialogHeader>
            <div className="flex gap-2">
              <Input readOnly value={secret} className="font-mono text-xs" onFocus={(e) => e.target.select()} />
              <CopyButton value={secret} />
            </div>
            <DialogFooter>
              <Button onClick={() => close(false)}>{t('Done')}</Button>
            </DialogFooter>
          </div>
        ) : (
          <form onSubmit={submit} className="grid gap-4">
            <DialogHeader>
              <DialogTitle>{t('New Webhook')}</DialogTitle>
              <DialogDescription>{t('OpenSign posts signed JSON events to a public https address, with retries.')}</DialogDescription>
            </DialogHeader>
            <FormError message={error} />
            <Field label="URL" htmlFor="hook-url">
              <Input
                id="hook-url"
                type="url"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="https://example.com/opensign"
                required
                autoFocus
              />
            </Field>
            <div className="grid gap-2">
              <label className="flex items-center gap-2.5">
                <Checkbox checked={all} onCheckedChange={(c) => setEvents(c ? ['*'] : [])} />
                <span>{t('All events')}</span>
              </label>
              {!all &&
                EVENTS.map((ev) => (
                  <label key={ev} className="flex items-center gap-2.5 ps-6">
                    <Checkbox
                      checked={events.includes(ev)}
                      onCheckedChange={(c) => setEvents(c ? [...events, ev] : events.filter((x) => x !== ev))}
                    />
                    <code className="text-xs">{ev}</code>
                  </label>
                ))}
            </div>
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => close(false)}>
                {t('Cancel')}
              </Button>
              <Button type="submit" disabled={create.isPending || !events.length}>
                {t('Create Webhook')}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}

export function WebhooksPage() {
  const hooks = useApi('webhooks.list');
  const remove = useApiMutation('webhooks.delete', { invalidates: ['webhooks.'] });
  const test = useApiMutation('webhooks.test', { invalidates: ['webhooks.'], onSuccess: () => toast.success(t('Test event queued')) });
  const [creating, setCreating] = useState(false);
  return (
    <SettingsCard
      help="webhooks"
      title={t('Webhooks')}
      description={t('Tell your systems when documents are sent, signed, completed or stopped.')}
    >
      <Button className="mb-4" onClick={() => setCreating(true)}>
        <Plus /> {t('New Webhook')}
      </Button>
      {hooks.isPending ? (
        <Loading />
      ) : !hooks.data?.length ? (
        <EmptyState icon={<Webhook />} title={t('No webhooks')} />
      ) : (
        <ul className="grid gap-2">
          {hooks.data.map((h) => (
            <li key={h.id} className="flex items-center gap-3 rounded-md border px-3 py-2">
              <StatusDot tone={!h.active ? 'off' : h.lastError ? 'error' : h.lastStatus ? 'ok' : 'off'} />
              <div className="min-w-0 flex-1">
                <div className="truncate font-mono text-xs">{h.url}</div>
                <div className="truncate text-xs text-muted-foreground">
                  {h.events.join(', ')}
                  {h.lastError ? ` · ${h.lastError}` : h.lastStatus ? ` · ${t('last {status}', { status: h.lastStatus })}` : ''}
                </div>
              </div>
              {h.failed > 0 && (
                <Link to={`/settings/webhooks/${h.id}`} className="shrink-0 text-destructive text-xs hover:underline">
                  {t('Failed: {count}', { count: h.failed })}
                </Link>
              )}
              <Button variant="ghost" size="sm" asChild>
                <Link to={`/settings/webhooks/${h.id}`}>
                  <History /> {t('Deliveries')}
                </Link>
              </Button>
              <Button variant="ghost" size="sm" onClick={() => test.mutate({ id: h.id })}>
                {t('Test')}
              </Button>
              <ConfirmButton
                label={t('Delete')}
                title={t('Delete this webhook?')}
                description={t('Deliveries to it stop at once.')}
                onConfirm={() => remove.mutate({ id: h.id })}
              />
            </li>
          ))}
        </ul>
      )}
      <CreateDialog open={creating} onOpenChange={setCreating} />
    </SettingsCard>
  );
}
