import { type FormEvent, useState } from 'react';
import { Link, useNavigate } from 'react-router';
import { Field, Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { useApiMutation } from '@/lib/api';
import { AuthLayout, FormError, PASSWORD_HINT, useHashToken } from './AuthLayout';

export function ForgotPasswordPage() {
  const [email, setEmail] = useState('');
  const request = useApiMutation('auth.request_password_reset');
  if (request.isSuccess) {
    return (
      <AuthLayout title="Check Your Email" description="If the address belongs to an account, a reset link is on its way.">
        <Button asChild variant="outline" className="w-full">
          <Link to="/login">Back to Sign In</Link>
        </Button>
      </AuthLayout>
    );
  }
  const submit = (e: FormEvent) => {
    e.preventDefault();
    request.mutate({ email: email.trim() });
  };
  return (
    <AuthLayout title="Reset Your Password" description="We will email you a link to choose a new one.">
      <form onSubmit={submit} className="grid gap-4">
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
        <Button type="submit" disabled={request.isPending}>
          {request.isPending && <Spinner className="text-primary-foreground" />} Send Reset Link
        </Button>
        <Link to="/login" className="text-center text-muted-foreground hover:text-foreground">
          Back to Sign In
        </Link>
      </form>
    </AuthLayout>
  );
}

export function ResetPasswordPage() {
  const token = useHashToken();
  const navigate = useNavigate();
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [error, setError] = useState<string>();
  const reset = useApiMutation('auth.reset_password', { silent: true, onSuccess: () => navigate('/login', { replace: true }) });
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (password !== confirm) return setError('The passwords do not match');
    setError(undefined);
    reset.mutate({ token, password }, { onError: (e) => setError(e.message) });
  };
  return (
    <AuthLayout title="Choose a New Password" description="This also signs you out everywhere else.">
      <form onSubmit={submit} className="grid gap-4">
        <FormError message={!token ? 'This link is incomplete. Open it from the email again.' : error} />
        <Field label="New Password" htmlFor="password" hint={PASSWORD_HINT}>
          <Input
            id="password"
            type="password"
            autoComplete="new-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
          />
        </Field>
        <Field label="Repeat Password" htmlFor="confirm">
          <Input
            id="confirm"
            type="password"
            autoComplete="new-password"
            value={confirm}
            onChange={(e) => setConfirm(e.target.value)}
            required
          />
        </Field>
        <Button type="submit" disabled={reset.isPending || !token}>
          {reset.isPending && <Spinner className="text-primary-foreground" />} Set Password
        </Button>
      </form>
    </AuthLayout>
  );
}
