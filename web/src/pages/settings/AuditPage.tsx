import { ShieldAlert, ShieldCheck } from 'lucide-react';
import { useState } from 'react';
import { Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { type AuditView, call, useApi } from '@/lib/api';
import { formatDateTime } from '@/lib/format';
import { SettingsCard } from './SettingsLayout';

const PAGE = 100;

export function AuditPage() {
  const [action, setAction] = useState('');
  const first = useApi('audit.list', { action: action.trim() || null, limit: PAGE });
  const [more, setMore] = useState<AuditView[]>([]);
  const [loadingMore, setLoadingMore] = useState(false);
  const chain = useApi('audit.verify', undefined, { enabled: false });
  const entries = [...(first.data ?? []), ...more];

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
        title="Audit Trail"
        description="Every change, sign-in and signature, in a hash chain: each entry includes the hash of the one before, so a changed or deleted entry shows."
      >
        <div className="mb-4 flex flex-wrap items-center gap-3">
          <Button variant="outline" onClick={() => chain.refetch()} disabled={chain.isFetching}>
            {chain.isFetching ? <Spinner /> : <ShieldCheck />} Verify the Chain
          </Button>
          {chain.data &&
            (chain.data.valid ? (
              <span className="flex items-center gap-1.5 text-success">
                <ShieldCheck className="size-4" /> All {chain.data.entries} entries intact
              </span>
            ) : (
              <span className="flex items-center gap-1.5 text-destructive">
                <ShieldAlert className="size-4" /> Broken at entry {chain.data.broken_at}
              </span>
            ))}
          <Input
            value={action}
            onChange={(e) => {
              setAction(e.target.value);
              setMore([]);
            }}
            placeholder="Filter by action, e.g. document.signed"
            className="ml-auto max-w-xs font-mono text-xs"
          />
        </div>
        {first.isPending ? (
          <Spinner />
        ) : (
          <ul className="grid gap-1.5">
            {entries.map((e) => (
              <li
                key={e.id}
                className="grid gap-0.5 rounded-md border px-3 py-2 sm:grid-cols-[180px_minmax(0,1fr)_auto] sm:items-center sm:gap-3"
              >
                <span className="text-xs text-muted-foreground">{formatDateTime(e.occurredAt)}</span>
                <span className="min-w-0 truncate">
                  <code className="text-xs">{e.action}</code> <span className="text-muted-foreground">by {e.actorLabel}</span>
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
              Show Older
            </Button>
          </div>
        )}
      </SettingsCard>
    </div>
  );
}
