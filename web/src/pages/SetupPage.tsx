import { useQueryClient } from '@tanstack/react-query';
import { Check, ShieldCheck } from 'lucide-react';
import { type FormEvent, useState } from 'react';
import { Navigate, useNavigate } from 'react-router';
import { Field, Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { call, useApi } from '@/lib/api';
import { msg, t, tx } from '@/lib/i18n';
import { useSession } from '@/lib/session';
import { cn } from '@/lib/utils';
import { AuthLayout, FormError, PASSWORD_HINT } from './auth/AuthLayout';

const STEPS = [msg('Administrator'), msg('Email')];

function Steps({ step }: { step: 1 | 2 }) {
  return (
    <ol className="mb-6 flex items-center" aria-label={t('Setup progress')}>
      {STEPS.map((label, i) => {
        const n = i + 1;
        const state = n < step ? 'done' : n === step ? 'current' : 'upcoming';
        return (
          <li key={t(label)} className="flex items-center" aria-current={state === 'current' ? 'step' : undefined}>
            <span
              className={cn(
                'flex size-7 shrink-0 items-center justify-center rounded-full border text-xs font-semibold transition-colors',
                state === 'done' && 'border-primary bg-primary text-primary-foreground',
                state === 'current' && 'border-primary bg-primary/10 text-primary ring-4 ring-primary/10',
                state === 'upcoming' && 'border-border bg-background text-muted-foreground',
              )}
            >
              {state === 'done' ? <Check className="size-3.5" strokeWidth={3} /> : n}
            </span>
            <span
              className={cn(
                'ms-2 text-sm whitespace-nowrap',
                state === 'upcoming' ? 'text-muted-foreground' : 'font-medium text-foreground',
              )}
            >
              {t(label)}
            </span>
            {n < STEPS.length && <span className={cn('mx-3 h-px w-10', n < step ? 'bg-primary' : 'bg-border')} aria-hidden />}
          </li>
        );
      })}
    </ol>
  );
}

function ExternalLink({ href, children }: { href: string; children: React.ReactNode }) {
  return (
    <a href={href} target="_blank" rel="noreferrer" className="font-medium text-primary underline-offset-2 hover:underline">
      {children}
    </a>
  );
}

const PASSWORD_CHARS = 'abcdefghjkmnpqrstuvwxyzABCDEFGHJKMNPQRSTUVWXYZ23456789';

/** 24 characters from a 54-character alphabet, about 138 bits, in groups of six so it can be read back. */
function generatePassword() {
  const out: string[] = [];
  const limit = 256 - (256 % PASSWORD_CHARS.length);
  while (out.length < 24) {
    for (const byte of crypto.getRandomValues(new Uint8Array(32))) {
      if (byte < limit && out.length < 24) out.push(PASSWORD_CHARS[byte % PASSWORD_CHARS.length]!);
    }
  }
  return out.join('').match(/.{6}/g)!.join('-');
}

function AdminStep() {
  const queryClient = useQueryClient();
  const [values, setValues] = useState({ setupCode: '', orgName: '', name: '', email: '', password: '' });
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [showPassword, setShowPassword] = useState(false);
  const set = (key: keyof typeof values) => (e: React.ChangeEvent<HTMLInputElement>) => setValues({ ...values, [key]: e.target.value });

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(undefined);
    setBusy(true);
    try {
      await call('setup.create_first_admin', { ...values, setupCode: values.setupCode.trim(), email: values.email.trim() });
      await call('auth.login', { email: values.email.trim(), password: values.password });
      await queryClient.invalidateQueries();
    } catch (e) {
      setError(e instanceof Error ? e.message : t('Setup failed'));
    } finally {
      setBusy(false);
    }
  };

  return (
    <AuthLayout
      title={t('Set Up OpenSign')}
      description={t('Create the first administrator. Everyone else is invited from Settings.')}
      wide
    >
      <Steps step={1} />
      <form onSubmit={submit} className="grid gap-4">
        <FormError message={error} />
        <Field label={t('Setup Code')} htmlFor="setupCode" hint={t('Printed in the server log when OpenSign starts without any users.')}>
          <Input id="setupCode" value={values.setupCode} onChange={set('setupCode')} className="font-mono" required autoFocus />
        </Field>
        <Field label={t('Organisation')} htmlFor="orgName">
          <Input id="orgName" autoComplete="organization" value={values.orgName} onChange={set('orgName')} required />
        </Field>
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label={t('Your Name')} htmlFor="name">
            <Input id="name" autoComplete="name" value={values.name} onChange={set('name')} required />
          </Field>
          <Field label={t('Email')} htmlFor="email">
            <Input id="email" type="email" autoComplete="username" value={values.email} onChange={set('email')} required />
          </Field>
        </div>
        <Field
          label={t('Password')}
          htmlFor="password"
          hint={
            <span className="flex items-start justify-between gap-3">
              <span>{t(PASSWORD_HINT)}</span>
              <button
                type="button"
                className="shrink-0 font-medium text-primary hover:underline"
                onClick={() => {
                  setValues({ ...values, password: generatePassword() });
                  setShowPassword(true);
                }}
              >
                {t('Generate Password')}
              </button>
            </span>
          }
        >
          <Input
            id="password"
            type={showPassword ? 'text' : 'password'}
            autoComplete="new-password"
            value={values.password}
            onChange={set('password')}
            className={cn(showPassword && 'font-mono')}
            required
          />
        </Field>
        <p className="flex gap-2 rounded-md border bg-muted/40 p-3 text-xs text-muted-foreground">
          <ShieldCheck className="size-4 shrink-0 text-primary" />
          {t('To secure your account, turn on two-factor authentication or add a passkey under Settings once you are signed in.')}
        </p>
        <Button type="submit" disabled={busy}>
          {busy && <Spinner className="text-primary-foreground" />} {t('Create Administrator')}
        </Button>
      </form>
    </AuthLayout>
  );
}

type EmailValues = {
  provider?: string;
  apiKey?: boolean;
  smtpHost?: string;
  smtpPort?: number;
  smtpSecurity?: string;
  smtpUsername?: string;
  smtpPassword?: boolean;
  fromAddress?: string;
  fromName?: string;
};

function EmailStep() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const current = useApi('settings.get', { group: 'email' });
  const values = (current.data?.values ?? {}) as EmailValues;
  const [provider, setProvider] = useState<string>();
  const [apiKey, setApiKey] = useState('');
  const [smtpHost, setSmtpHost] = useState<string>();
  const [smtpPort, setSmtpPort] = useState<string>();
  const [smtpSecurity, setSmtpSecurity] = useState<string>();
  const [smtpUsername, setSmtpUsername] = useState<string>();
  const [smtpPassword, setSmtpPassword] = useState('');
  const [fromAddress, setFromAddress] = useState<string>();
  const [fromName, setFromName] = useState<string>();
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);

  const chosen = provider ?? values.provider ?? 'resend';
  const smtp = chosen === 'smtp';
  const security = smtpSecurity ?? values.smtpSecurity ?? 'starttls';

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(undefined);
    setBusy(true);
    try {
      await call('settings.update', {
        group: 'email',
        values: {
          provider: chosen,
          ...(smtp
            ? {
                smtpHost: (smtpHost ?? values.smtpHost ?? '').trim(),
                smtpPort: Number(smtpPort ?? values.smtpPort ?? 587),
                smtpSecurity: security,
                smtpUsername: (smtpUsername ?? values.smtpUsername ?? '').trim(),
                smtpPassword: smtpPassword || null,
              }
            : { apiKey: apiKey.trim() || null }),
          fromAddress: (fromAddress ?? values.fromAddress ?? '').trim(),
          fromName: fromName ?? values.fromName ?? '',
        },
      });
      await call('settings.send_test_email', {});
      await queryClient.invalidateQueries({ queryKey: ['setup.status'] });
      setDone(true);
    } catch (e) {
      setError(e instanceof Error ? e.message : t('The test email failed'));
    } finally {
      setBusy(false);
    }
  };

  if (done) {
    return (
      <AuthLayout title={t('Email Works')} description={t('A test email is on its way to you. OpenSign is ready.')} wide>
        <Button className="w-full" onClick={() => navigate('/documents', { replace: true })}>
          {t('Go to Documents')}
        </Button>
      </AuthLayout>
    );
  }

  return (
    <AuthLayout
      title={t('Set Up Email')}
      description={t(
        'OpenSign sends signing invitations, codes and signed documents by email. It needs a working setup before anyone can sign.',
      )}
      wide
    >
      <Steps step={2} />
      <form onSubmit={submit} className="grid gap-4">
        <FormError message={error} />
        <Tabs value={chosen} onValueChange={setProvider}>
          <TabsList className="w-full">
            <TabsTrigger value="resend">Resend</TabsTrigger>
            <TabsTrigger value="smtp">{t('SMTP Server')}</TabsTrigger>
          </TabsList>
        </Tabs>
        {smtp ? (
          <>
            <div className="grid gap-4 sm:grid-cols-[1fr_7rem]">
              <Field label={t('SMTP Server')} htmlFor="smtpHost">
                <Input
                  id="smtpHost"
                  value={smtpHost ?? values.smtpHost ?? ''}
                  onChange={(e) => setSmtpHost(e.target.value)}
                  placeholder="smtp.example.com"
                  autoComplete="off"
                  required
                />
              </Field>
              <Field label={t('Port')} htmlFor="smtpPort">
                <Input
                  id="smtpPort"
                  type="number"
                  min={1}
                  max={65535}
                  value={smtpPort ?? String(values.smtpPort ?? 587)}
                  onChange={(e) => setSmtpPort(e.target.value)}
                  required
                />
              </Field>
            </div>
            <Field
              label={t('Security')}
              hint={
                security === 'tls'
                  ? t('TLS from the first byte, usually port 465.')
                  : t('Upgrades to TLS before signing in, usually port 587.')
              }
            >
              <Select
                value={security}
                onValueChange={(v) => {
                  setSmtpSecurity(v);
                  const port = smtpPort ?? String(values.smtpPort ?? 587);
                  if (port === '587' || port === '465') setSmtpPort(v === 'tls' ? '465' : '587');
                }}
              >
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="starttls">STARTTLS</SelectItem>
                  <SelectItem value="tls">TLS</SelectItem>
                </SelectContent>
              </Select>
            </Field>
            <div className="grid gap-4 sm:grid-cols-2">
              <Field label={t('Username')} htmlFor="smtpUsername" hint={t('Leave empty if the server needs no sign-in.')}>
                <Input
                  id="smtpUsername"
                  autoComplete="off"
                  value={smtpUsername ?? values.smtpUsername ?? ''}
                  onChange={(e) => setSmtpUsername(e.target.value)}
                />
              </Field>
              <Field
                label={t('Password')}
                htmlFor="smtpPassword"
                hint={values.smtpPassword ? t('A password is saved. Leave empty to keep it.') : undefined}
              >
                <Input
                  id="smtpPassword"
                  type="password"
                  autoComplete="new-password"
                  value={smtpPassword}
                  onChange={(e) => setSmtpPassword(e.target.value)}
                />
              </Field>
            </div>
          </>
        ) : (
          <Field
            label={t('Resend API Key')}
            htmlFor="apiKey"
            hint={
              values.apiKey
                ? t('A key is saved. Leave empty to keep it.')
                : tx('Create one in {link} with permission to send email.', {
                    link: <ExternalLink href="https://resend.com/api-keys">Resend API Keys</ExternalLink>,
                  })
            }
          >
            <Input
              id="apiKey"
              type="password"
              autoComplete="off"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="re_..."
              className="font-mono"
            />
          </Field>
        )}
        <div className="grid gap-4 sm:grid-cols-2">
          <Field
            label={t('From Address')}
            htmlFor="fromAddress"
            hint={
              smtp
                ? t('An address the server may send from.')
                : tx('On a domain you {link}.', {
                    link: <ExternalLink href="https://resend.com/domains">{t('verified in Resend')}</ExternalLink>,
                  })
            }
          >
            <Input
              id="fromAddress"
              type="email"
              value={fromAddress ?? values.fromAddress ?? ''}
              onChange={(e) => setFromAddress(e.target.value)}
              placeholder="sign@example.com"
              required
            />
          </Field>
          <Field label={t('From Name')} htmlFor="fromName">
            <Input
              id="fromName"
              value={fromName ?? values.fromName ?? ''}
              onChange={(e) => setFromName(e.target.value)}
              placeholder={t('Acme Signing')}
            />
          </Field>
        </div>
        <Button type="submit" disabled={busy || current.isPending}>
          {busy && <Spinner className="text-primary-foreground" />} {t('Save and Send Test Email')}
        </Button>
      </form>
    </AuthLayout>
  );
}

/** First run: the administrator, then email. Email is required, so setup resumes here until a test email has gone out. */
export function SetupPage() {
  const { user, loading, can } = useSession();
  const status = useApi('setup.status', undefined, { staleTime: 0 });
  if (loading || status.isPending) {
    return (
      <div className="flex h-svh items-center justify-center">
        <Spinner className="size-6" />
      </div>
    );
  }
  if (status.data?.needsSetup) return <AdminStep />;
  if (!user) return <Navigate to="/login" replace />;
  if (!can('settings.manage')) return <Navigate to="/documents" replace />;
  return <EmailStep />;
}
