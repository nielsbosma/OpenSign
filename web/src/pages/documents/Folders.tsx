import { Folder, FolderInput, FolderOpen, FolderPlus, Layers, MoreHorizontal, Pencil, Trash2 } from 'lucide-react';
import { type DragEvent, type FormEvent, type ReactNode, useState } from 'react';
import { Field } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { type Output, useApiMutation } from '@/lib/api';
import { t } from '@/lib/i18n';
import { cn } from '@/lib/utils';
import { FormError } from '../auth/AuthLayout';

export type FolderView = Output<'folders.list'>[number];
export type Scope = 'documents' | 'templates';

export const MAX_DEPTH = 3;
const DRAG_TYPE = 'application/x-opensign-documents';

/** Folders in tree order, each with how deep it sits (0 for the top level). */
export function flatten(folders: FolderView[]): { folder: FolderView; depth: number }[] {
  const out: { folder: FolderView; depth: number }[] = [];
  const visit = (parent: string | null, depth: number) => {
    for (const folder of folders.filter((f) => (f.parentId ?? null) === parent)) {
      out.push({ folder, depth });
      if (depth < MAX_DEPTH) visit(folder.id, depth + 1);
    }
  };
  visit(null, 0);
  return out;
}

export function folderPath(folders: FolderView[], id: string | null | undefined): string {
  const names: string[] = [];
  let current = folders.find((f) => f.id === id);
  while (current && names.length <= MAX_DEPTH) {
    names.unshift(current.name);
    current = folders.find((f) => f.id === current?.parentId);
  }
  return names.join(' / ');
}

export function dragDocuments(e: DragEvent, ids: string[]) {
  e.dataTransfer.setData(DRAG_TYPE, JSON.stringify(ids));
  e.dataTransfer.effectAllowed = 'move';
}

function droppedDocuments(e: DragEvent): string[] | null {
  if (!e.dataTransfer.types.includes(DRAG_TYPE)) return null;
  try {
    return JSON.parse(e.dataTransfer.getData(DRAG_TYPE));
  } catch {
    return null;
  }
}

type Editing = { mode: 'create'; parentId: string | null } | { mode: 'rename'; folder: FolderView };

function FolderDialog({ scope, editing, onClose }: { scope: Scope; editing: Editing | null; onClose: () => void }) {
  const [name, setName] = useState('');
  const [error, setError] = useState<string>();
  const [opened, setOpened] = useState<Editing | null>(null);
  if (editing !== opened) {
    setOpened(editing);
    setName(editing?.mode === 'rename' ? editing.folder.name : '');
    setError(undefined);
  }
  const create = useApiMutation('folders.create', { invalidates: ['folders.'], silent: true, onSuccess: onClose });
  const update = useApiMutation('folders.update', { invalidates: ['folders.'], silent: true, onSuccess: onClose });
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!editing) return;
    const onError = (e: Error) => setError(e.message);
    if (editing.mode === 'create') create.mutate({ scope, name, parentId: editing.parentId }, { onError });
    else update.mutate({ id: editing.folder.id, name, parentId: editing.folder.parentId ?? null }, { onError });
  };
  return (
    <Dialog open={!!editing} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-sm">
        <form onSubmit={submit} className="grid gap-4">
          <DialogHeader>
            <DialogTitle>
              {editing?.mode === 'rename' ? t('Rename Folder') : editing?.parentId ? t('New Subfolder') : t('New Folder')}
            </DialogTitle>
            <DialogDescription>{t('Folders are shared with everyone, but only show what each person may already see.')}</DialogDescription>
          </DialogHeader>
          <Field label={t('Name')} htmlFor="folder-name">
            <Input id="folder-name" value={name} onChange={(e) => setName(e.target.value)} maxLength={100} autoFocus />
          </Field>
          <FormError message={error} />
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              {t('Cancel')}
            </Button>
            <Button type="submit" disabled={!name.trim() || create.isPending || update.isPending}>
              {editing?.mode === 'rename' ? t('Save') : t('Create')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function DeleteDialog({
  folder,
  onClose,
  onDeleted,
}: {
  folder: FolderView | null;
  onClose: () => void;
  onDeleted: (f: FolderView) => void;
}) {
  const remove = useApiMutation('folders.delete', {
    invalidates: ['folders.', 'documents.'],
    onSuccess: () => {
      if (folder) onDeleted(folder);
      onClose();
    },
  });
  return (
    <Dialog open={!!folder} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>{t('Delete “{name}”?', { name: folder?.name ?? '' })}</DialogTitle>
          <DialogDescription>{t('Nothing in it is deleted. Its contents and subfolders move up one level.')}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t('Cancel')}
          </Button>
          <Button variant="destructive" disabled={remove.isPending} onClick={() => folder && remove.mutate({ id: folder.id })}>
            {t('Delete Folder')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function Row({
  label,
  icon,
  count,
  depth,
  active,
  onSelect,
  onDrop,
  menu,
}: {
  label: string;
  icon: ReactNode;
  count?: number;
  depth: number;
  active: boolean;
  onSelect: () => void;
  onDrop?: (ids: string[]) => void;
  menu?: ReactNode;
}) {
  const [over, setOver] = useState(false);
  return (
    <div
      className={cn(
        'group flex items-center rounded-md text-[13px] transition-colors',
        active ? 'bg-accent font-medium text-accent-foreground' : 'text-muted-foreground hover:bg-accent/60 hover:text-foreground',
        over && 'ring-2 ring-primary/60',
      )}
      onDragOver={(e) => {
        if (!onDrop || !e.dataTransfer.types.includes(DRAG_TYPE)) return;
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => {
        setOver(false);
        const ids = droppedDocuments(e);
        if (ids?.length && onDrop) {
          e.preventDefault();
          onDrop(ids);
        }
      }}
    >
      <button
        type="button"
        onClick={onSelect}
        className="flex min-w-0 flex-1 items-center gap-2 py-1.5 pe-1 text-start"
        style={{ paddingInlineStart: `${0.5 + depth * 0.875}rem` }}
        aria-current={active ? 'true' : undefined}
      >
        <span className="shrink-0 [&_svg]:size-4">{icon}</span>
        <span className="truncate">{label}</span>
        {!!count && <span className="ms-auto text-xs tabular-nums text-muted-foreground">{count}</span>}
      </button>
      {menu}
    </div>
  );
}

/** The folder column beside the Documents and Templates lists. `null` means every folder. */
export function FolderTree({
  scope,
  folders,
  selected,
  onSelect,
  onMove,
  canCreate,
}: {
  scope: Scope;
  folders: FolderView[];
  selected: string | null;
  onSelect: (id: string | null) => void;
  onMove: (ids: string[], folderId: string | null) => void;
  canCreate: boolean;
}) {
  const [editing, setEditing] = useState<Editing | null>(null);
  const [deleting, setDeleting] = useState<FolderView | null>(null);
  return (
    <nav aria-label={t('Folders')} className="grid content-start gap-0.5">
      <div className="mb-1 flex items-center justify-between ps-2">
        <span className="text-xs font-medium text-muted-foreground">{t('Folders')}</span>
        {canCreate && (
          <Button
            variant="ghost"
            size="icon"
            className="size-7 text-muted-foreground"
            aria-label={t('New Folder')}
            title={t('New Folder')}
            onClick={() => setEditing({ mode: 'create', parentId: null })}
          >
            <FolderPlus />
          </Button>
        )}
      </div>
      <Row
        label={scope === 'templates' ? t('All Templates') : t('All Documents')}
        icon={<Layers />}
        depth={0}
        active={selected === null}
        onSelect={() => onSelect(null)}
        onDrop={(ids) => onMove(ids, null)}
      />
      {flatten(folders).map(({ folder, depth }) => (
        <Row
          key={folder.id}
          label={folder.name}
          icon={selected === folder.id ? <FolderOpen /> : <Folder />}
          count={folder.itemCount}
          depth={depth}
          active={selected === folder.id}
          onSelect={() => onSelect(folder.id)}
          onDrop={(ids) => onMove(ids, folder.id)}
          menu={
            (folder.canEdit || (canCreate && depth < MAX_DEPTH - 1)) && (
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="size-7 shrink-0 text-muted-foreground opacity-0 group-hover:opacity-100 focus-visible:opacity-100 data-[state=open]:opacity-100"
                    aria-label={t('Folder Actions')}
                  >
                    <MoreHorizontal />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  {canCreate && depth < MAX_DEPTH - 1 && (
                    <DropdownMenuItem onSelect={() => setEditing({ mode: 'create', parentId: folder.id })}>
                      <FolderPlus /> {t('New Subfolder')}
                    </DropdownMenuItem>
                  )}
                  {folder.canEdit && (
                    <>
                      <DropdownMenuItem onSelect={() => setEditing({ mode: 'rename', folder })}>
                        <Pencil /> {t('Rename')}
                      </DropdownMenuItem>
                      <DropdownMenuSeparator />
                      <DropdownMenuItem className="text-destructive" onSelect={() => setDeleting(folder)}>
                        <Trash2 /> {t('Delete Folder')}
                      </DropdownMenuItem>
                    </>
                  )}
                </DropdownMenuContent>
              </DropdownMenu>
            )
          }
        />
      ))}
      <FolderDialog scope={scope} editing={editing} onClose={() => setEditing(null)} />
      <DeleteDialog
        folder={deleting}
        onClose={() => setDeleting(null)}
        onDeleted={(f) => selected === f.id && onSelect(f.parentId ?? null)}
      />
    </nav>
  );
}

/** For narrow screens, where the folder column does not fit. */
export function FolderSelect({
  scope,
  folders,
  selected,
  onSelect,
}: {
  scope: Scope;
  folders: FolderView[];
  selected: string | null;
  onSelect: (id: string | null) => void;
}) {
  return (
    <select
      aria-label={t('Folder')}
      value={selected ?? ''}
      onChange={(e) => onSelect(e.target.value || null)}
      className="h-9 w-full rounded-md border bg-background px-2 text-sm"
    >
      <option value="">{scope === 'templates' ? t('All Templates') : t('All Documents')}</option>
      {flatten(folders).map(({ folder, depth }) => (
        <option key={folder.id} value={folder.id}>
          {`${'  '.repeat(depth)}${folder.name}`}
        </option>
      ))}
    </select>
  );
}

/** "Move to" as a submenu of an existing dropdown. */
export function MoveToSubmenu({
  folders,
  current,
  onMove,
}: {
  folders: FolderView[];
  current?: string | null;
  onMove: (folderId: string | null) => void;
}) {
  return (
    <DropdownMenuSub>
      <DropdownMenuSubTrigger>
        <FolderInput /> {t('Move to Folder')}
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent className="max-h-80 overflow-y-auto">
        <DropdownMenuItem disabled={current === null} onSelect={() => onMove(null)}>
          <Layers /> {t('No Folder')}
        </DropdownMenuItem>
        {flatten(folders).map(({ folder, depth }) => (
          <DropdownMenuItem
            key={folder.id}
            disabled={current === folder.id}
            onSelect={() => onMove(folder.id)}
            style={{ paddingInlineStart: `${0.5 + depth * 0.875}rem` }}
          >
            <Folder /> {folder.name}
          </DropdownMenuItem>
        ))}
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  );
}
