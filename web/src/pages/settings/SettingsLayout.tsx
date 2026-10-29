import type { ReactNode } from 'react';
import { NavLink, Outlet } from 'react-router';
import { PageHeader } from '@/components/common';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import type { Permission } from '@/lib/api';
import { useSession } from '@/lib/session';
import { cn } from '@/lib/utils';

const SECTIONS: { title: string; items: { path: string; label: string; permission?: Permission }[] }[] = [
  { title: 'You', items: [{ path: '/settings/account', label: 'Account and Security' }] },
  {
    title: 'Organisation',
    items: [
      { path: '/settings/users', label: 'Users', permission: 'users.read' },
      { path: '/settings/admin/org', label: 'Organisation', permission: 'settings.manage' },
      { path: '/settings/admin/signing', label: 'Signing and Seal', permission: 'settings.manage' },
      { path: '/settings/admin/security', label: 'Security', permission: 'settings.manage' },
      { path: '/settings/admin/retention', label: 'Retention', permission: 'settings.manage' },
      { path: '/settings/audit', label: 'Audit Trail', permission: 'audit.read' },
      { path: '/settings/system', label: 'System', permission: 'system.read' },
    ],
  },
  {
    title: 'Integrations',
    items: [
      { path: '/settings/admin/email', label: 'Email (Resend)', permission: 'settings.manage' },
      { path: '/settings/admin/sms', label: 'SMS', permission: 'settings.manage' },
      { path: '/settings/admin/eid', label: 'BankID and eID', permission: 'settings.manage' },
      { path: '/settings/admin/ai', label: 'AI', permission: 'settings.manage' },
      { path: '/settings/api-keys', label: 'API Keys', permission: 'api_keys.manage' },
      { path: '/settings/webhooks', label: 'Webhooks', permission: 'webhooks.manage' },
    ],
  },
];

export function SettingsLayout() {
  const { can } = useSession();
  return (
    <>
      <PageHeader title="Settings" />
      <div className="grid gap-8 md:grid-cols-[200px_minmax(0,1fr)]">
        <nav className="grid content-start gap-5" aria-label="Settings">
          {SECTIONS.map((section) => {
            const items = section.items.filter((i) => !i.permission || can(i.permission));
            if (!items.length) return null;
            return (
              <div key={section.title} className="grid gap-0.5">
                <div className="px-2.5 pb-1 text-xs font-medium text-muted-foreground">{section.title}</div>
                {items.map((item) => (
                  <NavLink
                    key={item.path}
                    to={item.path}
                    className={({ isActive }) =>
                      cn('rounded-md px-2.5 py-1.5 text-[13px] hover:bg-accent', isActive && 'bg-accent font-medium')
                    }
                  >
                    {item.label}
                  </NavLink>
                ))}
              </div>
            );
          })}
        </nav>
        <div className="min-w-0">
          <Outlet />
        </div>
      </div>
    </>
  );
}

export function SettingsCard({
  title,
  description,
  children,
  className,
}: {
  title: string;
  description?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <Card className={cn('gap-5 shadow-none', className)}>
      <CardHeader className="gap-1.5">
        <CardTitle>{title}</CardTitle>
        {description && <CardDescription className="max-w-prose leading-relaxed">{description}</CardDescription>}
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  );
}
