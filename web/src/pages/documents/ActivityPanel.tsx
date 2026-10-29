import { Spinner } from '@/components/common';
import { type DocumentView, useApi } from '@/lib/api';
import { formatDateTime } from '@/lib/format';
import { t } from '@/lib/i18n';
import { useSession } from '@/lib/session';

function timeline(doc: DocumentView) {
  const events: { at: string; text: string }[] = [
    { at: doc.createdAt, text: doc.ownerName ? t('Created by {name}', { name: doc.ownerName }) : t('Created') },
  ];
  if (doc.sentAt) events.push({ at: doc.sentAt, text: t('Sent for signing') });
  for (const p of doc.parties) {
    if (p.notifiedAt) events.push({ at: p.notifiedAt, text: t('{name} was notified', { name: p.name }) });
    if (p.viewedAt) events.push({ at: p.viewedAt, text: t('{name} opened the document', { name: p.name }) });
    if (p.signedAt)
      events.push({
        at: p.signedAt,
        text: p.role === 'approver' ? t('{name} approved', { name: p.name }) : t('{name} signed', { name: p.name }),
      });
    if (p.declinedAt)
      events.push({
        at: p.declinedAt,
        text: p.declineReason
          ? t('{name} declined: {reason}', { name: p.name, reason: p.declineReason })
          : t('{name} declined', { name: p.name }),
      });
  }
  if (doc.completedAt) events.push({ at: doc.completedAt, text: t('Completed and sealed') });
  if (doc.canceledAt) events.push({ at: doc.canceledAt, text: t('Canceled') });
  return events.sort((a, b) => a.at.localeCompare(b.at));
}

/** What happened to the document. Auditors also see the tamper-evident audit entries behind it. */
export function ActivityPanel({ document }: { document: DocumentView }) {
  const { can } = useSession();
  const audit = useApi('audit.list', { entityType: 'document', entityId: document.id, limit: 100 }, { enabled: can('audit.read') });
  return (
    <div className="grid gap-6">
      <ol className="grid gap-3 border-s ps-4">
        {timeline(document).map((e, i) => (
          <li key={i} className="relative">
            <span className="absolute -start-[21px] top-1.5 size-2 rounded-full bg-primary" />
            <div>{e.text}</div>
            <div className="text-xs text-muted-foreground">{formatDateTime(e.at)}</div>
          </li>
        ))}
      </ol>
      {can('audit.read') && (
        <section className="grid gap-2">
          <h3 className="font-medium">{t('Audit Trail')}</h3>
          {audit.isPending ? (
            <Spinner />
          ) : (
            <ul className="grid gap-2">
              {audit.data?.map((entry) => (
                <li key={entry.id} className="rounded-md border px-3 py-2">
                  <div className="flex items-center justify-between gap-2">
                    <span className="font-mono text-xs">{entry.action}</span>
                    <span className="text-xs text-muted-foreground">{formatDateTime(entry.occurredAt)}</span>
                  </div>
                  <div className="text-xs text-muted-foreground">
                    {entry.actorLabel} · {entry.surface}
                  </div>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}
    </div>
  );
}
