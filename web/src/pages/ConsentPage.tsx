import { Plug } from 'lucide-react';
import { useState } from 'react';
import { useSearchParams } from 'react-router';
import { Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { call, errorMessage, useApi } from '@/lib/api';
import { useSession } from '@/lib/session';
import { AuthLayout, FormError } from './auth/AuthLayout';

/** An MCP client asks to act as the signed-in user. Shows where the browser goes next so a look-alike name cannot hide it. */
export function ConsentPage() {
  const [params] = useSearchParams();
  const request = params.get('request') ?? '';
  const { user } = useSession();
  const describe = useApi('oauth.describe', { request }, { enabled: !!request, retry: false });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  const decide = async (approve: boolean) => {
    setBusy(true);
    setError(undefined);
    try {
      const out = await call(approve ? 'oauth.approve' : 'oauth.deny', { request });
      window.location.assign(out.redirectUrl);
    } catch (e) {
      setError(errorMessage(e));
      setBusy(false);
    }
  };

  if (!request || describe.error) {
    return (
      <AuthLayout title="This Request Has Expired">
        <FormError message={describe.error?.message ?? 'Start connecting again from the app you were using.'} />
      </AuthLayout>
    );
  }
  if (!describe.data) {
    return (
      <AuthLayout title="Connect an App">
        <Spinner />
      </AuthLayout>
    );
  }
  const client = describe.data;
  return (
    <AuthLayout title={`Allow ${client.clientName}?`}>
      <div className="grid gap-4">
        <div className="flex items-center gap-3 rounded-md border p-3">
          <Plug className="size-5 shrink-0 text-primary" />
          <div className="min-w-0">
            <div className="truncate font-medium">{client.clientName}</div>
            <div className="truncate text-xs text-muted-foreground">Returns to {client.redirectHost}</div>
          </div>
        </div>
        <p className="text-muted-foreground">
          It can do anything you can do in OpenSign as {user?.name}, including reading and sending documents, until you disconnect it in
          Settings.
        </p>
        <FormError message={error} />
        <Button onClick={() => decide(true)} disabled={busy}>
          {busy && <Spinner className="text-primary-foreground" />} Allow
        </Button>
        <Button variant="outline" onClick={() => decide(false)} disabled={busy}>
          Deny
        </Button>
      </div>
    </AuthLayout>
  );
}
