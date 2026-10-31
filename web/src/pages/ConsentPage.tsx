import { Plug } from 'lucide-react';
import { useState } from 'react';
import { useSearchParams } from 'react-router';
import { Spinner } from '@/components/common';
import { needsStepUp, StepUpDialog } from '@/components/StepUpDialog';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { call, errorMessage, useApi } from '@/lib/api';
import { t } from '@/lib/i18n';
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
  const [stepUp, setStepUp] = useState(false);

  const decide = async (approve: boolean) => {
    setBusy(true);
    setError(undefined);
    try {
      const out = await call(approve ? 'oauth.approve' : 'oauth.deny', { request });
      window.location.assign(out.redirectUrl);
    } catch (e) {
      if (needsStepUp(e)) setStepUp(true);
      else setError(errorMessage(e));
      setBusy(false);
    }
  };

  if (!request || describe.error) {
    return (
      <AuthLayout title={t('This Request Has Expired')}>
        <FormError message={describe.error?.message ?? t('Start connecting again from the app you were using.')} />
      </AuthLayout>
    );
  }
  if (!describe.data) {
    return (
      <AuthLayout title={t('Connect an App')}>
        <Spinner />
      </AuthLayout>
    );
  }
  const client = describe.data;
  return (
    <AuthLayout title={t('Allow {app}?', { app: client.clientName })}>
      <div className="grid gap-4">
        <div className="flex items-center gap-3 rounded-md border p-3">
          <Plug className="size-5 shrink-0 text-primary" />
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <span className="break-words font-medium">{client.clientName}</span>
              <Badge variant="outline">{t('Unverified App')}</Badge>
            </div>
            <div className="break-all font-medium">{t('Access goes to {domain}', { domain: client.redirectDomain })}</div>
            <div className="break-all text-xs text-muted-foreground">{t('Returns to {host}', { host: client.redirectHost })}</div>
          </div>
        </div>
        <p className="text-muted-foreground">
          {t('Anyone can register an app under any name. Allow it only if you started this and you expect that domain.')}
        </p>
        <p className="text-muted-foreground">
          {t(
            'It can do anything you can do in OpenSign as {name}, including reading and sending documents, until you disconnect it in Settings.',
            { name: user?.name ?? '' },
          )}
        </p>
        <FormError message={error} />
        <Button onClick={() => decide(true)} disabled={busy}>
          {busy && <Spinner className="text-primary-foreground" />} {t('Allow')}
        </Button>
        <Button variant="outline" onClick={() => decide(false)} disabled={busy}>
          {t('Deny')}
        </Button>
      </div>
      <StepUpDialog
        open={stepUp}
        onOpenChange={setStepUp}
        onConfirmed={() => {
          setStepUp(false);
          void decide(true);
        }}
      />
    </AuthLayout>
  );
}
