import { useState } from 'react';
import { Link } from 'react-router';
import { Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { useApiMutation } from '@/lib/api';
import { t } from '@/lib/i18n';
import { AuthLayout, FormError, useHashToken } from './AuthLayout';

export function ConfirmEmailPage() {
  const token = useHashToken();
  const [error, setError] = useState<string>();
  const confirm = useApiMutation('auth.confirm_email_change', { silent: true, invalidates: ['account.me', 'users.'] });
  if (confirm.data) {
    return (
      <AuthLayout title={t('Email Changed')} description={t('You now sign in with {email}.', { email: confirm.data.email })}>
        <Button asChild className="w-full">
          <Link to="/">{t('Continue')}</Link>
        </Button>
      </AuthLayout>
    );
  }
  return (
    <AuthLayout title={t('Confirm Your New Email')} description={t('Confirm to start signing in with this address.')}>
      <div className="grid gap-4">
        <FormError message={!token ? t('This link is incomplete. Open it from the email again.') : error} />
        <Button disabled={confirm.isPending || !token} onClick={() => confirm.mutate({ token }, { onError: (e) => setError(e.message) })}>
          {confirm.isPending && <Spinner className="text-primary-foreground" />} {t('Confirm Email')}
        </Button>
      </div>
    </AuthLayout>
  );
}
