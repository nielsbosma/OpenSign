import { useQueryClient } from '@tanstack/react-query';
import { Building2, KeyRound } from 'lucide-react';
import { type FormEvent, useState } from 'react';
import { Link, Navigate, useNavigate, useSearchParams } from 'react-router';
import { Field, Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ApiError, call, errorMessage, type Output, useApi, useApiMutation } from '@/lib/api';
import { msg, t } from '@/lib/i18n';
import { getPasskey, passkeyCanceled, passkeysSupported } from '@/lib/passkeys';
import { useSession } from '@/lib/session';
import { AuthLayout, FormError } from './AuthLayout';

export function safeNext(next: string | null) {
  return next?.startsWith('/') && !next.startsWith('//') && !next.startsWith('/\\') ? next : '/';
}

/** Why the server sent someone back from the identity provider. */
const SSO_REFUSALS: Record<string, string> = {
  disabled: msg('Single sign-on is not available right now. Ask your admin.'),
  expired: msg('That sign-in expired or was already used. Start again.'),
  other_browser: msg('Finish signing in in the same browser where you started. Start again here.'),
  denied: msg('The identity provider did not let you in. Ask your admin if you should have access.'),
  failed: msg('The sign-in could not be completed. Try again, or ask your admin.'),
  email_unverified: msg('Your identity provider has not confirmed your email address, so you cannot sign in here.'),
  mfa_required: msg(
    'Sign-in needs a second factor, and your identity provider did not report one. Turn on two-factor there and sign in again.',
  ),
  inactive: msg('This account is deactivated. Ask your admin.'),
  admin_account: msg('This email belongs to an admin account, which cannot use single sign-on. Sign in with its password.'),
  email_conflict: msg('This email is already used by another account. Ask your admin.'),
};

const SSO_NEXT = 'opensign.sso_next';

/** Where someone was going when they left for the identity provider, once. */
export function takeSsoNext() {
  try {
    const next = sessionStorage.getItem(SSO_NEXT);
    sessionStorage.removeItem(SSO_NEXT);
    return next ? safeNext(next) : null;
  } catch {
    return null;
  }
}

export function LoginPage() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const { user } = useSession();
  const [params] = useSearchParams();
  const next = safeNext(params.get('next'));
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [code, setCode] = useState('');
  const [challenge, setChallenge] = useState<string | null>(null);
  const [ceremony, setCeremony] = useState<Extract<Output<'auth.login'>, { status: 'passkey_required' }>['ceremony'] | null>(null);
  const refusal = params.get('sso');
  const [error, setError] = useState<string | undefined>(() =>
    refusal ? t((Object.hasOwn(SSO_REFUSALS, refusal) && SSO_REFUSALS[refusal]) || SSO_REFUSALS.failed!) : undefined,
  );
  const [passkeyBusy, setPasskeyBusy] = useState(false);
  const [ssoBusy, setSsoBusy] = useState(false);
  const sso = useApi('auth.sso_status', undefined, { retry: false });

  const done = async () => {
    await queryClient.invalidateQueries();
    navigate(next, { replace: true });
  };
  const login = useApiMutation('auth.login', {
    silent: true,
    onSuccess: (result) => {
      if (result.status === 'two_factor_required') setChallenge(result.challenge);
      else if (result.status === 'passkey_required') setCeremony(result.ceremony);
      else void done();
    },
  });
  const verify = useApiMutation('auth.verify_two_factor', { silent: true, onSuccess: () => void done() });

  if (user && !login.isPending && !verify.isPending) return <Navigate to={next} replace />;

  const submitPassword = (e: FormEvent) => {
    e.preventDefault();
    setError(undefined);
    login.mutate(
      { email: email.trim(), password },
      {
        onError: (e) =>
          setError(
            e instanceof ApiError && e.code === 'sso_required'
              ? t('Use single sign-on. Only admins sign in with a password here.')
              : e.message,
          ),
      },
    );
  };

  // A click, never a redirect by itself: otherwise signing out would sign straight back in.
  const signInWithSso = async () => {
    setError(undefined);
    setSsoBusy(true);
    try {
      const { redirectUrl } = await call('auth.sso_start', {});
      try {
        if (next !== '/') sessionStorage.setItem(SSO_NEXT, next);
      } catch {}
      window.location.assign(redirectUrl);
    } catch (e) {
      setError(errorMessage(e));
      setSsoBusy(false);
    }
  };

  const signInWithPasskey = async () => {
    setError(undefined);
    setPasskeyBusy(true);
    try {
      const begin = await call('auth.passkey_begin', {});
      const response = await getPasskey(begin.options as Record<string, unknown>);
      await call('auth.passkey_sign_in', { state: begin.state, ...response });
      await done();
    } catch (e) {
      if (!passkeyCanceled(e)) setError(errorMessage(e));
    } finally {
      setPasskeyBusy(false);
    }
  };

  const finishWithPasskey = async () => {
    if (!ceremony) return;
    setError(undefined);
    setPasskeyBusy(true);
    try {
      const response = await getPasskey(ceremony.options as Record<string, unknown>);
      await call('auth.passkey_sign_in', { state: ceremony.state, ...response });
      await done();
    } catch (e) {
      if (!passkeyCanceled(e)) {
        // The request was good for this one attempt.
        setCeremony(null);
        setError(errorMessage(e));
      }
    } finally {
      setPasskeyBusy(false);
    }
  };

  const submitCode = (e: FormEvent) => {
    e.preventDefault();
    setError(undefined);
    verify.mutate({ challenge: challenge!, code: code.trim() }, { onError: (e) => setError(e.message) });
  };

  if (challenge) {
    return (
      <AuthLayout
        title={t('Two-Factor Authentication')}
        description={t('Enter the 6-digit code from your authenticator app, or a recovery code.')}
      >
        <form onSubmit={submitCode} className="grid gap-4">
          <FormError message={error} />
          <Field label={t('Code')} htmlFor="code">
            <Input
              id="code"
              autoComplete="one-time-code"
              value={code}
              onChange={(e) => setCode(e.target.value)}
              className="font-mono"
              autoFocus
            />
          </Field>
          <Button type="submit" disabled={verify.isPending || !code.trim()}>
            {verify.isPending && <Spinner className="text-primary-foreground" />} {t('Verify')}
          </Button>
          <Button type="button" variant="ghost" onClick={() => setChallenge(null)}>
            {t('Back')}
          </Button>
        </form>
      </AuthLayout>
    );
  }

  if (ceremony) {
    return (
      <AuthLayout title={t('Use Your Passkey')} description={t('Your password was right. This account signs in with its passkey too.')}>
        <div className="grid gap-4">
          <FormError message={error} />
          <Button disabled={passkeyBusy} onClick={() => void finishWithPasskey()} autoFocus>
            {passkeyBusy ? <Spinner className="text-primary-foreground" /> : <KeyRound />} {t('Use Passkey')}
          </Button>
          <Button type="button" variant="ghost" onClick={() => setCeremony(null)}>
            {t('Back')}
          </Button>
        </div>
      </AuthLayout>
    );
  }

  return (
    <AuthLayout title={t('Sign In')}>
      <form onSubmit={submitPassword} className="grid gap-4">
        <FormError message={error} />
        {sso.data?.enabled && (
          <Button type="button" disabled={ssoBusy} onClick={() => void signInWithSso()}>
            {ssoBusy ? <Spinner className="text-primary-foreground" /> : <Building2 />}{' '}
            {sso.data.buttonLabel || t('Sign In with Single Sign-On')}
          </Button>
        )}
        <Field label={t('Email')} htmlFor="email">
          <Input
            id="email"
            type="email"
            autoComplete="username"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            required
            autoFocus
          />
        </Field>
        <Field label={t('Password')} htmlFor="password">
          <Input
            id="password"
            type="password"
            autoComplete="current-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
          />
        </Field>
        <Button type="submit" variant={sso.data?.enabled ? 'outline' : 'default'} disabled={login.isPending}>
          {login.isPending && <Spinner className="text-primary-foreground" />} {t('Sign In')}
        </Button>
        {passkeysSupported() && (
          <Button type="button" variant="outline" disabled={passkeyBusy} onClick={() => void signInWithPasskey()}>
            {passkeyBusy ? <Spinner /> : <KeyRound />} {t('Sign In with a Passkey')}
          </Button>
        )}
        <Link to="/forgot-password" className="text-center text-muted-foreground hover:text-foreground">
          {t('Forgot Your Password?')}
        </Link>
      </form>
    </AuthLayout>
  );
}
