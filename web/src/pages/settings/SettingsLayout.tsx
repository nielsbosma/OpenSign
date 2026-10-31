import type { ReactNode } from 'react';
import { NavLink, Outlet } from 'react-router';
import { HelpLink, PageHeader } from '@/components/common';
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import type { Permission } from '@/lib/api';
import { msg, t } from '@/lib/i18n';
import { useSession } from '@/lib/session';
import { cn } from '@/lib/utils';

const SECTIONS: { title: string; items: { path: string; label: string; permission?: Permission }[] }[] = [
  { title: msg('You'), items: [{ path: '/settings/account', label: msg('Account and Security') }] },
  {
    title: msg('Organisation'),
    items: [
      { path: '/settings/users', label: msg('Users'), permission: 'users.read' },
      { path: '/settings/admin/org', label: msg('Organisation'), permission: 'settings.manage' },
      { path: '/settings/admin/signing', label: msg('Signing and Seal'), permission: 'settings.manage' },
      { path: '/settings/admin/security', label: msg('Security'), permission: 'settings.manage' },
      { path: '/settings/admin/sso', label: msg('Single Sign-On'), permission: 'settings.manage' },
      { path: '/settings/admin/retention', label: msg('Retention'), permission: 'settings.manage' },
      { path: '/settings/audit', label: msg('Audit Trail'), permission: 'audit.read' },
      { path: '/settings/system', label: msg('System'), permission: 'system.read' },
    ],
  },
  {
    title: msg('Integrations'),
    items: [
      { path: '/settings/admin/email', label: msg('Email'), permission: 'settings.manage' },
      { path: '/settings/admin/sms', label: msg('SMS'), permission: 'settings.manage' },
      { path: '/settings/admin/eid', label: msg('BankID and eID'), permission: 'settings.manage' },
      { path: '/settings/admin/ai', label: msg('AI'), permission: 'settings.manage' },
      { path: '/settings/api-keys', label: msg('API Keys'), permission: 'api_keys.manage' },
      { path: '/settings/webhooks', label: msg('Webhooks'), permission: 'webhooks.manage' },
      { path: '/settings/automations', label: msg('Automations'), permission: 'automations.manage' },
    ],
  },
];

export function SettingsLayout() {
  const { can, user } = useSession();
  return (
    <>
      <PageHeader title={t('Settings')} />
      <div className="grid gap-8 md:grid-cols-[200px_minmax(0,1fr)]">
        <nav className="grid content-start gap-5" aria-label={t('Settings')}>
          {SECTIONS.map((section) => {
            const items = section.items.filter(
              (i) => (!i.permission || can(i.permission)) && !(user?.sso && i.path === '/settings/api-keys'),
            );
            if (!items.length) return null;
            return (
              <div key={section.title} className="grid gap-0.5">
                <div className="px-2.5 pb-1 text-xs font-medium text-muted-foreground">{t(section.title)}</div>
                {items.map((item) => (
                  <NavLink
                    key={item.path}
                    to={item.path}
                    className={({ isActive }) =>
                      cn('rounded-md px-2.5 py-1.5 text-[13px] hover:bg-accent', isActive && 'bg-accent font-medium')
                    }
                  >
                    {t(item.label)}
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
  help,
}: {
  title: string;
  description?: ReactNode;
  children: ReactNode;
  className?: string;
  help?: string;
}) {
  return (
    <Card className={cn('gap-5 shadow-none', className)}>
      <CardHeader className="gap-1.5">
        <CardTitle>{title}</CardTitle>
        {description && <CardDescription className="max-w-prose leading-relaxed">{description}</CardDescription>}
        {help && (
          <CardAction>
            <HelpLink page={help} className="-my-1.5 -me-2" />
          </CardAction>
        )}
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  );
}
