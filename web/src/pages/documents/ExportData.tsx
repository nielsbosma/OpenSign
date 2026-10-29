import { FileDown, FileJson, FileSpreadsheet, Sheet } from 'lucide-react';
import { useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { call, errorMessage, type Input } from '@/lib/api';
import { saveBytes } from '@/lib/format';
import { t } from '@/lib/i18n';

type Filter = Omit<Input<'documents.export'>, 'format'>;
type Format = NonNullable<Input<'documents.export'>['format']>;

function useExport(filter: Filter) {
  const [busy, setBusy] = useState(false);
  const run = async (format: Format) => {
    setBusy(true);
    try {
      const out = await call('documents.export', { ...filter, format });
      if (out.count === 0) {
        toast.info(t('There are no documents to export'));
        return;
      }
      const bytes = out.contentBase64
        ? Uint8Array.from(atob(out.contentBase64), (c) => c.charCodeAt(0))
        : new TextEncoder().encode(JSON.stringify(out.documents ?? [], null, 2));
      saveBytes(bytes, out.name, out.contentType);
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  return { busy, run };
}

function Items({ run }: { run: (format: Format) => void }) {
  return (
    <>
      <DropdownMenuItem onSelect={() => run('xlsx')}>
        <FileSpreadsheet /> Excel
      </DropdownMenuItem>
      <DropdownMenuItem onSelect={() => run('csv')}>
        <Sheet /> CSV
      </DropdownMenuItem>
      <DropdownMenuItem onSelect={() => run('json')}>
        <FileJson /> JSON
      </DropdownMenuItem>
    </>
  );
}

/** A button that downloads the filled-in values of the documents the filter matches. */
export function ExportDataButton({ filter }: { filter: Filter }) {
  const { busy, run } = useExport(filter);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" disabled={busy} title={t('Download the values filled in on these documents')}>
          <FileDown /> {t('Export Data')}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <Items run={run} />
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export function ExportDataSubmenu({ filter }: { filter: Filter }) {
  const { run } = useExport(filter);
  return (
    <DropdownMenuSub>
      <DropdownMenuSubTrigger>
        <FileDown /> {t('Export Data')}
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent>
        <Items run={run} />
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  );
}
