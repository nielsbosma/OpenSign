import { Archive, ArchiveRestore, FileStack, FileText, MoreHorizontal, Plus, Search, X } from 'lucide-react';
import { useState } from 'react';
import { useNavigate } from 'react-router';
import { toast } from 'sonner';
import { EmptyState, Loading, PageHeader } from '@/components/common';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useApi, useApiMutation } from '@/lib/api';
import { formatDate, STATUS_LABELS, STATUS_TONES } from '@/lib/format';
import { t } from '@/lib/i18n';
import { useSession } from '@/lib/session';
import { RecentBulkSends } from './BulkSendPage';
import { ExportDataButton, ExportDataSubmenu } from './ExportData';
import { dragDocuments, FolderSelect, FolderTree, folderPath, MoveToSubmenu } from './Folders';
import { NewDocumentDialog } from './NewDocumentDialog';

const FILTERS = ['all', 'draft', 'pending', 'completed', 'declined', 'canceled', 'expired', 'archived'] as const;
const TEMPLATE_FILTERS = ['all', 'archived'] as const;
const FINISHED = ['completed', 'declined', 'canceled', 'expired'];
const PAGE = 50;

export function StatusBadge({ status }: { status: string }) {
  return <Badge variant={STATUS_TONES[status] ?? 'outline'}>{STATUS_LABELS[status] ?? status}</Badge>;
}

export function DocumentsPage({ templates }: { templates: boolean }) {
  const navigate = useNavigate();
  const { can } = useSession();
  const scope = templates ? 'templates' : 'documents';
  const [filter, setFilter] = useState<(typeof FILTERS)[number]>('all');
  const [folderId, setFolderId] = useState<string | null>(null);
  const [search, setSearch] = useState('');
  const [limit, setLimit] = useState(PAGE);
  const [creating, setCreating] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [shown, setShown] = useState(scope);
  if (shown !== scope) {
    setShown(scope);
    setFilter('all');
    setFolderId(null);
    setSelected(new Set());
  }
  const archived = filter === 'archived';
  const list = useApi('documents.list', {
    templates,
    status: filter === 'all' || archived ? null : filter,
    search: search.trim() || null,
    folderId,
    archived,
    limit,
  });
  const counts = useApi('documents.counts', { search: search.trim() || null, templates, folderId });
  const folders = useApi('folders.list', { scope });
  const createTemplate = useApiMutation('documents.create', { onSuccess: (doc) => navigate(`/documents/${doc.id}`) });
  const move = useApiMutation('documents.move', {
    invalidates: ['documents.', 'folders.'],
    onSuccess: (out) => {
      setSelected(new Set());
      toast.success(out.moved === 1 ? t('Moved 1 item') : t('Moved {count} items', { count: out.moved }));
    },
  });
  const archive = useApiMutation('documents.archive', {
    invalidates: ['documents.', 'folders.'],
    onSuccess: (out, input) => {
      setSelected(new Set());
      toast.success(input.archived ? t('{count} archived', { count: out.changed }) : t('{count} restored', { count: out.changed }));
    },
  });

  const canChange = can('documents.create');
  const allFolders = folders.data ?? [];
  const rows = list.data ?? [];
  const chosen = rows.filter((d) => selected.has(d.id));
  const canArchive = chosen.length > 0 && chosen.every((d) => d.isTemplate || FINISHED.includes(d.status));
  const toggle = (id: string, on: boolean) => {
    const next = new Set(selected);
    if (on) next.add(id);
    else next.delete(id);
    setSelected(next);
  };
  const moveTo = (ids: string[], target: string | null) => move.mutate({ ids, folderId: target });
  const filters = templates ? TEMPLATE_FILTERS : FILTERS;
  const filterLabel = (f: (typeof FILTERS)[number]) => (f === 'all' ? t('All') : f === 'archived' ? t('Archived') : STATUS_LABELS[f]);
  const filterCount = (f: (typeof FILTERS)[number]) =>
    !counts.data ? undefined : f === 'all' ? counts.data.all : f === 'archived' ? counts.data.archived : (counts.data.byStatus[f] ?? 0);
  const folderName = folderId ? folderPath(allFolders, folderId) : null;

  return (
    <>
      <PageHeader
        title={templates ? t('Templates') : t('Documents')}
        description={templates ? t('Reusable documents with parties and fields set up in advance.') : undefined}
        help={templates ? 'templates' : 'documents'}
        actions={
          <>
            {!templates && can('documents.export') && (
              <ExportDataButton
                filter={{ status: filter === 'all' || archived ? null : filter, search: search.trim() || null, folderId, archived }}
              />
            )}
            {canChange && (
              <Button
                onClick={() =>
                  templates ? createTemplate.mutate({ title: t('Untitled Template'), isTemplate: true, folderId }) : setCreating(true)
                }
                disabled={createTemplate.isPending}
              >
                <Plus /> {templates ? t('New Template') : t('New Document')}
              </Button>
            )}
          </>
        }
      />
      <div className="grid gap-6 lg:grid-cols-[200px_minmax(0,1fr)]">
        <aside className="hidden lg:block">
          <FolderTree
            scope={scope}
            folders={allFolders}
            selected={folderId}
            onSelect={(id) => {
              setFolderId(id);
              setSelected(new Set());
            }}
            onMove={moveTo}
            canCreate={templates ? can('templates.manage') : canChange}
          />
        </aside>
        <div className="min-w-0">
          <div className="mb-4 flex flex-col gap-3 2xl:flex-row 2xl:items-center 2xl:justify-between">
            <Tabs
              value={filter}
              onValueChange={(v) => {
                setFilter(v as typeof filter);
                setSelected(new Set());
              }}
            >
              <TabsList className="max-w-full scrollbar-none justify-start overflow-x-auto overflow-y-hidden">
                {filters.map((f) => (
                  <TabsTrigger key={f} value={f}>
                    {f === 'archived' && <Archive className="size-3.5" />}
                    {filterLabel(f)}
                    {filterCount(f) !== undefined && (
                      <span className="text-xs font-normal text-muted-foreground tabular-nums">{filterCount(f)}</span>
                    )}
                  </TabsTrigger>
                ))}
              </TabsList>
            </Tabs>
            <div className="flex flex-col gap-3 sm:flex-row">
              {allFolders.length > 0 && (
                <div className="lg:hidden sm:w-56">
                  <FolderSelect scope={scope} folders={allFolders} selected={folderId} onSelect={setFolderId} />
                </div>
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
          </div>
          {chosen.length > 0 && (
            <div className="mb-3 flex flex-wrap items-center gap-2 rounded-lg border bg-muted/40 px-3 py-2 text-sm">
              <span className="font-medium">{t('{count} selected', { count: chosen.length })}</span>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="outline" size="sm">
                    <MoreHorizontal /> {t('Actions')}
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start">
                  <MoveToSubmenu folders={allFolders} onMove={(target) => moveTo([...selected], target)} />
                  {!templates && can('documents.export') && <ExportDataSubmenu filter={{ ids: [...selected] }} />}
                  {archived ? (
                    <DropdownMenuItem onSelect={() => archive.mutate({ ids: [...selected], archived: false })}>
                      <ArchiveRestore /> {t('Restore')}
                    </DropdownMenuItem>
                  ) : (
                    <DropdownMenuItem
                      disabled={!canArchive}
                      title={canArchive ? undefined : t('Only finished documents can be archived')}
                      onSelect={() => archive.mutate({ ids: [...selected], archived: true })}
                    >
                      <Archive /> {t('Archive')}
                    </DropdownMenuItem>
                  )}
                </DropdownMenuContent>
              </DropdownMenu>
              <Button variant="ghost" size="sm" className="ms-auto" onClick={() => setSelected(new Set())}>
                <X /> {t('Clear')}
              </Button>
            </div>
          )}
          {list.isPending ? (
            <Loading />
          ) : !rows.length ? (
            <EmptyState
              icon={templates ? <FileStack /> : archived ? <Archive /> : <FileText />}
              title={
                search
                  ? t('Nothing matches')
                  : archived
                    ? t('Nothing archived here')
                    : folderName
                      ? t('This folder is empty')
                      : templates
                        ? t('No templates yet')
                        : t('No documents yet')
              }
            >
              {search
                ? null
                : archived
                  ? t('Archive finished documents and unused templates to keep the lists short. They stay searchable here.')
                  : folderName
                    ? t('Drag rows onto a folder, or select them and choose Move to Folder.')
                    : !templates && t('Upload a PDF or Word document, add the people who should sign, and send it.')}
            </EmptyState>
          ) : (
            <div className="rounded-lg border">
              <Table>
                <TableHeader>
                  <TableRow>
                    {canChange && (
                      <TableHead className="w-10">
                        <Checkbox
                          aria-label={t('Select All')}
                          checked={chosen.length === rows.length}
                          onCheckedChange={(on) => setSelected(on === true ? new Set(rows.map((d) => d.id)) : new Set())}
                        />
                      </TableHead>
                    )}
                    <TableHead>{t('Title')}</TableHead>
                    {!templates && <TableHead>{t('Status')}</TableHead>}
                    {!templates && <TableHead>{t('Signed')}</TableHead>}
                    <TableHead className="hidden md:table-cell">{t('Owner')}</TableHead>
                    <TableHead className="text-end">{templates ? t('Created') : t('Updated')}</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {rows.map((doc) => (
                    <TableRow
                      key={doc.id}
                      className="cursor-pointer"
                      data-state={selected.has(doc.id) ? 'selected' : undefined}
                      onClick={() => navigate(`/documents/${doc.id}`)}
                      draggable={canChange}
                      onDragStart={(e) => dragDocuments(e, selected.has(doc.id) ? [...selected] : [doc.id])}
                    >
                      {canChange && (
                        <TableCell onClick={(e) => e.stopPropagation()}>
                          <Checkbox
                            aria-label={t('Select {title}', { title: doc.title })}
                            checked={selected.has(doc.id)}
                            onCheckedChange={(on) => toggle(doc.id, on === true)}
                          />
                        </TableCell>
                      )}
                      <TableCell className="max-w-80">
                        <div className="truncate font-medium">{doc.title}</div>
                        {(doc.externalId || (!folderId && doc.folderId)) && (
                          <div className="truncate text-xs text-muted-foreground">
                            {doc.externalId && <span className="font-mono">{doc.externalId}</span>}
                            {doc.externalId && !folderId && doc.folderId && ' · '}
                            {!folderId && doc.folderId && folderPath(allFolders, doc.folderId)}
                          </div>
                        )}
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
        </div>
      </div>
      <NewDocumentDialog open={creating} onOpenChange={setCreating} folderId={folderId} />
    </>
  );
}
