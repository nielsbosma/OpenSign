import { useQueryClient } from '@tanstack/react-query';
import { Check, CheckCircle2 } from 'lucide-react';
import { type FormEvent, useState } from 'react';
import { Navigate, useNavigate } from 'react-router';
import { Field, Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { call, useApi } from '@/lib/api';
import { useSession } from '@/lib/session';
import { cn } from '@/lib/utils';
import { AuthLayout, FormError, PASSWORD_HINT } from './auth/AuthLayout';

const STEPS = ['Administrator', 'Email'];

function Steps({ step }: { step: 1 | 2 }) {
  return (
    <ol className="mb-6 flex items-center" aria-label="Setup progress">
      {STEPS.map((label, i) => {
        const n = i + 1;
        const state = n < step ? 'done' : n === step ? 'current' : 'upcoming';
        return (
          <li
            key={label}
            className={cn('flex items-center', n < STEPS.length && 'flex-1')}
            aria-current={state === 'current' ? 'step' : undefined}
          >
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
                'ml-2 text-sm whitespace-nowrap',
                state === 'upcoming' ? 'text-muted-foreground' : 'font-medium text-foreground',
              )}
            >
              {label}
            </span>
            {n < STEPS.length && <span className={cn('mx-3 h-px flex-1', n < step ? 'bg-primary' : 'bg-border')} aria-hidden />}
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

function AdminStep() {
  const queryClient = useQueryClient();
  const [values, setValues] = useState({ setupCode: '', orgName: '', name: '', email: '', password: '' });
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
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
      setError(e instanceof Error ? e.message : 'Setup failed');
    } finally {
      setBusy(false);
    }
  };

  return (
    <AuthLayout title="Set Up OpenSign" description="Create the first administrator. Everyone else is invited from Settings." wide>
      <Steps step={1} />
      <form onSubmit={submit} className="grid gap-4">
        <FormError message={error} />
        <Field label="Setup Code" htmlFor="setupCode" hint="Printed in the server log when OpenSign starts without any users.">
          <Input id="setupCode" value={values.setupCode} onChange={set('setupCode')} className="font-mono" required autoFocus />
        </Field>
        <Field label="Organisation" htmlFor="orgName">
          <Input id="orgName" autoComplete="organization" value={values.orgName} onChange={set('orgName')} required />
        </Field>
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Your Name" htmlFor="name">
            <Input id="name" autoComplete="name" value={values.name} onChange={set('name')} required />
          </Field>
          <Field label="Email" htmlFor="email">
            <Input id="email" type="email" autoComplete="username" value={values.email} onChange={set('email')} required />
          </Field>
        </div>
        <Field label="Password" htmlFor="password" hint={PASSWORD_HINT}>
          <Input id="password" type="password" autoComplete="new-password" value={values.password} onChange={set('password')} required />
        </Field>
        <Button type="submit" disabled={busy}>
          {busy && <Spinner className="text-primary-foreground" />} Create Administrator
        </Button>
      </form>
    </AuthLayout>
  );
}

function EmailStep() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const current = useApi('settings.get', { group: 'email' });
  const values = (current.data?.values ?? {}) as { fromAddress?: string; fromName?: string; apiKey?: boolean };
  const [apiKey, setApiKey] = useState('');
  const [fromAddress, setFromAddress] = useState<string>();
  const [fromName, setFromName] = useState<string>();
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(undefined);
    setBusy(true);
    try {
      await call('settings.update', {
        group: 'email',
        values: {
          apiKey: apiKey.trim() || null,
          fromAddress: (fromAddress ?? values.fromAddress ?? '').trim(),
          fromName: fromName ?? values.fromName ?? '',
        },
      });
      await call('settings.send_test_email', {});
      await queryClient.invalidateQueries({ queryKey: ['setup.status'] });
      setDone(true);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'The test email failed');
    } finally {
      setBusy(false);
    }
  };

  if (done) {
    return (
      <AuthLayout title="Email Works" description="A test email is on its way to you. OpenSign is ready." wide>
        <div className="flex flex-col items-center gap-4 py-2">
          <CheckCircle2 className="size-10 text-success" />
          <Button className="w-full" onClick={() => navigate('/documents', { replace: true })}>
            Go to Documents
          </Button>
        </div>
      </AuthLayout>
    );
  }

  return (
    <AuthLayout
      title="Connect Resend"
      description={
        <>
          OpenSign sends signing invitations, codes and signed documents through{' '}
          <ExternalLink href="https://resend.com">Resend</ExternalLink>. It needs a working setup before anyone can sign.
        </>
      }
      wide
    >
      <Steps step={2} />
      <form onSubmit={submit} className="grid gap-4">
        <FormError message={error} />
        <Field
          label="Resend API Key"
          htmlFor="apiKey"
          hint={
            values.apiKey ? (
              'A key is saved. Leave empty to keep it.'
            ) : (
              <>
                Create one in <ExternalLink href="https://resend.com/api-keys">Resend API Keys</ExternalLink> with permission to send email.
              </>
            )
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
        <div className="grid gap-4 sm:grid-cols-2">
          <Field
            label="From Address"
            htmlFor="fromAddress"
            hint={
              <>
                On a domain you <ExternalLink href="https://resend.com/domains">verified in Resend</ExternalLink>.
              </>
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
          <Field label="From Name" htmlFor="fromName">
            <Input
              id="fromName"
              value={fromName ?? values.fromName ?? ''}
              onChange={(e) => setFromName(e.target.value)}
              placeholder="Acme Signing"
            />
          </Field>
        </div>
        <Button type="submit" disabled={busy || current.isPending}>
          {busy && <Spinner className="text-primary-foreground" />} Save and Send Test Email
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
