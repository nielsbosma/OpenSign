import { type FormEvent, useState } from 'react';
import { useNavigate } from 'react-router';
import { Field, Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { useApiMutation } from '@/lib/api';
import { AuthLayout, FormError, PASSWORD_HINT, useHashToken } from './AuthLayout';

export function InvitePage() {
  const token = useHashToken();
  const navigate = useNavigate();
  const [name, setName] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string>();
  const accept = useApiMutation('users.accept_invite', { silent: true, onSuccess: () => navigate('/login', { replace: true }) });
  const submit = (e: FormEvent) => {
    e.preventDefault();
    setError(undefined);
    accept.mutate({ token, name: name.trim(), password }, { onError: (e) => setError(e.message) });
  };
  return (
    <AuthLayout title="Join OpenSign" description="Choose your name and a password, then sign in.">
      <form onSubmit={submit} className="grid gap-4">
        <FormError message={!token ? 'This link is incomplete. Open it from the email again.' : error} />
        <Field label="Your Name" htmlFor="name">
          <Input id="name" autoComplete="name" value={name} onChange={(e) => setName(e.target.value)} required autoFocus />
        </Field>
        <Field label="Password" htmlFor="password" hint={PASSWORD_HINT}>
          <Input
            id="password"
            type="password"
            autoComplete="new-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
          />
        </Field>
        <Button type="submit" disabled={accept.isPending || !token}>
          {accept.isPending && <Spinner className="text-primary-foreground" />} Create Account
        </Button>
      </form>
    </AuthLayout>
  );
}
