import { FileStack, FileText, Plus, Search } from 'lucide-react';
import { useState } from 'react';
import { useNavigate } from 'react-router';
import { EmptyState, Loading, PageHeader } from '@/components/common';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useApi, useApiMutation } from '@/lib/api';
import { formatDate, STATUS_LABELS, STATUS_TONES } from '@/lib/format';
import { t } from '@/lib/i18n';
import { useSession } from '@/lib/session';
import { RecentBulkSends } from './BulkSendPage';
import { NewDocumentDialog } from './NewDocumentDialog';

const FILTERS = ['all', 'draft', 'pending', 'completed', 'declined', 'canceled', 'expired'] as const;
const PAGE = 50;

export function StatusBadge({ status }: { status: string }) {
  return <Badge variant={STATUS_TONES[status] ?? 'outline'}>{STATUS_LABELS[status] ?? status}</Badge>;
}

export function DocumentsPage({ templates }: { templates: boolean }) {
  const navigate = useNavigate();
  const { can } = useSession();
  const [filter, setFilter] = useState<(typeof FILTERS)[number]>('all');
  const [search, setSearch] = useState('');
  const [limit, setLimit] = useState(PAGE);
  const [creating, setCreating] = useState(false);
  const list = useApi('documents.list', {
    templates,
    status: templates || filter === 'all' ? null : filter,
    search: search.trim() || null,
    limit,
  });
  const createTemplate = useApiMutation('documents.create', { onSuccess: (doc) => navigate(`/documents/${doc.id}`) });

  return (
    <>
      <PageHeader
        title={templates ? t('Templates') : t('Documents')}
        description={templates ? t('Reusable documents with parties and fields set up in advance.') : undefined}
        help={templates ? 'templates' : 'documents'}
        actions={
          can('documents.create') && (
            <Button
              onClick={() => (templates ? createTemplate.mutate({ title: t('Untitled Template'), isTemplate: true }) : setCreating(true))}
              disabled={createTemplate.isPending}
            >
              <Plus /> {templates ? t('New Template') : t('New Document')}
            </Button>
          )
        }
      />
      <div className="mb-4 flex flex-col gap-3 xl:flex-row xl:items-center xl:justify-between">
        {!templates && (
          <Tabs value={filter} onValueChange={(v) => setFilter(v as typeof filter)}>
            <TabsList className="max-w-full scrollbar-none justify-start overflow-x-auto overflow-y-hidden">
              {FILTERS.map((f) => (
                <TabsTrigger key={f} value={f}>
                  {f === 'all' ? t('All') : STATUS_LABELS[f]}
                </TabsTrigger>
              ))}
            </TabsList>
          </Tabs>
        )}
        <div className="relative sm:w-72">
          <Search className="pointer-events-none absolute top-2.5 start-2.5 size-4 text-muted-foreground" />
          <Input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t('Search by title or reference')}
            className="ps-8"
          />
        </div>
      </div>
      {list.isPending ? (
        <Loading />
      ) : !list.data?.length ? (
        <EmptyState
          icon={templates ? <FileStack /> : <FileText />}
          title={search ? t('Nothing matches') : templates ? t('No templates yet') : t('No documents yet')}
        >
          {!search && !templates && t('Upload a PDF or Word document, add the people who should sign, and send it.')}
        </EmptyState>
      ) : (
        <div className="rounded-lg border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t('Title')}</TableHead>
                {!templates && <TableHead>{t('Status')}</TableHead>}
                {!templates && <TableHead>{t('Signed')}</TableHead>}
                <TableHead className="hidden md:table-cell">{t('Owner')}</TableHead>
                <TableHead className="text-end">{templates ? t('Created') : t('Updated')}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {list.data.map((doc) => (
                <TableRow key={doc.id} className="cursor-pointer" onClick={() => navigate(`/documents/${doc.id}`)}>
                  <TableCell className="max-w-80">
                    <div className="truncate font-medium">{doc.title}</div>
                    {doc.externalId && <div className="truncate font-mono text-xs text-muted-foreground">{doc.externalId}</div>}
                  </TableCell>
                  {!templates && (
                    <TableCell>
                      <StatusBadge status={doc.status} />
                    </TableCell>
                  )}
                  {!templates && (
                    <TableCell className="tabular text-muted-foreground">
                      {doc.partiesTotal ? t('{done} of {total}', { done: doc.partiesDone, total: doc.partiesTotal }) : '–'}
                    </TableCell>
                  )}
                  <TableCell className="hidden text-muted-foreground md:table-cell">{doc.ownerName}</TableCell>
                  <TableCell className="text-end text-muted-foreground">
                    {formatDate(doc.completedAt ?? doc.sentAt ?? doc.createdAt)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}
      {list.data?.length === limit && (
        <div className="mt-4 flex justify-center">
          <Button variant="outline" onClick={() => setLimit(limit + PAGE)}>
            {t('Show More')}
          </Button>
        </div>
      )}
      {templates && <RecentBulkSends />}
      <NewDocumentDialog open={creating} onOpenChange={setCreating} />
    </>
  );
}
