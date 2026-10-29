import { ArrowLeft } from 'lucide-react';
import { Link, useParams } from 'react-router';
import { ConfirmButton, Loading, PageHeader } from '@/components/common';
import { Badge } from '@/components/ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { useApi, useApiMutation } from '@/lib/api';
import { formatDateTime } from '@/lib/format';
import { labels, msg, t } from '@/lib/i18n';

const ROW_TONES: Record<string, 'default' | 'secondary' | 'destructive' | 'outline'> = {
  sent: 'default',
  failed: 'destructive',
  queued: 'outline',
  canceled: 'secondary',
};
const ROW_LABELS = labels({ sent: msg('Sent'), failed: msg('Failed'), queued: msg('Waiting'), canceled: msg('Skipped') });
const STATUS_LABELS = labels({
  queued: msg('Waiting to Start'),
  running: msg('Sending'),
  done: msg('Finished'),
  canceled: msg('Canceled'),
});

export function BulkSendPage() {
  const { id = '' } = useParams();
  const bulk = useApi(
    'bulk.get',
    { id },
    { refetchInterval: (q) => (q.state.data && ['queued', 'running'].includes(q.state.data.status) ? 2000 : false) },
  );
  const cancel = useApiMutation('bulk.cancel', { invalidates: ['bulk.get', 'bulk.list'] });
  if (bulk.isPending) return <Loading />;
  if (!bulk.data) return null;
  const b = bulk.data;
  const handled = b.sent + b.failed;
  const active = b.status === 'queued' || b.status === 'running';

  return (
    <>
      <Link to="/templates" className="mb-3 inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground">
        <ArrowLeft className="size-4 rtl:rotate-180" /> {t('Templates')}
      </Link>
      <PageHeader
        title={t('Bulk Send: {template}', { template: b.templateTitle })}
        description={`${b.fileName} · ${t('started {date}', { date: formatDateTime(b.createdAt) })}`}
        help="bulk-send"
        actions={
          active && (
            <ConfirmButton
              label={t('Stop')}
              title={t('Stop this bulk send?')}
              description={t('Rows not sent yet are skipped. Documents already sent stay out for signing.')}
              confirmLabel={t('Stop Sending')}
              onConfirm={() => cancel.mutate({ id: b.id })}
            />
          )
        }
      />
      <div className="mb-4 grid gap-2">
        <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-sm">
          <span className="font-medium">{STATUS_LABELS[b.status] ?? b.status}</span>
          <span className="tabular text-muted-foreground">
            {handled} of {b.total} handled · {b.sent} sent · {b.failed} {t('failed')}
          </span>
        </div>
        <div className="h-2 overflow-hidden rounded-full bg-muted">
          <div className="h-full bg-primary transition-all" style={{ width: `${b.total ? (handled / b.total) * 100 : 0}%` }} />
        </div>
      </div>
      <div className="rounded-lg border">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-16">{t('Row')}</TableHead>
              <TableHead>{t('Recipient')}</TableHead>
              <TableHead>{t('Status')}</TableHead>
              <TableHead>{t('Document')}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {b.rows.map((r) => (
              <TableRow key={r.rowNumber}>
                <TableCell className="tabular text-muted-foreground">{r.rowNumber}</TableCell>
                <TableCell className="font-medium">{r.label}</TableCell>
                <TableCell>
                  <Badge variant={ROW_TONES[r.status] ?? 'outline'}>{ROW_LABELS[r.status] ?? r.status}</Badge>
                  {r.error && <div className="mt-1 text-xs text-destructive">{r.error}</div>}
                </TableCell>
                <TableCell>
                  {r.documentId && (
                    <Link to={`/documents/${r.documentId}`} className="underline">
                      {t('Open')}
                    </Link>
                  )}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
    </>
  );
}

/** The latest bulk sends, under the templates list. */
export function RecentBulkSends() {
  const list = useApi('bulk.list', {});
  if (!list.data?.length) return null;
  return (
    <section className="mt-8 grid gap-2">
      <h2 className="font-medium">{t('Bulk Sends')}</h2>
      <div className="rounded-lg border">
        <Table>
          <TableBody>
            {list.data.slice(0, 10).map((b) => (
              <TableRow key={b.id}>
                <TableCell>
                  <Link to={`/bulk/${b.id}`} className="font-medium hover:underline">
                    {b.templateTitle}
                  </Link>
                  <div className="text-xs text-muted-foreground">{b.fileName}</div>
                </TableCell>
                <TableCell className="tabular text-muted-foreground">
                  {t('{sent} of {total} sent', { sent: b.sent, total: b.total })}
                  {b.failed ? ` · ${t('{count} failed', { count: b.failed })}` : ''}
                </TableCell>
                <TableCell className="text-end text-muted-foreground">{formatDateTime(b.createdAt)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
    </section>
  );
}
