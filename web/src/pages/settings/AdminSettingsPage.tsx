import { type FormEvent, useEffect, useState } from 'react';
import { Navigate, useParams } from 'react-router';
import { toast } from 'sonner';
import { ConfirmButton, Field, Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { type Group, useApi, useApiMutation } from '@/lib/api';
import { formatDate, formatDateTime } from '@/lib/format';
import { SettingsCard } from './SettingsLayout';

type Spec = {
  key: string;
  label: string;
  type?: 'text' | 'secret' | 'bool' | 'number' | 'select' | 'file' | 'color';
  hint?: string;
  options?: [string, string][];
  placeholder?: string;
  /** Shown only when this other boolean or text field is set. */
  when?: string;
};

const GROUPS: Record<Group, { title: string; description: string; fields: Spec[] }> = {
  org: {
    title: 'Organisation',
    description: 'Shown to the people you send documents to, in emails and on the signing page.',
    fields: [
      { key: 'name', label: 'Name' },
      { key: 'registrationNumber', label: 'Registration Number' },
      { key: 'country', label: 'Country', placeholder: 'SE' },
      { key: 'timeZone', label: 'Time Zone', placeholder: 'Europe/Stockholm' },
      { key: 'locale', label: 'Language', placeholder: 'en' },
      { key: 'brandColor', label: 'Brand Color', type: 'color' },
    ],
  },
  email: {
    title: 'Email (Resend)',
    description: 'Invitations, codes and signed documents go out through Resend. Changing the key or sender needs a new test email.',
    fields: [
      { key: 'apiKey', label: 'API Key', type: 'secret' },
      { key: 'fromAddress', label: 'From Address', placeholder: 'sign@example.com' },
      { key: 'fromName', label: 'From Name' },
      { key: 'replyTo', label: 'Reply-To', hint: 'Empty: replies go to the sender of each document.' },
    ],
  },
  security: {
    title: 'Security',
    description: 'Sessions, uploads and how long documents are kept.',
    fields: [
      { key: 'enforceTwoFactor', label: 'Require two-factor authentication for everyone', type: 'bool' },
      { key: 'sessionHours', label: 'Session Length (Hours)', type: 'number' },
      { key: 'idleMinutes', label: 'Sign Out After Idle (Minutes)', type: 'number' },
      { key: 'signerLinkDays', label: 'Signing Links Expire After (Days)', type: 'number' },
      { key: 'maxUploadMb', label: 'Largest Upload (MB)', type: 'number' },
      { key: 'retentionDays', label: 'Delete Completed Documents After (Days)', type: 'number', hint: '0 keeps them.' },
    ],
  },
  signing: {
    title: 'Signing and Seal',
    description: 'Completed documents are sealed with your organisation seal and a timestamp from a trusted timestamp authority.',
    fields: [
      { key: 'sealName', label: 'Seal Name', hint: 'Shown as the signer of the seal. Empty: the organisation name.' },
      { key: 'reason', label: 'Signature Reason' },
      { key: 'tsaUrl', label: 'Timestamp Authority (RFC 3161)', placeholder: 'http://timestamp.digicert.com' },
    ],
  },
  ai: {
    title: 'AI',
    description: 'Optional. Connect a language model for the assistant and document reviews. Nothing is sent until this is set up.',
    fields: [
      {
        key: 'provider',
        label: 'Provider',
        type: 'select',
        options: [
          ['', 'Off'],
          ['anthropic', 'Anthropic'],
          ['openai', 'OpenAI'],
          ['berget', 'Berget AI (EU)'],
          ['opper', 'Opper (EU)'],
          ['litellm', 'LiteLLM proxy'],
          ['openai-compatible', 'Other OpenAI-compatible'],
        ],
      },
      { key: 'model', label: 'Model', placeholder: 'claude-sonnet-5-5', when: 'provider' },
      { key: 'apiKey', label: 'API Key', type: 'secret', when: 'provider' },
      { key: 'baseUrl', label: 'Base URL', hint: 'Only for LiteLLM and other compatible servers.', when: 'provider' },
      { key: 'allowDocumentText', label: 'Allow document text to be sent for reviews', type: 'bool', when: 'provider' },
      { key: 'monthlyTokenBudget', label: 'Monthly Token Budget', type: 'number', hint: '0 means no limit.', when: 'provider' },
    ],
  },
  sms: {
    title: 'SMS',
    description: 'For SMS invitations and one-time codes.',
    fields: [
      {
        key: 'provider',
        label: 'Provider',
        type: 'select',
        options: [
          ['', 'Off'],
          ['46elks', '46elks'],
          ['twilio', 'Twilio'],
        ],
      },
      { key: 'username', label: 'Username or Account SID', when: 'provider' },
      { key: 'password', label: 'Password or Auth Token', type: 'secret', when: 'provider' },
      { key: 'sender', label: 'Sender', hint: 'A name of up to 11 letters, or a number you own.', when: 'provider' },
    ],
  },
  eid: {
    title: 'BankID and eID',
    description: 'Let parties identify with Swedish BankID, Freja eID, or other Nordic eIDs through a broker.',
    fields: [
      { key: 'bankidEnabled', label: 'Swedish BankID', type: 'bool' },
      { key: 'bankidTest', label: 'Use the BankID test environment', type: 'bool', when: 'bankidEnabled' },
      { key: 'bankidCertificate', label: 'Relying-Party Certificate (.p12)', type: 'file', when: 'bankidEnabled' },
      { key: 'bankidCertificatePassword', label: 'Certificate Password', type: 'secret', when: 'bankidEnabled' },
      { key: 'frejaEnabled', label: 'Freja eID', type: 'bool' },
      { key: 'frejaTest', label: 'Use the Freja test environment', type: 'bool', when: 'frejaEnabled' },
      { key: 'frejaCertificate', label: 'Client Certificate (.p12)', type: 'file', when: 'frejaEnabled' },
      { key: 'frejaCertificatePassword', label: 'Certificate Password', type: 'secret', when: 'frejaEnabled' },
      { key: 'brokerEnabled', label: 'eID broker (OpenID Connect)', type: 'bool' },
      { key: 'brokerIssuer', label: 'Issuer URL', placeholder: 'https://example.idura.broker', when: 'brokerEnabled' },
      { key: 'brokerClientId', label: 'Client ID', when: 'brokerEnabled' },
      { key: 'brokerClientSecret', label: 'Client Secret', type: 'secret', when: 'brokerEnabled' },
    ],
  },
};

function readBase64(file: File) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result).split(',')[1] ?? '');
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}

function EmailExtras() {
  const status = useApi('setup.status', undefined, { staleTime: 0 });
  const test = useApiMutation('settings.send_test_email', {
    invalidates: ['setup.status'],
    onSuccess: () => toast.success('Test email sent'),
  });
  return (
    <div className="flex items-center gap-3">
      <Button variant="outline" onClick={() => test.mutate({})} disabled={test.isPending}>
        {test.isPending && <Spinner />} Send Test Email
      </Button>
      <span className={status.data?.emailConfigured ? 'text-success' : 'text-warning'}>
        {status.data?.emailConfigured ? 'Working' : 'Not verified yet'}
      </span>
    </div>
  );
}

function SealExtras() {
  const seal = useApi('seal.get');
  const regenerate = useApiMutation('seal.regenerate', { invalidates: ['seal.'] });
  return (
    <SettingsCard title="Seal Certificate" description="The key that seals completed documents. It is generated here and stored encrypted.">
      {seal.data ? (
        <div className="grid gap-1">
          <div className="font-medium">{seal.data.subject}</div>
          <div className="text-muted-foreground">
            Created {formatDateTime(seal.data.createdAt)} · valid until {formatDate(seal.data.notAfter)}
          </div>
        </div>
      ) : (
        <p className="text-muted-foreground">Created automatically when the first document completes.</p>
      )}
      <div className="mt-4">
        <ConfirmButton
          label="Replace Seal Key"
          title="Replace the seal key?"
          description="New documents are sealed with a new key. Documents sealed before stay valid and verifiable."
          onConfirm={() => regenerate.mutate({})}
        />
      </div>
    </SettingsCard>
  );
}

function AiExtras() {
  const usage = useApi('ai.usage');
  const test = useApiMutation('ai.test', { onSuccess: (out) => toast.success(`The model answered in ${out.milliseconds} ms`) });
  return (
    <div className="flex flex-wrap items-center gap-3">
      <Button variant="outline" onClick={() => test.mutate({})} disabled={test.isPending}>
        {test.isPending && <Spinner />} Test Connection
      </Button>
      {usage.data && (
        <span className="text-muted-foreground">
          {usage.data.tokensThisMonth.toLocaleString()} tokens this month
          {usage.data.monthlyBudget ? ` of ${usage.data.monthlyBudget.toLocaleString()}` : ''}
        </span>
      )}
    </div>
  );
}

export function AdminSettingsPage() {
  const { group = '' } = useParams();
  const spec = GROUPS[group as Group];
  const current = useApi('settings.get', { group: group as Group }, { enabled: !!spec });
  const [draft, setDraft] = useState<Record<string, unknown>>({});
  const save = useApiMutation('settings.update', {
    invalidates: ['settings.', 'ai.', 'setup.'],
    onSuccess: () => {
      setDraft({});
      toast.success('Saved');
    },
  });
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new group starts with no unsaved changes
  useEffect(() => setDraft({}), [group]);

  if (!spec) return <Navigate to="/settings" replace />;
  if (current.isPending) return <Spinner />;
  const saved = (current.data?.values ?? {}) as Record<string, unknown>;
  const pinned = current.data?.pinned ?? [];
  const value = (key: string) => (key in draft ? draft[key] : saved[key]);
  const set = (key: string, v: unknown) => setDraft({ ...draft, [key]: v });

  const submit = (e: FormEvent) => {
    e.preventDefault();
    save.mutate({ group: group as Group, values: draft });
  };

  return (
    <div className="grid gap-6">
      <SettingsCard title={spec.title} description={spec.description}>
        <form onSubmit={submit} className="grid max-w-xl gap-5">
          {spec.fields
            .filter((f) => !f.when || !!value(f.when))
            .map((f) => {
              const locked = pinned.includes(f.key);
              const hint = locked ? 'Set by an environment variable.' : f.hint;
              const id = `setting-${f.key}`;
              if (f.type === 'bool') {
                return (
                  <label key={f.key} className="flex items-center justify-between gap-4">
                    <span>{f.label}</span>
                    <Switch checked={!!value(f.key)} onCheckedChange={(c) => set(f.key, c)} disabled={locked} />
                  </label>
                );
              }
              if (f.type === 'select') {
                return (
                  <Field key={f.key} label={f.label} hint={hint}>
                    <Select
                      value={String(value(f.key) ?? '') || 'off'}
                      onValueChange={(v) => set(f.key, v === 'off' ? '' : v)}
                      disabled={locked}
                    >
                      <SelectTrigger className="w-full">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {f.options!.map(([v, label]) => (
                          <SelectItem key={v || 'off'} value={v || 'off'}>
                            {label}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </Field>
                );
              }
              if (f.type === 'secret' || f.type === 'file') {
                const isSet = saved[f.key] === true;
                return (
                  <Field key={f.key} label={f.label} htmlFor={id} hint={hint ?? (isSet ? 'Saved. Leave empty to keep it.' : undefined)}>
                    <div className="flex gap-2">
                      {f.type === 'file' ? (
                        <Input
                          id={id}
                          type="file"
                          accept=".p12,.pfx"
                          disabled={locked}
                          onChange={async (e) => {
                            const file = e.target.files?.[0];
                            if (file) set(f.key, await readBase64(file));
                          }}
                        />
                      ) : (
                        <Input
                          id={id}
                          type="password"
                          autoComplete="off"
                          value={typeof draft[f.key] === 'string' ? (draft[f.key] as string) : ''}
                          onChange={(e) => set(f.key, e.target.value || null)}
                          placeholder={isSet ? '••••••••' : undefined}
                          disabled={locked}
                        />
                      )}
                      {isSet && !locked && (
                        <Button type="button" variant="ghost" className="text-destructive" onClick={() => set(f.key, '')}>
                          Clear
                        </Button>
                      )}
                    </div>
                  </Field>
                );
              }
              return (
                <Field key={f.key} label={f.label} htmlFor={id} hint={hint}>
                  <Input
                    id={id}
                    type={f.type === 'number' ? 'number' : f.type === 'color' ? 'color' : 'text'}
                    min={f.type === 'number' ? 0 : undefined}
                    value={String(value(f.key) ?? '')}
                    onChange={(e) => set(f.key, f.type === 'number' ? Number(e.target.value) : e.target.value)}
                    placeholder={f.placeholder}
                    disabled={locked}
                    className={f.type === 'color' ? 'h-9 w-20 p-1' : undefined}
                  />
                </Field>
              );
            })}
          <div className="flex gap-2">
            <Button type="submit" disabled={save.isPending || !Object.keys(draft).length}>
              Save
            </Button>
            {Object.keys(draft).length > 0 && (
              <Button type="button" variant="ghost" onClick={() => setDraft({})}>
                Discard
              </Button>
            )}
          </div>
        </form>
        {(group === 'email' || group === 'ai') && (
          <div className="mt-6 border-t pt-5">{group === 'email' ? <EmailExtras /> : <AiExtras />}</div>
        )}
      </SettingsCard>
      {group === 'signing' && <SealExtras />}
    </div>
  );
}
