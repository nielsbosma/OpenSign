import { Link } from 'react-router';
import { Button } from '@/components/ui/button';
import { AuthLayout } from './AuthLayout';

export function TwoFactorRequiredPage() {
  return (
    <AuthLayout title="Set Up Two-Factor Authentication" description="Your organisation requires an authenticator app for every account.">
      <Button asChild className="w-full">
        <Link to="/settings/account">Set It Up Now</Link>
      </Button>
    </AuthLayout>
  );
}
