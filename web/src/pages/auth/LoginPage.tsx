import { useQueryClient } from '@tanstack/react-query';
import { KeyRound } from 'lucide-react';
import { type FormEvent, useState } from 'react';
import { Link, Navigate, useNavigate, useSearchParams } from 'react-router';
import { Field, Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { call, errorMessage, useApiMutation } from '@/lib/api';
import { t } from '@/lib/i18n';
import { getPasskey, passkeyCanceled, passkeysSupported } from '@/lib/passkeys';
import { useSession } from '@/lib/session';
import { AuthLayout, FormError } from './AuthLayout';

export function safeNext(next: string | null) {
  return next?.startsWith('/') && !next.startsWith('//') && !next.startsWith('/\\') ? next : '/';
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
  const [error, setError] = useState<string>();
  const [passkeyBusy, setPasskeyBusy] = useState(false);

  const done = async () => {
    await queryClient.invalidateQueries();
    navigate(next, { replace: true });
  };
  const login = useApiMutation('auth.login', {
    silent: true,
    onSuccess: (result) => (result.status === 'two_factor_required' ? setChallenge(result.challenge) : void done()),
  });
  const verify = useApiMutation('auth.verify_two_factor', { silent: true, onSuccess: () => void done() });

  if (user && !login.isPending && !verify.isPending) return <Navigate to={next} replace />;

  const submitPassword = (e: FormEvent) => {
    e.preventDefault();
    setError(undefined);
    login.mutate({ email: email.trim(), password }, { onError: (e) => setError(e.message) });
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

  return (
    <AuthLayout title={t('Sign In')}>
      <form onSubmit={submitPassword} className="grid gap-4">
        <FormError message={error} />
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
        <Button type="submit" disabled={login.isPending}>
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
