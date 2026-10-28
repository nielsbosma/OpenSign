import { Plus, Webhook } from 'lucide-react';
import { type FormEvent, useState } from 'react';
import { toast } from 'sonner';
import { ConfirmButton, CopyButton, EmptyState, Field, Spinner, StatusDot } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { useApi, useApiMutation } from '@/lib/api';
import { FormError } from '../auth/AuthLayout';
import { SettingsCard } from './SettingsLayout';

const EVENTS = ['document.sent', 'document.signed', 'document.completed', 'document.declined', 'document.canceled', 'document.expired'];

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
              <DialogTitle>Signing Secret</DialogTitle>
              <DialogDescription>
                Check the <code>OpenSign-Signature</code> header of every delivery with this secret. It is shown only now.
              </DialogDescription>
            </DialogHeader>
            <div className="flex gap-2">
              <Input readOnly value={secret} className="font-mono text-xs" onFocus={(e) => e.target.select()} />
              <CopyButton value={secret} />
            </div>
            <DialogFooter>
              <Button onClick={() => close(false)}>Done</Button>
            </DialogFooter>
          </div>
        ) : (
          <form onSubmit={submit} className="grid gap-4">
            <DialogHeader>
              <DialogTitle>New Webhook</DialogTitle>
              <DialogDescription>OpenSign posts signed JSON events to a public https address, with retries.</DialogDescription>
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
                <span>All events</span>
              </label>
              {!all &&
                EVENTS.map((ev) => (
                  <label key={ev} className="flex items-center gap-2.5 pl-6">
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
                Cancel
              </Button>
              <Button type="submit" disabled={create.isPending || !events.length}>
                Create Webhook
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
  const test = useApiMutation('webhooks.test', { invalidates: ['webhooks.'], onSuccess: () => toast.success('Test event queued') });
  const [creating, setCreating] = useState(false);
  return (
    <SettingsCard title="Webhooks" description="Tell your systems when documents are sent, signed, completed or stopped.">
      <Button className="mb-4" onClick={() => setCreating(true)}>
        <Plus /> New Webhook
      </Button>
      {hooks.isPending ? (
        <Spinner />
      ) : !hooks.data?.length ? (
        <EmptyState icon={<Webhook />} title="No webhooks" />
      ) : (
        <ul className="grid gap-2">
          {hooks.data.map((h) => (
            <li key={h.id} className="flex items-center gap-3 rounded-md border px-3 py-2">
              <StatusDot tone={!h.active ? 'off' : h.lastError ? 'error' : h.lastStatus ? 'ok' : 'off'} />
              <div className="min-w-0 flex-1">
                <div className="truncate font-mono text-xs">{h.url}</div>
                <div className="truncate text-xs text-muted-foreground">
                  {h.events.join(', ')}
                  {h.lastError ? ` · ${h.lastError}` : h.lastStatus ? ` · last ${h.lastStatus}` : ''}
                </div>
              </div>
              <Button variant="ghost" size="sm" onClick={() => test.mutate({ id: h.id })}>
                Test
              </Button>
              <ConfirmButton
                label="Delete"
                title="Delete this webhook?"
                description="Deliveries to it stop at once."
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
