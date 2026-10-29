import { ArrowLeft, History, RefreshCw, RotateCcw } from 'lucide-react';
import { useState } from 'react';
import { Link, useParams } from 'react-router';
import { toast } from 'sonner';
import { EmptyState, Loading, Spinner, StatusDot } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { call, type DeliveryView, type Input, useApi, useApiMutation } from '@/lib/api';
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

/** Webhook deliveries and automation runs share one log; only the services differ. */
export type LogKind = 'webhook' | 'automation';
const SERVICES = {
  webhook: {
    list: 'webhooks.deliveries',
    detail: 'webhooks.delivery',
    resend: 'webhooks.resend',
    resendFailed: 'webhooks.resend_failed',
    invalidates: 'webhooks.',
  },
  automation: {
    list: 'automations.runs',
    detail: 'automations.run',
    resend: 'automations.resend',
    resendFailed: 'automations.resend_failed',
    invalidates: 'automations.',
  },
} as const;

function listInput(kind: LogKind, owner: string, rest: { status: Filter | null; limit: number; before?: string }) {
  const input = kind === 'webhook' ? { webhookId: owner, ...rest } : { automationId: owner, ...rest };
  return input as Input<'webhooks.deliveries'>;
}

function ownerInput(kind: LogKind, owner: string) {
  return (kind === 'webhook' ? { webhookId: owner } : { automationId: owner }) as Input<'webhooks.resend_failed'>;
}

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

function DeliveryDetails({ kind, id, onOpen }: { kind: LogKind; id: string; onOpen: (id: string) => void }) {
  const services = SERVICES[kind];
  const detail = useApi(services.detail as 'webhooks.delivery', { id });
  const resend = useApiMutation(services.resend as 'webhooks.resend', {
    invalidates: [services.invalidates],
    onSuccess: (out) => {
      toast.success(kind === 'webhook' ? t('Delivery queued again') : t('Run queued again'));
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
          {resend.isPending ? <Spinner /> : <RotateCcw />} {kind === 'webhook' ? t('Resend') : t('Run Again')}
        </Button>
      </div>
      {(d.resentFrom || d.resentAs.length > 0) && (
        <div className="grid gap-1 text-sm">
          {d.resentFrom && (
            <button type="button" className="text-start text-primary hover:underline" onClick={() => onOpen(d.resentFrom as string)}>
              {kind === 'webhook' ? t('Resends an earlier delivery') : t('Runs an earlier event again')}
            </button>
          )}
          {d.resentAs.map((r) => (
            <button key={r} type="button" className="text-start text-primary hover:underline" onClick={() => onOpen(r)}>
              {kind === 'webhook' ? t('Resent as a new delivery') : t('Run again as a new run')}
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
        <h3 className="font-medium">{kind === 'webhook' ? t('Payload') : t('Event')}</h3>
        <pre className="overflow-auto rounded-md border bg-muted/50 px-3 py-2 font-mono text-xs" dir="ltr">
          {JSON.stringify(d.payload, null, 2)}
        </pre>
      </section>
    </div>
  );
}

export function WebhookDeliveriesPage() {
  const { id = '' } = useParams();
  const hooks = useApi('webhooks.list');
  const hook = hooks.data?.find((h) => h.id === id);
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
    <DeliveryLog
      kind="webhook"
      owner={id}
      failed={hook?.failed ?? 0}
      help="webhooks#delivery-log"
      title={t('Deliveries')}
      subtitle={hook?.url}
      description={t('Every event sent to this webhook in the last 30 days, with each attempt and the answer.')}
      back={{ to: '/settings/webhooks', label: t('All Webhooks') }}
    />
  );
}

export function DeliveryLog({
  kind,
  owner,
  failed,
  help,
  title,
  subtitle,
  description,
  back,
}: {
  kind: LogKind;
  owner: string;
  failed: number;
  help: string;
  title: string;
  subtitle?: string;
  description: string;
  back: { to: string; label: string };
}) {
  const services = SERVICES[kind];
  const rtl = isRtl(useLanguage().language);
  const [filter, setFilter] = useState<Filter>('all');
  const [open, setOpen] = useState<string | null>(null);
  const [more, setMore] = useState<DeliveryView[]>([]);
  const [loadingMore, setLoadingMore] = useState(false);
  const status = filter === 'all' ? null : filter;
  const first = useApi(services.list as 'webhooks.deliveries', listInput(kind, owner, { status, limit: PAGE }));
  const resendFailed = useApiMutation(services.resendFailed as 'webhooks.resend_failed', {
    invalidates: [services.invalidates],
    onSuccess: (out) =>
      toast.success(
        kind === 'webhook'
          ? t('Deliveries queued again: {count}', { count: out.count })
          : t('Runs queued again: {count}', { count: out.count }),
      ),
  });
  const deliveries = [...(first.data ?? []), ...more];

  const loadMore = async () => {
    const last = deliveries.at(-1);
    if (!last) return;
    setLoadingMore(true);
    try {
      const page = await call(services.list as 'webhooks.deliveries', listInput(kind, owner, { status, limit: PAGE, before: last.id }));
      setMore([...more, ...page]);
    } finally {
      setLoadingMore(false);
    }
  };

  return (
    <SettingsCard
      help={help}
      title={title}
      description={
        <span className="grid gap-1">
          {subtitle && (
            <span className="truncate font-mono text-xs" dir="ltr">
              {subtitle}
            </span>
          )}
          <span>{description}</span>
        </span>
      }
    >
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <Button variant="ghost" size="sm" asChild>
          <Link to={back.to}>
            <ArrowLeft className="rtl:rotate-180" /> {back.label}
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
          {failed > 0 && (
            <Button size="sm" onClick={() => resendFailed.mutate(ownerInput(kind, owner))} disabled={resendFailed.isPending}>
              <RotateCcw /> {kind === 'webhook' ? t('Resend {count} Failed', { count: failed }) : t('Run {count} Failed Again', { count: failed })}
            </Button>
          )}
        </div>
      </div>
      {first.isPending ? (
        <Loading />
      ) : !deliveries.length ? (
        <EmptyState icon={<History />} title={kind === 'webhook' ? t('No deliveries') : t('No runs yet')} />
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
                  {d.resentFrom ? `${kind === 'webhook' ? t('Resend') : t('Run Again')} · ` : ''}
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
            <SheetTitle>{kind === 'webhook' ? t('Delivery') : t('Run')}</SheetTitle>
            <SheetDescription className="font-mono text-xs" dir="ltr">
              {open}
            </SheetDescription>
          </SheetHeader>
          {open && <DeliveryDetails key={open} kind={kind} id={open} onOpen={setOpen} />}
        </SheetContent>
      </Sheet>
    </SettingsCard>
  );
}
