import type { ReactNode } from 'react';
import { Navigate, Route, Routes, useLocation } from 'react-router';
import { Spinner } from '@/components/common';
import { AppShell } from '@/components/layout/AppShell';
import { useApi } from '@/lib/api';
import { useSession } from '@/lib/session';
import { InvitePage } from '@/pages/auth/InvitePage';
import { LoginPage } from '@/pages/auth/LoginPage';
import { ForgotPasswordPage, ResetPasswordPage } from '@/pages/auth/PasswordPages';
import { TwoFactorRequiredPage } from '@/pages/auth/TwoFactorRequiredPage';
import { ConsentPage } from '@/pages/ConsentPage';
import { DocumentPage } from '@/pages/documents/DocumentPage';
import { DocumentsPage } from '@/pages/documents/DocumentsPage';
import { NotFoundPage } from '@/pages/NotFoundPage';
import { SetupPage } from '@/pages/SetupPage';
import { AccountPage } from '@/pages/settings/AccountPage';
import { AdminSettingsPage } from '@/pages/settings/AdminSettingsPage';
import { ApiKeysPage } from '@/pages/settings/ApiKeysPage';
import { AuditPage } from '@/pages/settings/AuditPage';
import { SettingsLayout } from '@/pages/settings/SettingsLayout';
import { UsersPage } from '@/pages/settings/UsersPage';
import { WebhooksPage } from '@/pages/settings/WebhooksPage';
import { SignPage } from '@/pages/sign/SignPage';
import { VerifyPage } from '@/pages/VerifyPage';

function FullPageSpinner() {
  return (
    <div className="flex h-svh items-center justify-center">
      <Spinner className="size-6" />
    </div>
  );
}

/** Signed-in pages: unknown users go to sign in (or to setup on a fresh install), and back here afterwards. */
function RequireSession({ children }: { children: ReactNode }) {
  const { user, loading, can } = useSession();
  const setup = useApi('setup.status', undefined, { staleTime: 60_000 });
  const location = useLocation();
  if (loading || setup.isPending) return <FullPageSpinner />;
  if (!user) {
    if (setup.data?.needsSetup) return <Navigate to="/setup" replace />;
    const next = `${location.pathname}${location.search}`;
    return <Navigate to={`/login${next === '/' ? '' : `?next=${encodeURIComponent(next)}`}`} replace />;
  }
  if (setup.data && !setup.data.emailConfigured && can('settings.manage')) return <Navigate to="/setup" replace />;
  if (user.mustEnableTwoFactor && !location.pathname.startsWith('/settings/account')) return <TwoFactorRequiredPage />;
  return children;
}

export function App() {
  return (
    <Routes>
      <Route path="/setup" element={<SetupPage />} />
      <Route path="/login" element={<LoginPage />} />
      <Route path="/forgot-password" element={<ForgotPasswordPage />} />
      <Route path="/reset-password" element={<ResetPasswordPage />} />
      <Route path="/invite" element={<InvitePage />} />
      <Route path="/sign" element={<SignPage />} />
      <Route path="/verify" element={<VerifyPage />} />
      <Route
        path="/oauth/consent"
        element={
          <RequireSession>
            <ConsentPage />
          </RequireSession>
        }
      />
      <Route
        element={
          <RequireSession>
            <AppShell />
          </RequireSession>
        }
      >
        <Route index element={<Navigate to="/documents" replace />} />
        <Route path="/documents" element={<DocumentsPage templates={false} />} />
        <Route path="/templates" element={<DocumentsPage templates />} />
        <Route path="/documents/:id" element={<DocumentPage />} />
        <Route path="/settings" element={<SettingsLayout />}>
          <Route index element={<Navigate to="/settings/account" replace />} />
          <Route path="account" element={<AccountPage />} />
          <Route path="users" element={<UsersPage />} />
          <Route path="api-keys" element={<ApiKeysPage />} />
          <Route path="webhooks" element={<WebhooksPage />} />
          <Route path="audit" element={<AuditPage />} />
          <Route path="admin/:group" element={<AdminSettingsPage />} />
        </Route>
        <Route path="*" element={<NotFoundPage />} />
      </Route>
    </Routes>
  );
}
