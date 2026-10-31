import { Link } from 'react-router';
import { Button } from '@/components/ui/button';
import { call } from '@/lib/api';
import { t } from '@/lib/i18n';
import { useSession } from '@/lib/session';
import { AuthLayout } from './AuthLayout';

export function TwoFactorRequiredPage() {
  const { user } = useSession();
  if (user?.sso) {
    return (
      <AuthLayout
        title={t('Sign In Again')}
        description={t(
          'Your administrator requires two-factor, and your last sign-in did not prove one. Sign out, then sign in again through your identity provider.',
        )}
      >
        <Button className="w-full" onClick={() => void call('auth.logout', {}).finally(() => window.location.assign('/login'))}>
          {t('Sign Out')}
        </Button>
      </AuthLayout>
    );
  }
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
