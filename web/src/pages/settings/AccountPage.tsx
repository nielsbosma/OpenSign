import { useQueryClient } from '@tanstack/react-query';
import QRCode from 'qrcode';
import { type FormEvent, useEffect, useState } from 'react';
import { toast } from 'sonner';
import { CopyButton, Field, Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { call, errorMessage, type TwoFactorSetup, useApi, useApiMutation } from '@/lib/api';
import { formatDateTime } from '@/lib/format';
import { createPasskey, passkeyCanceled, passkeysSupported } from '@/lib/passkeys';
import { useSession } from '@/lib/session';
import { FormError, PASSWORD_HINT } from '../auth/AuthLayout';
import { SettingsCard } from './SettingsLayout';

function PasswordCard() {
  const [current, setCurrent] = useState('');
  const [next, setNext] = useState('');
  const [error, setError] = useState<string>();
  const change = useApiMutation('account.change_password', {
    silent: true,
    onSuccess: () => {
      setCurrent('');
      setNext('');
      toast.success('Password changed. Your other sessions were signed out.');
    },
  });
  const submit = (e: FormEvent) => {
    e.preventDefault();
    setError(undefined);
    change.mutate({ currentPassword: current, newPassword: next }, { onError: (e) => setError(e.message) });
  };
  return (
    <SettingsCard title="Password">
      <form onSubmit={submit} className="grid max-w-md gap-4">
        <FormError message={error} />
        <Field label="Current Password" htmlFor="current">
          <Input
            id="current"
            type="password"
            autoComplete="current-password"
            value={current}
            onChange={(e) => setCurrent(e.target.value)}
            required
          />
        </Field>
        <Field label="New Password" htmlFor="new" hint={PASSWORD_HINT}>
          <Input id="new" type="password" autoComplete="new-password" value={next} onChange={(e) => setNext(e.target.value)} required />
        </Field>
        <Button type="submit" className="justify-self-start" disabled={change.isPending}>
          Change Password
        </Button>
      </form>
    </SettingsCard>
  );
}

function PasskeysCard() {
  const queryClient = useQueryClient();
  const passkeys = useApi('account.passkeys', {});
  const remove = useApiMutation('account.passkey_remove', { invalidates: ['account.passkeys', 'account.me'] });
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState('');
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  const add = async (e: FormEvent) => {
    e.preventDefault();
    setError(undefined);
    setBusy(true);
    try {
      const begin = await call('account.passkey_begin', { currentPassword: password });
      const response = await createPasskey(begin.options as Record<string, unknown>);
      await call('account.passkey_add', { state: begin.state, name: name.trim() || 'Passkey', ...response });
      await queryClient.invalidateQueries({ queryKey: ['account.passkeys'] });
      await queryClient.invalidateQueries({ queryKey: ['account.me'] });
      setAdding(false);
      setName('');
      setPassword('');
      toast.success('Passkey added. You can now sign in with it.');
    } catch (e) {
      setError(passkeyCanceled(e) ? 'The passkey was not created.' : errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <SettingsCard
      title="Passkeys"
      description="Sign in with your fingerprint, face or device PIN instead of a password and code. Works with iCloud Keychain, Google Password Manager, Windows Hello and security keys."
    >
      <div className="grid gap-3">
        {passkeys.data?.map((p) => (
          <div key={p.id} className="flex items-center justify-between gap-3 rounded-md border p-3">
            <div className="min-w-0">
              <div className="truncate font-medium">{p.name}</div>
              <div className="text-xs text-muted-foreground">
                Added {formatDateTime(p.createdAt)}
                {p.lastUsedAt && ` · last used ${formatDateTime(p.lastUsedAt)}`}
              </div>
            </div>
            <Button variant="ghost" size="sm" onClick={() => remove.mutate({ id: p.id })} disabled={remove.isPending}>
              Remove
            </Button>
          </div>
        ))}
        {!passkeysSupported() ? (
          <p className="text-muted-foreground">This browser does not support passkeys.</p>
        ) : adding ? (
          <form onSubmit={add} className="grid max-w-md gap-4">
            <FormError message={error} />
            <Field label="Name" htmlFor="passkey-name" hint="To tell your passkeys apart.">
              <Input id="passkey-name" value={name} onChange={(e) => setName(e.target.value)} placeholder="MacBook" autoFocus />
            </Field>
            <Field label="Current Password" htmlFor="passkey-password">
              <Input
                id="passkey-password"
                type="password"
                autoComplete="current-password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                required
              />
            </Field>
            <div className="flex gap-2">
              <Button type="submit" disabled={busy}>
                {busy && <Spinner className="text-primary-foreground" />} Create Passkey
              </Button>
              <Button type="button" variant="ghost" onClick={() => setAdding(false)}>
                Cancel
              </Button>
            </div>
          </form>
        ) : (
          <Button variant="outline" className="justify-self-start" onClick={() => setAdding(true)}>
            Add a Passkey
          </Button>
        )}
      </div>
    </SettingsCard>
  );
}

function TwoFactorCard() {
  const { user } = useSession();
  const queryClient = useQueryClient();
  const [setup, setSetup] = useState<TwoFactorSetup | null>(null);
  const [qr, setQr] = useState<string>();
  const [code, setCode] = useState('');
  const [codes, setCodes] = useState<string[] | null>(null);
  const [error, setError] = useState<string>();
  const begin = useApiMutation('account.two_factor_begin', { onSuccess: setSetup });
  const confirm = useApiMutation('account.two_factor_confirm', {
    silent: true,
    onSuccess: (out) => {
      setCodes(out.recoveryCodes);
      setSetup(null);
      void queryClient.invalidateQueries({ queryKey: ['account.me'] });
    },
  });
  useEffect(() => {
    if (setup) void QRCode.toDataURL(setup.otpauthUrl, { margin: 1, width: 180 }).then(setQr);
  }, [setup]);

  if (codes) {
    return (
      <SettingsCard
        title="Recovery Codes"
        description="Each code signs you in once if you lose your phone. Store them somewhere safe; they are not shown again."
      >
        <pre className="mb-4 grid grid-cols-2 gap-1 rounded-md bg-muted p-4 font-mono text-sm">{codes.join('\n')}</pre>
        <div className="flex gap-2">
          <CopyButton value={codes.join('\n')} label="Copy Codes" />
          <Button onClick={() => setCodes(null)}>I Saved Them</Button>
        </div>
      </SettingsCard>
    );
  }
  return (
    <SettingsCard
      title="Two-Factor Authentication"
      description={
        user?.twoFactorEnabled
          ? 'On. Signing in needs a code from your authenticator app.'
          : 'Protect your account with a code from an authenticator app.'
      }
    >
      {setup ? (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            setError(undefined);
            confirm.mutate({ code: code.trim() }, { onError: (e) => setError(e.message) });
          }}
          className="grid max-w-md gap-4"
        >
          <p>Scan the code with your authenticator app, then enter the 6 digits it shows.</p>
          {qr && <img src={qr} alt="Authenticator QR code" className="size-44 rounded-md border bg-white p-1" />}
          <code className="break-all text-xs text-muted-foreground">{setup.secret}</code>
          <FormError message={error} />
          <Input
            value={code}
            onChange={(e) => setCode(e.target.value)}
            inputMode="numeric"
            autoComplete="one-time-code"
            className="font-mono"
            aria-label="Code"
          />
          <Button type="submit" className="justify-self-start" disabled={confirm.isPending}>
            Turn On
          </Button>
        </form>
      ) : user?.twoFactorEnabled ? (
        <p className="text-muted-foreground">Lost your phone? Sign in with a recovery code, or ask an Admin to reset it.</p>
      ) : (
        <Button onClick={() => begin.mutate({})} disabled={begin.isPending}>
          Set Up
        </Button>
      )}
    </SettingsCard>
  );
}

function McpCard() {
  const connections = useApi('account.mcp_connections');
  const revoke = useApiMutation('account.revoke_mcp_connection', { invalidates: ['account.mcp_connections'] });
  const url = `${window.location.origin}/mcp`;
  return (
    <SettingsCard
      title="AI Apps (MCP)"
      description="Connect Claude or another MCP client to work with OpenSign as you. Add this server URL in the app; you approve the connection here."
    >
      <div className="mb-4 flex max-w-lg gap-2">
        <Input readOnly value={url} className="font-mono" onFocus={(e) => e.target.select()} />
        <CopyButton value={url} />
      </div>
      {connections.isPending ? (
        <Spinner />
      ) : !connections.data?.length ? (
        <p className="text-muted-foreground">No apps connected.</p>
      ) : (
        <ul className="grid gap-2">
          {connections.data.map((c) => (
            <li key={c.id} className="flex items-center gap-3 rounded-md border px-3 py-2">
              <div className="min-w-0 flex-1">
                <div className="truncate font-medium">{c.clientName}</div>
                <div className="text-xs text-muted-foreground">
                  Connected {formatDateTime(c.createdAt)}
                  {c.lastUsedAt ? ` · last used ${formatDateTime(c.lastUsedAt)}` : ''}
                </div>
              </div>
              <Button variant="ghost" className="text-destructive" onClick={() => revoke.mutate({ id: c.id })}>
                Disconnect
              </Button>
            </li>
          ))}
        </ul>
      )}
    </SettingsCard>
  );
}

export function AccountPage() {
  const { user } = useSession();
  return (
    <div className="grid gap-6">
      <SettingsCard title={user?.name ?? 'You'} description={`${user?.email} · ${user?.role}`}>
        <p className="text-muted-foreground">Ask an Admin to change your name, email or role.</p>
      </SettingsCard>
      <PasskeysCard />
      <TwoFactorCard />
      <PasswordCard />
      <McpCard />
    </div>
  );
}
