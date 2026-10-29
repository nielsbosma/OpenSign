import { ArrowUpCircle, CheckCircle2, ExternalLink, RefreshCw } from 'lucide-react';
import { toast } from 'sonner';
import { ConfirmButton, Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { useApi, useApiMutation } from '@/lib/api';
import { formatDate, formatDateTime } from '@/lib/format';
import { t, tx } from '@/lib/i18n';
import { useSession } from '@/lib/session';
import { SettingsGroup } from './AdminSettingsPage';
import { SettingsCard } from './SettingsLayout';

function Command({ children }: { children: string }) {
  return <pre className="overflow-x-auto rounded-md border bg-muted/50 px-3 py-2 font-mono text-xs">{children}</pre>;
}

function VersionCard() {
  const { can } = useSession();
  const version = useApi('system.version', { refresh: false });
  const check = useApiMutation('system.version', { invalidates: ['system.version'] });
  const update = useApiMutation('system.update', {
    onSuccess: () => toast.success(t('Update started. OpenSign restarts on the new version in a minute or two.')),
  });
  if (!version.data) return <Spinner />;
  const v = check.data ?? version.data;
  const tag = v.latest?.version ?? 'latest';
  const released = v.latest?.publishedAt ? formatDate(v.latest.publishedAt) : null;

  return (
    <SettingsCard
      title={t('Version')}
      description={tx('OpenSign checks {link} for new releases at most every six hours. Only the version is sent.', {
        link: (
          <a
            href="https://github.com/nielsbosma/OpenSign/releases"
            target="_blank"
            rel="noreferrer"
            className="text-primary hover:underline"
          >
            GitHub
          </a>
        ),
      })}
    >
      <div className="grid gap-4">
        <div className="flex flex-wrap items-center gap-3">
          {v.updateAvailable ? <ArrowUpCircle className="size-5 text-primary" /> : <CheckCircle2 className="size-5 text-success" />}
          <div className="grid">
            <span className="font-medium">
              {v.development
                ? t('Running {version} (development build)', { version: v.current })
                : t('Running {version}', { version: v.current })}
            </span>
            <span className="text-muted-foreground text-sm">
              {v.checkProblem ??
                (v.latest
                  ? v.updateAvailable
                    ? released
                      ? t('Version {version} is available, released {date}.', { version: v.latest.version, date: released })
                      : t('Version {version} is available.', { version: v.latest.version })
                    : t('This is the latest release.')
                  : t('No release has been published yet.'))}
            </span>
          </div>
          <div className="ms-auto flex gap-2">
            <Button variant="outline" onClick={() => check.mutate({ refresh: true })} disabled={check.isPending}>
              {check.isPending ? <Spinner /> : <RefreshCw />} {t('Check Now')}
            </Button>
            {v.updateAvailable && v.canUpdate && can('settings.manage') && (
              <ConfirmButton
                label={t('Update Now')}
                title={t('Update to {version}?', { version: tag })}
                description={t(
                  'OpenSign redeploys through the deploy hook. Database migrations run when the new version starts, and signing links keep working. Expect a minute or two of downtime.',
                )}
                onConfirm={() => update.mutate({})}
                disabled={update.isPending}
              />
            )}
            {v.updateAvailable && !v.canUpdate && v.updateLink && can('settings.manage') && (
              <Button asChild>
                <a href={v.updateLink} target="_blank" rel="noreferrer">
                  {t('Update Now')} <ExternalLink />
                </a>
              </Button>
            )}
          </div>
        </div>
        {v.updateAvailable && !v.canUpdate && v.updateLink && can('settings.manage') && (
          <p className="text-muted-foreground text-sm">
            {t('Update Now opens the Sliplane Deployer. Sign in to Sliplane and confirm; migrations run when the new version starts.')}
          </p>
        )}
        {v.latest?.url && (
          <a href={v.latest.url} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 text-primary text-sm">
            {t('Release notes for {version}', { version: v.latest.version })} <ExternalLink className="size-3.5" />
          </a>
        )}
        {v.updateAvailable && !v.canUpdate && !v.updateLink && (
          <div className="grid gap-2 text-sm">
            <p className="text-muted-foreground">
              {t('To update from here, add a deploy hook under Updates below. Or redeploy yourself; migrations run on start:')}
            </p>
            <Command>{`OPENSIGN_TAG=${tag} ./deploy/sliplane.sh`}</Command>
            <Command>{'docker compose pull && docker compose up -d'}</Command>
          </div>
        )}
      </div>
    </SettingsCard>
  );
}

export function SystemPage() {
  const { can } = useSession();
  const status = useApi('system.status', {});
  const s = status.data;
  return (
    <div className="grid gap-6">
      <VersionCard />
      {can('settings.manage') && <SettingsGroup group="updates" />}
      {s ? (
        <>
          <SettingsCard
            title={t('Database')}
            description={t('Migrations run automatically when OpenSign starts, so a new version brings its database changes with it.')}
          >
            <div className="grid gap-3 text-sm">
              <div>
                PostgreSQL {s.database} ·{' '}
                {s.pendingMigrations === 0
                  ? t('{count} migrations applied, none pending.', { count: s.migrations.length })
                  : t('{count} migrations pending; they run at the next start.', { count: s.pendingMigrations })}
              </div>
              <ul className="grid gap-1 text-muted-foreground">
                {s.migrations.slice(0, 5).map((m) => (
                  <li key={m.version} className="flex gap-3">
                    <span className="w-10 font-mono tabular-nums">{String(m.version).padStart(4, '0')}</span>
                    <span className="flex-1">{m.description}</span>
                    <span className={m.success ? '' : 'text-destructive'}>{formatDateTime(m.installedOn)}</span>
                  </li>
                ))}
              </ul>
            </div>
          </SettingsCard>
          <SettingsCard
            title={t('Background Jobs')}
            description={t('Emails, reminders, sealing and webhooks waiting or done in the last day.')}
          >
            {s.jobs.length === 0 ? (
              <p className="text-muted-foreground text-sm">{t('Nothing queued.')}</p>
            ) : (
              <ul className="grid gap-1.5 text-sm">
                {s.jobs.map((j) => (
                  <li key={j.kind} className="grid gap-0.5">
                    <div className="flex gap-3">
                      <span className="flex-1 font-mono">{j.kind}</span>
                      <span className="text-muted-foreground">{t('{count} waiting', { count: j.waiting })}</span>
                      {j.failing > 0 && <span className="text-destructive">{t('{count} retrying', { count: j.failing })}</span>}
                    </div>
                    {j.lastError && <span className="truncate text-destructive text-xs">{j.lastError}</span>}
                  </li>
                ))}
              </ul>
            )}
          </SettingsCard>
        </>
      ) : (
        <Spinner />
      )}
    </div>
  );
}
