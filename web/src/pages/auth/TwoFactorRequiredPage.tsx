import { Link } from 'react-router';
import { Button } from '@/components/ui/button';
import { t } from '@/lib/i18n';
import { AuthLayout } from './AuthLayout';

export function TwoFactorRequiredPage() {
  return (
    <AuthLayout
      title={t('Set Up Two-Factor Authentication')}
      description={t('Your organisation requires an authenticator app for every account.')}
    >
      <Button asChild className="w-full">
        <Link to="/settings/account">{t('Set It Up Now')}</Link>
      </Button>
    </AuthLayout>
  );
}
