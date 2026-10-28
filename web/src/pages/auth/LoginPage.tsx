import { useQueryClient } from '@tanstack/react-query';
import { type FormEvent, useState } from 'react';
import { Link, Navigate, useNavigate, useSearchParams } from 'react-router';
import { Field, Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { useApiMutation } from '@/lib/api';
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

  const submitCode = (e: FormEvent) => {
    e.preventDefault();
    setError(undefined);
    verify.mutate({ challenge: challenge!, code: code.trim() }, { onError: (e) => setError(e.message) });
  };

  if (challenge) {
    return (
      <AuthLayout title="Two-Factor Authentication" description="Enter the 6-digit code from your authenticator app, or a recovery code.">
        <form onSubmit={submitCode} className="grid gap-4">
          <FormError message={error} />
          <Field label="Code" htmlFor="code">
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
            {verify.isPending && <Spinner className="text-primary-foreground" />} Verify
          </Button>
          <Button type="button" variant="ghost" onClick={() => setChallenge(null)}>
            Back
          </Button>
        </form>
      </AuthLayout>
    );
  }

  return (
    <AuthLayout title="Sign In">
      <form onSubmit={submitPassword} className="grid gap-4">
        <FormError message={error} />
        <Field label="Email" htmlFor="email">
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
        <Field label="Password" htmlFor="password">
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
          {login.isPending && <Spinner className="text-primary-foreground" />} Sign In
        </Button>
        <Link to="/forgot-password" className="text-center text-muted-foreground hover:text-foreground">
          Forgot Your Password?
        </Link>
      </form>
    </AuthLayout>
  );
}
