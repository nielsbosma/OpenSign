import { type FormEvent, useEffect, useState } from 'react';
import { Navigate, useParams } from 'react-router';
import { toast } from 'sonner';
import { ConfirmButton, Field, Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { type Group, useApi, useApiMutation } from '@/lib/api';
import { formatDate, formatDateTime, LANGUAGES } from '@/lib/format';
import { SettingsCard } from './SettingsLayout';

type Spec = {
  key: string;
  label: string;
  /** `file` is a secret PEM or PKCS#12 file, `pem` a public one such as a CA certificate. */
  type?: 'text' | 'secret' | 'bool' | 'number' | 'select' | 'file' | 'pem' | 'color';
  hint?: string;
  options?: [string, string][];
  placeholder?: string;
  /** Shown only when this other boolean or text field is set. */
  when?: string;
  /** For a `file`: where the password of an uploaded .p12/.pfx goes. The server keeps only the PEM it unlocks. */
  passwordKey?: string;
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
      {
        key: 'locale',
        label: 'Language for Signers',
        type: 'select',
        options: LANGUAGES,
        hint: 'The default for new documents. Each document can use its own.',
      },
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
    description: 'Sessions, sign-in and uploads.',
    fields: [
      { key: 'enforceTwoFactor', label: 'Require two-factor authentication for everyone', type: 'bool' },
      { key: 'sessionHours', label: 'Session Length (Hours)', type: 'number' },
      { key: 'idleMinutes', label: 'Sign Out After Idle (Minutes)', type: 'number' },
      { key: 'signerLinkDays', label: 'Signing Links Expire After (Days)', type: 'number' },
      { key: 'maxUploadMb', label: 'Largest Upload (MB)', type: 'number' },
    ],
  },
  updates: {
    title: 'Updates',
    description: 'Check for new releases and update from the System page.',
    fields: [
      {
        key: 'checkForUpdates',
        label: 'Check for Updates',
        type: 'bool',
        hint: 'Asks GitHub for the latest release at most every six hours.',
      },
      {
        key: 'deployHookUrl',
        label: 'Deploy Hook',
        type: 'secret',
        placeholder: 'https://api.sliplane.io/deploy/…',
        hint: 'Calling it redeploys the service with its configured image tag, so use latest or a version series such as 1.2 rather than an exact version.',
      },
    ],
  },
  retention: {
    title: 'Retention',
    description:
      'Delete documents and their files automatically after a set time. 0 keeps them. Templates and documents on legal hold are always kept, and the audit trail keeps a record of every deletion.',
    fields: [
      { key: 'completedDays', label: 'Delete Signed Documents After (Days)', type: 'number', hint: 'Counted from completion.' },
      {
        key: 'endedDays',
        label: 'Delete Declined, Canceled and Expired Documents After (Days)',
        type: 'number',
        hint: 'Counted from when signing stopped.',
      },
      { key: 'draftDays', label: 'Delete Untouched Drafts After (Days)', type: 'number', hint: 'Counted from the last change.' },
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
    description:
      'Let parties identify with Swedish BankID (directly or through TIC Identity), Freja eID, or other Nordic eIDs through a broker.',
    fields: [
      { key: 'bankidEnabled', label: 'Swedish BankID', type: 'bool' },
      { key: 'bankidTest', label: 'Use the BankID test environment', type: 'bool', when: 'bankidEnabled' },
      {
        key: 'bankidCertificate',
        label: 'Relying-Party Certificate',
        type: 'file',
        hint: "The .p12 from your bank, or a PEM with the key. In the test environment, leave it empty to use BankID's public test certificate.",
        passwordKey: 'bankidCertificatePassword',
        when: 'bankidEnabled',
      },
      {
        key: 'bankidCaCertificate',
        label: 'BankID Server CA (optional)',
        type: 'pem',
        hint: "BankID's own root is built in. Only replace it if BankID tells you to.",
        when: 'bankidEnabled',
      },
      { key: 'frejaEnabled', label: 'Freja eID', type: 'bool' },
      { key: 'frejaTest', label: 'Use the Freja test environment', type: 'bool', when: 'frejaEnabled' },
      {
        key: 'frejaCertificate',
        label: 'Client Certificate',
        type: 'file',
        hint: 'The .pfx from Freja, or a PEM with the key.',
        passwordKey: 'frejaCertificatePassword',
        when: 'frejaEnabled',
      },
      {
        key: 'frejaSigningCertificate',
        label: 'Freja Result-Signing Certificate',
        type: 'pem',
        hint: 'From Freja for your environment. Results without a valid signature are refused.',
        when: 'frejaEnabled',
      },
      {
        key: 'frejaCaCertificate',
        label: 'Freja Server CA (optional)',
        type: 'pem',
        hint: 'Leave empty to trust the public CAs.',
        when: 'frejaEnabled',
      },
      { key: 'brokerEnabled', label: 'eID broker (OpenID Connect)', type: 'bool' },
      { key: 'brokerIssuer', label: 'Issuer URL', placeholder: 'https://example.idura.broker', when: 'brokerEnabled' },
      { key: 'brokerClientId', label: 'Client ID', when: 'brokerEnabled' },
      { key: 'brokerClientSecret', label: 'Client Secret', type: 'secret', when: 'brokerEnabled' },
      {
        key: 'brokerAcrValues',
        label: 'eID Choice (acr_values)',
        placeholder: 'urn:grn:authn:no:bankid',
        hint: 'Leave empty to let the signer choose at the broker. Register /api/eid/callback as the redirect URI.',
        when: 'brokerEnabled',
      },
      {
        key: 'ticEnabled',
        label: 'BankID Through TIC Identity',
        type: 'bool',
        hint: 'For Swedish BankID without an agreement with a bank. Used when Swedish BankID above is off.',
      },
      { key: 'ticTenant', label: 'Tenant', placeholder: 'your-tenant', hint: 'As in https://id.tic.io/your-tenant.', when: 'ticEnabled' },
      { key: 'ticClientId', label: 'Client ID', when: 'ticEnabled' },
      {
        key: 'ticClientSecret',
        label: 'Client Secret',
        type: 'secret',
        hint: 'Turn on OIDC for the tenant in the TIC portal and register /api/eid/callback as the redirect URI.',
        when: 'ticEnabled',
      },
    ],
  },
};

async function readCertificate(file: File) {
  const bytes = new Uint8Array(await file.arrayBuffer());
  const text = new TextDecoder().decode(bytes);
  if (text.includes('-----BEGIN ')) return text;
  let binary = '';
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary);
}

function isPkcs12(value: unknown) {
  return typeof value === 'string' && value !== '' && !value.includes('-----BEGIN ');
}

async function readPem(file: File) {
  const text = await file.text();
  if (!text.includes('-----BEGIN ')) {
    toast.error('That is not a PEM file. Convert it with openssl first.');
    return null;
  }
  return text;
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

function RetentionExtras() {
  const preview = useApi('retention.preview', {}, { staleTime: 0 });
  if (!preview.data) return null;
  const { completed, ended, drafts, held, oldestHeldSince } = preview.data;
  const due = completed + ended + drafts;
  return (
    <SettingsCard title="Next Run" description="Retention runs every ten minutes. Each deletion is recorded in the audit trail.">
      <div className="grid gap-1">
        <div>
          {due === 0
            ? 'Nothing is due for deletion with the saved rules.'
            : `${due.toLocaleString()} ${due === 1 ? 'document is' : 'documents are'} due: ${completed} signed, ${ended} declined, canceled or expired, ${drafts} drafts.`}
        </div>
        <div className="text-muted-foreground">
          {held === 0
            ? 'No documents are on legal hold.'
            : `${held} ${held === 1 ? 'document is' : 'documents are'} on legal hold${oldestHeldSince ? `, the oldest since ${formatDate(oldestHeldSince)}` : ''}.`}
        </div>
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
                          accept=".pem,.crt,.cer,.key,.p12,.pfx"
                          disabled={locked}
                          onChange={async (e) => {
                            const file = e.target.files?.[0];
                            if (file) set(f.key, await readCertificate(file));
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
                    {f.passwordKey && isPkcs12(draft[f.key]) && (
                      <Input
                        className="mt-2"
                        type="password"
                        autoComplete="off"
                        aria-label="Certificate file password"
                        placeholder="The file's password"
                        value={typeof draft[f.passwordKey] === 'string' ? (draft[f.passwordKey] as string) : ''}
                        onChange={(e) => set(f.passwordKey!, e.target.value)}
                      />
                    )}
                  </Field>
                );
              }
              if (f.type === 'pem') {
                const current = value(f.key);
                return (
                  <Field key={f.key} label={f.label} htmlFor={id} hint={current ? `Saved. ${hint ?? ''}` : hint}>
                    <div className="flex gap-2">
                      <Input
                        id={id}
                        type="file"
                        accept=".pem,.crt,.cer"
                        disabled={locked}
                        onChange={async (e) => {
                          const file = e.target.files?.[0];
                          const pem = file && (await readPem(file));
                          if (pem) set(f.key, pem);
                        }}
                      />
                      {!!current && !locked && (
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
      {group === 'retention' && <RetentionExtras />}
    </div>
  );
}
