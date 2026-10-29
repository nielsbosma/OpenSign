import { ArrowLeft, History, RefreshCw, RotateCcw } from 'lucide-react';
import { useState } from 'react';
import { Link, useParams } from 'react-router';
import { toast } from 'sonner';
import { EmptyState, Loading, Spinner, StatusDot } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { call, type DeliveryView, useApi, useApiMutation } from '@/lib/api';
import { formatDateTime } from '@/lib/format';
import { isRtl, msg, t, useLanguage } from '@/lib/i18n';
import { SettingsCard } from './SettingsLayout';

const PAGE = 50;
type Filter = 'all' | 'failed' | 'delivered' | 'pending';
const FILTERS: [Filter, string][] = [
  ['all', msg('All')],
  ['failed', msg('Failed')],
  ['delivered', msg('Delivered')],
  ['pending', msg('Pending')],
];
const STATUS: Record<string, string> = { delivered: msg('Delivered'), pending: msg('Pending'), failed: msg('Failed') };

function tone(d: DeliveryView) {
  if (d.status === 'delivered') return 'ok';
  if (d.status === 'failed') return 'error';
  return d.attempts > 0 ? 'warn' : 'off';
}

function outcome(d: DeliveryView) {
  if (d.status === 'delivered') return t('Answered {status}', { status: d.statusCode ?? 200 });
  if (d.error) return d.error;
  return t('Queued');
}

function DeliveryDetails({ id, onOpen }: { id: string; onOpen: (id: string) => void }) {
  const detail = useApi('webhooks.delivery', { id });
  const resend = useApiMutation('webhooks.resend', {
    invalidates: ['webhooks.'],
    onSuccess: (out) => {
      toast.success(t('Delivery queued again'));
      onOpen(out.id);
    },
  });
  if (!detail.data) return <Loading />;
  const d = detail.data;
  return (
    <div className="grid gap-5 overflow-y-auto px-4 pb-6">
      <div className="flex flex-wrap items-center gap-2">
        <StatusDot tone={tone(d)} />
        <span className="font-medium">{t(STATUS[d.status] ?? d.status)}</span>
        <span className="text-muted-foreground text-sm">
          {d.nextAttemptAt ? t('Next try {time}', { time: formatDateTime(d.nextAttemptAt) }) : ''}
        </span>
        <Button className="ms-auto" variant="outline" size="sm" onClick={() => resend.mutate({ id: d.id })} disabled={resend.isPending}>
          {resend.isPending ? <Spinner /> : <RotateCcw />} {t('Resend')}
        </Button>
      </div>
      {(d.resentFrom || d.resentAs.length > 0) && (
        <div className="grid gap-1 text-sm">
          {d.resentFrom && (
            <button type="button" className="text-start text-primary hover:underline" onClick={() => onOpen(d.resentFrom as string)}>
              {t('Resends an earlier delivery')}
            </button>
          )}
          {d.resentAs.map((r) => (
            <button key={r} type="button" className="text-start text-primary hover:underline" onClick={() => onOpen(r)}>
              {t('Resent as a new delivery')}
            </button>
          ))}
        </div>
      )}
      <section className="grid gap-2">
        <h3 className="font-medium">{t('Attempts')}</h3>
        {d.attemptsLog.length === 0 ? (
          <p className="text-muted-foreground text-sm">{t('Not tried yet.')}</p>
        ) : (
          <ol className="grid gap-2">
            {d.attemptsLog
              .map((a, i) => ({ ...a, number: i + 1 }))
              .reverse()
              .map((a) => (
                <li key={a.createdAt} className="grid gap-1.5 rounded-md border px-3 py-2">
                  <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm">
                    <StatusDot tone={a.error ? 'error' : 'ok'} />
                    <span className="font-medium">{t('Attempt {number}', { number: a.number })}</span>
                    <span className="text-muted-foreground text-xs">{formatDateTime(a.createdAt)}</span>
                    <span className="ms-auto text-muted-foreground text-xs" dir="ltr">
                      {a.durationMs} ms
                    </span>
                  </div>
                  <div className="text-sm">{a.error ?? t('Answered {status}', { status: a.statusCode ?? 200 })}</div>
                  {a.responseExcerpt && (
                    <div className="grid gap-1">
                      <span className="text-muted-foreground text-xs">{t('Response')}</span>
                      <pre
                        className="max-h-48 overflow-auto whitespace-pre-wrap break-all rounded bg-muted/50 px-2 py-1.5 font-mono text-xs"
                        dir="ltr"
                      >
                        {a.responseExcerpt}
                      </pre>
                    </div>
                  )}
                </li>
              ))}
          </ol>
        )}
      </section>
      <section className="grid gap-2">
        <h3 className="font-medium">{t('Payload')}</h3>
        <pre className="overflow-auto rounded-md border bg-muted/50 px-3 py-2 font-mono text-xs" dir="ltr">
          {JSON.stringify(d.payload, null, 2)}
        </pre>
      </section>
    </div>
  );
}

export function WebhookDeliveriesPage() {
  const { id = '' } = useParams();
  const rtl = isRtl(useLanguage().language);
  const [filter, setFilter] = useState<Filter>('all');
  const [open, setOpen] = useState<string | null>(null);
  const [more, setMore] = useState<DeliveryView[]>([]);
  const [loadingMore, setLoadingMore] = useState(false);
  const hooks = useApi('webhooks.list');
  const hook = hooks.data?.find((h) => h.id === id);
  const status = filter === 'all' ? null : filter;
  const first = useApi('webhooks.deliveries', { webhookId: id, status, limit: PAGE });
  const resendFailed = useApiMutation('webhooks.resend_failed', {
    invalidates: ['webhooks.'],
    onSuccess: (out) => toast.success(t('Deliveries queued again: {count}', { count: out.count })),
  });
  const deliveries = [...(first.data ?? []), ...more];

  const loadMore = async () => {
    const last = deliveries.at(-1);
    if (!last) return;
    setLoadingMore(true);
    try {
      setMore([...more, ...(await call('webhooks.deliveries', { webhookId: id, status, limit: PAGE, before: last.id }))]);
    } finally {
      setLoadingMore(false);
    }
  };

  if (hooks.data && !hook) {
    return (
      <SettingsCard title={t('Deliveries')}>
        <EmptyState icon={<History />} title={t('This webhook no longer exists.')}>
          <Link to="/settings/webhooks" className="text-primary hover:underline">
            {t('All Webhooks')}
          </Link>
        </EmptyState>
      </SettingsCard>
    );
  }

  return (
    <SettingsCard
      help="webhooks#delivery-log"
      title={t('Deliveries')}
      description={
        <span className="grid gap-1">
          <span className="truncate font-mono text-xs" dir="ltr">
            {hook?.url}
          </span>
          <span>{t('Every event sent to this webhook in the last 30 days, with each attempt and the answer.')}</span>
        </span>
      }
    >
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <Button variant="ghost" size="sm" asChild>
          <Link to="/settings/webhooks">
            <ArrowLeft className="rtl:rotate-180" /> {t('All Webhooks')}
          </Link>
        </Button>
        <Tabs
          value={filter}
          onValueChange={(v) => {
            setFilter(v as Filter);
            setMore([]);
          }}
        >
          <TabsList>
            {FILTERS.map(([value, label]) => (
              <TabsTrigger key={value} value={value}>
                {t(label)}
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
        <div className="ms-auto flex gap-2">
          <Button variant="outline" size="sm" onClick={() => first.refetch()} disabled={first.isFetching}>
            {first.isFetching ? <Spinner /> : <RefreshCw />} {t('Refresh')}
          </Button>
          {!!hook?.failed && (
            <Button size="sm" onClick={() => resendFailed.mutate({ webhookId: id })} disabled={resendFailed.isPending}>
              <RotateCcw /> {t('Resend {count} Failed', { count: hook.failed })}
            </Button>
          )}
        </div>
      </div>
      {first.isPending ? (
        <Loading />
      ) : !deliveries.length ? (
        <EmptyState icon={<History />} title={t('No deliveries')} />
      ) : (
        <ul className="grid gap-1.5">
          {deliveries.map((d) => (
            <li key={d.id}>
              <button
                type="button"
                onClick={() => setOpen(d.id)}
                className="grid w-full gap-0.5 rounded-md border px-3 py-2 text-start hover:bg-accent/50 sm:grid-cols-[170px_minmax(0,1fr)_auto] sm:items-center sm:gap-3"
              >
                <span className="text-xs text-muted-foreground">{formatDateTime(d.createdAt)}</span>
                <span className="flex min-w-0 flex-wrap items-center gap-x-2 sm:flex-nowrap">
                  <StatusDot tone={tone(d)} />
                  <code className="text-xs">{d.event}</code>
                  <span className="min-w-0 text-muted-foreground text-sm sm:truncate">{outcome(d)}</span>
                </span>
                <span className="text-xs text-muted-foreground">
                  {d.resentFrom ? `${t('Resend')} · ` : ''}
                  {d.nextAttemptAt
                    ? t('Next try {time}', { time: formatDateTime(d.nextAttemptAt) })
                    : t('Attempts: {count}', { count: d.attempts })}
                </span>
              </button>
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
      <Sheet open={!!open} onOpenChange={(o) => !o && setOpen(null)}>
        <SheetContent side={rtl ? 'left' : 'right'} className="w-full sm:max-w-xl">
          <SheetHeader>
            <SheetTitle>{t('Delivery')}</SheetTitle>
            <SheetDescription className="font-mono text-xs" dir="ltr">
              {open}
            </SheetDescription>
          </SheetHeader>
          {open && <DeliveryDetails key={open} id={open} onOpen={setOpen} />}
        </SheetContent>
      </Sheet>
    </SettingsCard>
  );
}
