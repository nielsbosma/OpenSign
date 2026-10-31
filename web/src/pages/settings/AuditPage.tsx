import { ShieldAlert, ShieldCheck } from 'lucide-react';
import { useState } from 'react';
import { Loading, Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { type AuditView, call, useApi } from '@/lib/api';
import { formatDateTime } from '@/lib/format';
import { t } from '@/lib/i18n';
import { SettingsCard } from './SettingsLayout';

const PAGE = 100;

export function AuditPage() {
  const [action, setAction] = useState('');
  const first = useApi('audit.list', { action: action.trim() || null, limit: PAGE });
  const [more, setMore] = useState<AuditView[]>([]);
  const [loadingMore, setLoadingMore] = useState(false);
  const chain = useApi('audit.verify', undefined, { enabled: false });
  const entries = [...(first.data ?? []), ...more];
  const report = chain.data;
  const badAnchors = report ? report.broken_anchors.length + report.unsigned_anchors.length + report.invalid_anchors.length : 0;
  const copyProblem = report && ['missing', 'behind', 'unreadable'].includes(report.witness);

  const loadMore = async () => {
    const last = entries.at(-1);
    if (!last) return;
    setLoadingMore(true);
    try {
      setMore([...more, ...(await call('audit.list', { action: action.trim() || null, limit: PAGE, beforeId: last.id }))]);
    } finally {
      setLoadingMore(false);
    }
  };

  return (
    <div className="grid gap-6">
      <SettingsCard
        help="audit-log"
        title={t('Audit Trail')}
        description={t(
          'Every change, sign-in and signature, in a hash chain: each entry includes the hash of the one before, so a changed or deleted entry shows.',
        )}
      >
        <div className="mb-4 flex flex-wrap items-center gap-3">
          <Button variant="outline" onClick={() => chain.refetch()} disabled={chain.isFetching}>
            {chain.isFetching ? <Spinner /> : <ShieldCheck />} {t('Verify the Chain')}
          </Button>
          {chain.data &&
            (chain.data.valid ? (
              <span className="flex items-center gap-1.5 text-success">
                <ShieldCheck className="size-4" /> {t('All {count} entries intact', { count: chain.data.entries })}
              </span>
            ) : (
              <span className="flex items-center gap-1.5 text-destructive">
                <ShieldAlert className="size-4" />{' '}
                {chain.data.broken_at != null
                  ? t('Broken at entry {entry}', { entry: chain.data.broken_at })
                  : badAnchors > 0
                    ? t('{count} time stamp anchors do not match the chain', { count: badAnchors })
                    : t('The audit trail does not match the time stamp copy in the bucket')}
              </span>
            ))}
          {report?.anchor_stale && (
            <span className="text-warning">{t('Recent entries have no time stamp. Check the time stamp authority.')}</span>
          )}
          {copyProblem && <span className="text-warning">{t('The time stamp copy in the bucket is missing or out of date.')}</span>}
          <Input
            value={action}
            onChange={(e) => {
              setAction(e.target.value);
              setMore([]);
            }}
            placeholder={t('Filter by action, e.g. document.signed')}
            className="ms-auto max-w-xs"
          />
        </div>
        {first.isPending ? (
          <Loading />
        ) : (
          <ul className="grid gap-1.5">
            {entries.map((e) => (
              <li
                key={e.id}
                className="grid gap-0.5 rounded-md border px-3 py-2 sm:grid-cols-[180px_minmax(0,1fr)_auto] sm:items-center sm:gap-3"
              >
                <span className="text-xs text-muted-foreground">{formatDateTime(e.occurredAt)}</span>
                <span className="min-w-0 truncate">
                  <code className="text-xs">{e.action}</code>{' '}
                  <span className="text-muted-foreground">{t('by {name}', { name: e.actorLabel })}</span>
                </span>
                <span className="text-xs text-muted-foreground">
                  {e.entityType}
                  {e.surface !== 'web' ? ` · ${e.surface}` : ''}
                </span>
              </li>
            ))}
          </ul>
        )}
        {first.data?.length === PAGE && (
          <div className="mt-4 flex justify-center">
            <Button variant="outline" onClick={loadMore} disabled={loadingMore}>
              {t('Show Older')}
            </Button>
          </div>
        )}
      </SettingsCard>
    </div>
  );
}
