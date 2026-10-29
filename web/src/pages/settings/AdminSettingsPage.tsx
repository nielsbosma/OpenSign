import { type FormEvent, useEffect, useState } from 'react';
import { Navigate, useParams } from 'react-router';
import { toast } from 'sonner';
import { ConfirmButton, Field, Spinner } from '@/components/common';
import { SearchSelect } from '@/components/SearchSelect';
import { Button } from '@/components/ui/button';
import { FilePicker } from '@/components/ui/file-picker';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { errorMessage, type Group, useApi, useApiMutation } from '@/lib/api';
import { formatDate, formatDateTime, languages } from '@/lib/format';
import { msg, t } from '@/lib/i18n';
import { countryOptions, timeZoneOptions } from '@/lib/regions';
import { SettingsCard } from './SettingsLayout';

type Spec = {
  key: string;
  label: string;
  /** `file` is a secret PEM or PKCS#12 file, `pem` a public one such as a CA certificate. */
  type?: 'text' | 'secret' | 'bool' | 'number' | 'select' | 'file' | 'pem' | 'color' | 'country' | 'timeZone' | 'aiModel';
  hint?: string;
  options?: [string, string][] | (() => [string, string][]);
  placeholder?: string;
  /** Shown only when this other boolean or text field is set, or equals `is` when given. */
  when?: string;
  is?: string;
  /** For a `file`: where the password of an uploaded .p12/.pfx goes. The server keeps only the PEM it unlocks. */
  passwordKey?: string;
};

const GROUPS: Record<Group, { title: string; description: string; help?: string; fields: Spec[] }> = {
  org: {
    help: 'settings#organisation',
    title: msg('Organisation'),
    description: msg('Shown to the people you send documents to, in emails and on the signing page.'),
    fields: [
      { key: 'name', label: msg('Name') },
      { key: 'registrationNumber', label: msg('Registration Number') },
      { key: 'country', label: msg('Country'), type: 'country' },
      { key: 'timeZone', label: msg('Time Zone'), type: 'timeZone' },
      {
        key: 'locale',
        label: msg('Language for Signers'),
        type: 'select',
        options: languages,
        hint: msg('The default for new documents. Each document can use its own.'),
      },
      { key: 'brandColor', label: msg('Brand Color'), type: 'color' },
    ],
  },
  email: {
    help: 'setup#step-2-email',
    title: msg('Email'),
    description: msg(
      'Invitations, codes and signed documents go out through Resend or your own SMTP server. Changing the provider, server, key or sender needs a new test email.',
    ),
    fields: [
      {
        key: 'provider',
        label: msg('Provider'),
        type: 'select',
        options: [
          ['resend', 'Resend'],
          ['smtp', 'SMTP'],
        ],
      },
      { key: 'apiKey', label: msg('API Key'), type: 'secret', when: 'provider', is: 'resend' },
      { key: 'smtpHost', label: msg('SMTP Server'), placeholder: 'smtp.example.com', when: 'provider', is: 'smtp' },
      {
        key: 'smtpPort',
        label: msg('Port'),
        type: 'number',
        hint: msg('Usually 587 for STARTTLS or 465 for TLS.'),
        when: 'provider',
        is: 'smtp',
      },
      {
        key: 'smtpSecurity',
        label: msg('Security'),
        type: 'select',
        options: [
          ['starttls', 'STARTTLS'],
          ['tls', 'TLS'],
        ],
        hint: msg('The certificate must be valid for the server name. Unencrypted SMTP is not supported.'),
        when: 'provider',
        is: 'smtp',
      },
      { key: 'smtpUsername', label: msg('Username'), hint: msg('Empty: send without signing in.'), when: 'provider', is: 'smtp' },
      { key: 'smtpPassword', label: msg('Password'), type: 'secret', when: 'provider', is: 'smtp' },
      { key: 'fromAddress', label: msg('From Address'), placeholder: 'sign@example.com' },
      { key: 'fromName', label: msg('From Name') },
      { key: 'replyTo', label: msg('Reply-To'), hint: msg('Empty: replies go to the sender of each document.') },
    ],
  },
  security: {
    help: 'settings#security',
    title: msg('Security'),
    description: msg('Sessions, sign-in and uploads.'),
    fields: [
      { key: 'enforceTwoFactor', label: msg('Require two-factor authentication for everyone'), type: 'bool' },
      { key: 'sessionHours', label: msg('Session Length (Hours)'), type: 'number' },
      { key: 'idleMinutes', label: msg('Sign Out After Idle (Minutes)'), type: 'number' },
      { key: 'signerLinkDays', label: msg('Signing Links Expire After (Days)'), type: 'number' },
      { key: 'maxUploadMb', label: msg('Largest Upload (MB)'), type: 'number' },
      {
        key: 'reviewLinkDays',
        label: msg('AI Review Links Expire After (Days)'),
        type: 'number',
        hint: msg('Signers can give their own AI a read-only copy of a document. 0 turns this off; at most 30.'),
      },
    ],
  },
  updates: {
    title: msg('Updates'),
    description: msg('How OpenSign finds and installs new releases.'),
    fields: [
      {
        key: 'checkForUpdates',
        label: msg('Check for Updates'),
        type: 'bool',
        hint: msg('Asks GitHub for the latest release at most every six hours.'),
      },
      {
        key: 'deployHookUrl',
        label: msg('Deploy Hook'),
        type: 'secret',
        placeholder: 'https://api.sliplane.io/deploy/…',
        hint: msg(
          'Calling it redeploys the service with its configured image tag, so use latest or a version series such as 1.2 rather than an exact version.',
        ),
      },
      {
        key: 'updateUrl',
        label: msg('Update Link'),
        placeholder: 'https://sliplane.opendeploy.cloud/update?…',
        hint: msg(
          'A page that installs a release given as ref, such as the link the Sliplane Deployer sets. Used when there is no deploy hook.',
        ),
      },
    ],
  },
  retention: {
    help: 'retention',
    title: msg('Retention'),
    description: msg(
      'Delete documents and their files automatically after a set time. 0 keeps them. Templates and documents on legal hold are always kept, and the audit trail keeps a record of every deletion.',
    ),
    fields: [
      { key: 'completedDays', label: msg('Delete Signed Documents After (Days)'), type: 'number', hint: msg('Counted from completion.') },
      {
        key: 'endedDays',
        label: msg('Delete Declined, Canceled and Expired Documents After (Days)'),
        type: 'number',
        hint: msg('Counted from when signing stopped.'),
      },
      { key: 'draftDays', label: msg('Delete Untouched Drafts After (Days)'), type: 'number', hint: msg('Counted from the last change.') },
    ],
  },
  signing: {
    help: 'sealed-pdfs',
    title: msg('Signing and Seal'),
    description: msg('Completed documents are sealed with your organisation seal and a timestamp from a trusted timestamp authority.'),
    fields: [
      { key: 'sealName', label: msg('Seal Name'), hint: msg('Shown as the signer of the seal. Empty: the organisation name.') },
      { key: 'reason', label: msg('Signature Reason') },
      { key: 'tsaUrl', label: msg('Timestamp Authority (RFC 3161)'), placeholder: 'http://timestamp.digicert.com' },
    ],
  },
  ai: {
    help: 'ai',
    title: msg('AI'),
    description: msg('Optional. Connect a language model for the assistant and document reviews. Nothing is sent until this is set up.'),
    fields: [
      {
        key: 'provider',
        label: msg('Provider'),
        type: 'select',
        options: [
          ['', msg('Off')],
          ['anthropic', 'Anthropic'],
          ['openai', 'OpenAI'],
          ['berget', 'Berget AI (EU)'],
          ['opper', 'Opper (EU)'],
          ['litellm', msg('LiteLLM proxy')],
          ['openai-compatible', msg('Other OpenAI-compatible')],
        ],
      },
      { key: 'apiKey', label: msg('API Key'), type: 'secret', when: 'provider' },
      { key: 'baseUrl', label: msg('Base URL'), hint: msg('Only for LiteLLM and other compatible servers.'), when: 'provider' },
      { key: 'allowDocumentText', label: msg('Allow document text to be sent for reviews'), type: 'bool', when: 'provider' },
      { key: 'monthlyTokenBudget', label: msg('Monthly Token Budget'), type: 'number', hint: msg('0 means no limit.'), when: 'provider' },
      { key: 'model', label: msg('Model'), type: 'aiModel', when: 'provider' },
    ],
  },
  sms: {
    help: 'identification#sms-codes',
    title: msg('SMS'),
    description: msg('For SMS invitations and one-time codes.'),
    fields: [
      {
        key: 'provider',
        label: msg('Provider'),
        type: 'select',
        options: [
          ['', msg('Off')],
          ['46elks', '46elks'],
          ['twilio', 'Twilio'],
        ],
      },
      { key: 'username', label: msg('Username or Account SID'), when: 'provider' },
      { key: 'password', label: msg('Password or Auth Token'), type: 'secret', when: 'provider' },
      { key: 'sender', label: msg('Sender'), hint: msg('A name of up to 11 letters, or a number you own.'), when: 'provider' },
    ],
  },
  eid: {
    help: 'identification',
    title: msg('BankID and eID'),
    description: msg(
      'Let parties identify with Swedish BankID (directly or through TIC Identity), Freja eID, or other Nordic eIDs through a broker.',
    ),
    fields: [
      { key: 'bankidEnabled', label: msg('Swedish BankID'), type: 'bool' },
      { key: 'bankidTest', label: msg('Use the BankID test environment'), type: 'bool', when: 'bankidEnabled' },
      {
        key: 'bankidCertificate',
        label: msg('Relying-Party Certificate'),
        type: 'file',
        hint: msg(
          "The .p12 from your bank, or a PEM with the key. In the test environment, leave it empty to use BankID's public test certificate.",
        ),
        passwordKey: 'bankidCertificatePassword',
        when: 'bankidEnabled',
      },
      {
        key: 'bankidCaCertificate',
        label: msg('BankID Server CA (optional)'),
        type: 'pem',
        hint: msg("BankID's own root is built in. Only replace it if BankID tells you to."),
        when: 'bankidEnabled',
      },
      { key: 'frejaEnabled', label: msg('Freja eID'), type: 'bool' },
      { key: 'frejaTest', label: msg('Use the Freja test environment'), type: 'bool', when: 'frejaEnabled' },
      {
        key: 'frejaCertificate',
        label: msg('Client Certificate'),
        type: 'file',
        hint: msg('The .pfx from Freja, or a PEM with the key.'),
        passwordKey: 'frejaCertificatePassword',
        when: 'frejaEnabled',
      },
      {
        key: 'frejaSigningCertificate',
        label: msg('Freja Result-Signing Certificate'),
        type: 'pem',
        hint: msg('From Freja for your environment. Results without a valid signature are refused.'),
        when: 'frejaEnabled',
      },
      {
        key: 'frejaCaCertificate',
        label: msg('Freja Server CA (optional)'),
        type: 'pem',
        hint: msg('Leave empty to trust the public CAs.'),
        when: 'frejaEnabled',
      },
      { key: 'brokerEnabled', label: msg('eID broker (OpenID Connect)'), type: 'bool' },
      { key: 'brokerIssuer', label: msg('Issuer URL'), placeholder: 'https://example.idura.broker', when: 'brokerEnabled' },
      { key: 'brokerClientId', label: msg('Client ID'), when: 'brokerEnabled' },
      { key: 'brokerClientSecret', label: msg('Client Secret'), type: 'secret', when: 'brokerEnabled' },
      {
        key: 'brokerAcrValues',
        label: msg('eID Choice (acr_values)'),
        placeholder: 'urn:grn:authn:no:bankid',
        hint: msg('Leave empty to let the signer choose at the broker. Register /api/eid/callback as the redirect URI.'),
        when: 'brokerEnabled',
      },
      {
        key: 'ticEnabled',
        label: msg('BankID Through TIC Identity'),
        type: 'bool',
        hint: msg('For Swedish BankID without an agreement with a bank. Used when Swedish BankID above is off.'),
      },
      {
        key: 'ticTenant',
        label: msg('Tenant'),
        placeholder: 'your-tenant',
        hint: msg('As in https://id.tic.io/your-tenant.'),
        when: 'ticEnabled',
      },
      { key: 'ticClientId', label: msg('Client ID'), when: 'ticEnabled' },
      {
        key: 'ticClientSecret',
        label: msg('Client Secret'),
        type: 'secret',
        hint: msg('Turn on OIDC for the tenant in the TIC portal and register /api/eid/callback as the redirect URI.'),
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
    toast.error(t('That is not a PEM file. Convert it with openssl first.'));
    return null;
  }
  return text;
}

function EmailExtras() {
  const status = useApi('setup.status', undefined, { staleTime: 0 });
  const test = useApiMutation('settings.send_test_email', {
    invalidates: ['setup.status'],
    onSuccess: () => toast.success(t('Test email sent')),
  });
  return (
    <div className="flex items-center gap-3">
      <Button variant="outline" onClick={() => test.mutate({})} disabled={test.isPending}>
        {test.isPending && <Spinner />} {t('Send Test Email')}
      </Button>
      <span className={status.data?.emailConfigured ? 'text-success' : 'text-warning'}>
        {status.data?.emailConfigured ? t('Working') : t('Not verified yet')}
      </span>
    </div>
  );
}

function SealExtras() {
  const seal = useApi('seal.get');
  const regenerate = useApiMutation('seal.regenerate', { invalidates: ['seal.'] });
  return (
    <SettingsCard
      title={t('Seal Certificate')}
      description={t('The key that seals completed documents. It is generated here and stored encrypted.')}
    >
      {seal.data ? (
        <div className="grid gap-1">
          <div className="font-medium">{seal.data.subject}</div>
          <div className="text-muted-foreground">
            {t('Created {created} · valid until {until}', {
              created: formatDateTime(seal.data.createdAt),
              until: formatDate(seal.data.notAfter),
            })}
          </div>
        </div>
      ) : (
        <p className="text-muted-foreground">{t('Created automatically when the first document completes.')}</p>
      )}
      <div className="mt-4">
        <ConfirmButton
          label={t('Replace Seal Key')}
          title={t('Replace the seal key?')}
          description={t('New documents are sealed with a new key. Documents sealed before stay valid and verifiable.')}
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
    <SettingsCard title={t('Next Run')} description={t('Retention runs every ten minutes. Each deletion is recorded in the audit trail.')}>
      <div className="grid gap-1">
        <div>
          {due === 0
            ? t('Nothing is due for deletion with the saved rules.')
            : t('Documents due: {due}. Signed: {completed}. Declined, canceled or expired: {ended}. Drafts: {drafts}.', {
                due: due.toLocaleString(),
                completed,
                ended,
                drafts,
              })}
        </div>
        <div className="text-muted-foreground">
          {held === 0
            ? t('No documents are on legal hold.')
            : oldestHeldSince
              ? t('Documents on legal hold: {count}, the oldest since {date}.', { count: held, date: formatDate(oldestHeldSince) })
              : t('Documents on legal hold: {count}.', { count: held })}
        </div>
      </div>
    </SettingsCard>
  );
}

function AiExtras() {
  const usage = useApi('ai.usage');
  const test = useApiMutation('ai.test', {
    onSuccess: (out) => toast.success(t('The model answered in {ms} ms', { ms: out.milliseconds })),
  });
  return (
    <div className="flex flex-wrap items-center gap-3">
      <Button variant="outline" onClick={() => test.mutate({})} disabled={test.isPending}>
        {test.isPending && <Spinner />} {t('Test Connection')}
      </Button>
      {usage.data && (
        <span className="text-muted-foreground">
          {usage.data.monthlyBudget
            ? t('{used} of {budget} tokens this month', {
                used: usage.data.tokensThisMonth.toLocaleString(),
                budget: usage.data.monthlyBudget.toLocaleString(),
              })
            : t('{used} tokens this month', { used: usage.data.tokensThisMonth.toLocaleString() })}
        </span>
      )}
    </div>
  );
}

const MODEL_PLACEHOLDERS: Record<string, string> = {
  anthropic: 'claude-sonnet-5-5',
  openai: 'gpt-5',
};

/** Asks the provider for its models with the form's current values; the server keeps the saved key. */
function AiModelField({
  id,
  value,
  onChange,
  provider,
  baseUrl,
  apiKey,
  locked,
}: {
  id: string;
  value: string;
  onChange: (value: string) => void;
  provider: string;
  baseUrl: string;
  apiKey: string | undefined;
  locked: boolean;
}) {
  const models = useApiMutation('ai.models', { silent: true });
  // biome-ignore lint/correctness/useExhaustiveDependencies: list again only when the connection details change
  useEffect(() => {
    if (!provider) return;
    const timer = setTimeout(() => models.mutate({ provider, baseUrl, apiKey }), 600);
    return () => clearTimeout(timer);
  }, [provider, baseUrl, apiKey]);

  if (models.data && models.data.length > 0) {
    const options = models.data.map((m) => ({ value: m.id, label: m.name, detail: m.name !== m.id ? m.id : undefined }));
    if (value && !options.some((o) => o.value === value)) {
      options.unshift({ value, label: value, detail: t('Not offered by this provider') });
    }
    const count = models.data.length;
    return (
      <Field
        label={t('Model')}
        htmlFor={id}
        hint={
          count === 1
            ? t('Connected. 1 model is available with this key.')
            : t('Connected. {count} models are available with this key.', { count })
        }
      >
        <SearchSelect
          id={id}
          value={value}
          onChange={onChange}
          options={options}
          placeholder={t('Choose a model…')}
          searchPlaceholder={t('Search models…')}
          disabled={locked}
        />
      </Field>
    );
  }
  const hint = locked
    ? t('Set by an environment variable.')
    : models.isPending || models.isIdle
      ? t('Checking the connection…')
      : models.error
        ? t('Could not list models: {error}. Enter the model name instead.', { error: errorMessage(models.error).replace(/\.$/, '') })
        : t('The provider listed no models. Enter the model name.');
  return (
    <Field label={t('Model')} htmlFor={id} hint={hint}>
      <Input
        id={id}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={MODEL_PLACEHOLDERS[provider]}
        disabled={locked}
      />
    </Field>
  );
}

export function AdminSettingsPage() {
  const { group = '' } = useParams();
  if (group === 'updates') return <Navigate to="/settings/system" replace />;
  if (!GROUPS[group as Group]) return <Navigate to="/settings" replace />;
  return <SettingsGroup group={group as Group} />;
}

export function SettingsGroup({ group }: { group: Group }) {
  const spec = GROUPS[group];
  const current = useApi('settings.get', { group });
  const [draft, setDraft] = useState<Record<string, unknown>>({});
  const [certFiles, setCertFiles] = useState<Record<string, File>>({});
  const save = useApiMutation('settings.update', {
    invalidates: ['settings.', 'ai.', 'setup.'],
    onSuccess: () => {
      setDraft({});
      setCertFiles({});
      toast.success(t('Saved'));
    },
  });
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new group starts with no unsaved changes
  useEffect(() => {
    setDraft({});
    setCertFiles({});
  }, [group]);

  if (current.isPending) return <Spinner />;
  const saved = (current.data?.values ?? {}) as Record<string, unknown>;
  const pinned = current.data?.pinned ?? [];
  const value = (key: string) => (key in draft ? draft[key] : saved[key]);
  const set = (key: string, v: unknown) => setDraft({ ...draft, [key]: v });

  const submit = (e: FormEvent) => {
    e.preventDefault();
    save.mutate({ group, values: draft });
  };

  return (
    <div className="grid gap-6">
      <SettingsCard title={t(spec.title)} description={t(spec.description)} help={spec.help}>
        <form onSubmit={submit} className="grid max-w-xl gap-5">
          {spec.fields
            .filter((f) => !f.when || (f.is === undefined ? !!value(f.when) : value(f.when) === f.is))
            .map((f) => {
              const locked = pinned.includes(f.key);
              const hint = locked ? t('Set by an environment variable.') : f.hint && t(f.hint);
              const label = t(f.label);
              const id = `setting-${f.key}`;
              if (f.type === 'bool') {
                return (
                  <label key={f.key} className="flex items-center justify-between gap-4">
                    <span>{label}</span>
                    <Switch checked={!!value(f.key)} onCheckedChange={(c) => set(f.key, c)} disabled={locked} />
                  </label>
                );
              }
              if (f.type === 'select') {
                return (
                  <Field key={f.key} label={label} hint={hint}>
                    <Select
                      value={String(value(f.key) ?? '') || 'off'}
                      onValueChange={(v) => set(f.key, v === 'off' ? '' : v)}
                      disabled={locked}
                    >
                      <SelectTrigger className="w-full">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {(typeof f.options === 'function' ? f.options() : f.options!).map(([v, name]) => (
                          <SelectItem key={v || 'off'} value={v || 'off'}>
                            {t(name)}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </Field>
                );
              }
              if (f.type === 'aiModel') {
                return (
                  <AiModelField
                    key={f.key}
                    id={id}
                    value={String(value(f.key) ?? '')}
                    onChange={(v) => set(f.key, v)}
                    provider={String(value('provider') ?? '')}
                    baseUrl={String(value('baseUrl') ?? '')}
                    apiKey={typeof draft.apiKey === 'string' ? draft.apiKey : undefined}
                    locked={locked}
                  />
                );
              }
              if (f.type === 'country' || f.type === 'timeZone') {
                const country = f.type === 'country';
                return (
                  <Field key={f.key} label={f.label} htmlFor={id} hint={hint}>
                    <SearchSelect
                      id={id}
                      value={String(value(f.key) ?? '')}
                      onChange={(v) => set(f.key, v)}
                      options={country ? countryOptions() : timeZoneOptions()}
                      searchPlaceholder={country ? t('Search countries…') : t('Search by city or offset…')}
                      disabled={locked}
                    />
                  </Field>
                );
              }
              if (f.type === 'secret' || f.type === 'file') {
                const isSet = saved[f.key] === true;
                return (
                  <Field key={f.key} label={label} htmlFor={id} hint={hint ?? (isSet ? t('Saved. Leave empty to keep it.') : undefined)}>
                    <div className="flex gap-2">
                      {f.type === 'file' ? (
                        <FilePicker
                          id={id}
                          className="flex-1"
                          accept=".pem,.crt,.cer,.key,.p12,.pfx"
                          disabled={locked}
                          hint={t('.pem, .crt, .cer, .key, .p12 or .pfx')}
                          files={certFiles[f.key] ? [certFiles[f.key]!] : []}
                          onChange={async ([file]) => {
                            setCertFiles((c) => {
                              const { [f.key]: _, ...rest } = c;
                              return file ? { ...rest, [f.key]: file } : rest;
                            });
                            set(f.key, file ? await readCertificate(file) : undefined);
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
                          {t('Clear')}
                        </Button>
                      )}
                    </div>
                    {f.passwordKey && isPkcs12(draft[f.key]) && (
                      <Input
                        className="mt-2"
                        type="password"
                        autoComplete="off"
                        aria-label={t('Certificate file password')}
                        placeholder={t("The file's password")}
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
                  <Field key={f.key} label={label} htmlFor={id} hint={current ? `${t('Saved.')} ${hint ?? ''}` : hint}>
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
                          {t('Clear')}
                        </Button>
                      )}
                    </div>
                  </Field>
                );
              }
              return (
                <Field key={f.key} label={label} htmlFor={id} hint={hint}>
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
              {t('Save')}
            </Button>
            {Object.keys(draft).length > 0 && (
              <Button
                type="button"
                variant="ghost"
                onClick={() => {
                  setDraft({});
                  setCertFiles({});
                }}
              >
                {t('Discard')}
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
