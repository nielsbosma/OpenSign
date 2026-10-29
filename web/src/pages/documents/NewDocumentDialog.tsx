import { Upload } from 'lucide-react';
import { type FormEvent, useState } from 'react';
import { useNavigate } from 'react-router';
import { Field, Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { call, errorMessage, uploadFile, useApi } from '@/lib/api';
import { t } from '@/lib/i18n';
import { FormError } from '../auth/AuthLayout';
import { announceUpload } from './SuggestedFields';

export const ACCEPT = '.pdf,.docx,.doc,.odt,.rtf,application/pdf';

function titleFrom(name: string) {
  return name
    .replace(/\.[^.]+$/, '')
    .replace(/[_-]+/g, ' ')
    .trim();
}

export function FileDrop({ onFiles, busy }: { onFiles: (files: File[]) => void; busy?: boolean }) {
  const [over, setOver] = useState(false);
  return (
    <label
      onDragOver={(e) => {
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setOver(false);
        onFiles([...e.dataTransfer.files]);
      }}
      className={`flex cursor-pointer flex-col items-center gap-2 rounded-lg border border-dashed px-6 py-8 text-center transition-colors hover:bg-accent/50 ${over ? 'border-primary bg-primary/5' : ''}`}
    >
      {busy ? <Spinner className="size-5" /> : <Upload className="size-5 text-muted-foreground" />}
      <span className="font-medium">{t('Drop PDF or Word files here')}</span>
      <span className="text-xs text-muted-foreground">{t('or click to choose. Word documents are converted to PDF.')}</span>
      <input type="file" accept={ACCEPT} multiple className="sr-only" onChange={(e) => onFiles([...(e.target.files ?? [])])} />
    </label>
  );
}

export function NewDocumentDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const navigate = useNavigate();
  const templates = useApi('documents.list', { templates: true, limit: 100 }, { enabled: open });
  const [title, setTitle] = useState('');
  const [templateId, setTemplateId] = useState('none');
  const [files, setFiles] = useState<File[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(undefined);
    try {
      const fromTemplate = templateId !== 'none' ? templateId : null;
      const doc = await call('documents.create', {
        title: title.trim() || (files[0] ? titleFrom(files[0].name) : t('Untitled Document')),
        templateId: fromTemplate,
      });
      for (const file of files) announceUpload(file.name, await uploadFile(doc.id, file));
      onOpenChange(false);
      navigate(`/documents/${doc.id}`);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <form onSubmit={submit} className="grid gap-4">
          <DialogHeader>
            <DialogTitle>{t('New Document')}</DialogTitle>
            <DialogDescription>{t('Start from files or a template. You add signers and fields next.')}</DialogDescription>
          </DialogHeader>
          <FormError message={error} />
          <Field label={t('Title')} htmlFor="title">
            <Input
              id="title"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder={files[0] ? titleFrom(files[0].name) : t('Employment agreement')}
            />
          </Field>
          {!!templates.data?.length && (
            <Field label={t('Template')}>
              <Select value={templateId} onValueChange={setTemplateId}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="none">{t('No template')}</SelectItem>
                  {templates.data.map((template) => (
                    <SelectItem key={template.id} value={template.id}>
                      {template.title}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
          )}
          <FileDrop onFiles={(f) => setFiles([...files, ...f])} />
          {files.length > 0 && (
            <ul className="grid gap-1 text-muted-foreground">
              {files.map((f, i) => (
                <li key={i} className="flex items-center justify-between gap-2">
                  <span className="truncate">{f.name}</span>
                  <Button type="button" variant="ghost" size="sm" onClick={() => setFiles(files.filter((_, j) => j !== i))}>
                    {t('Remove')}
                  </Button>
                </li>
              ))}
            </ul>
          )}
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              {t('Cancel')}
            </Button>
            <Button type="submit" disabled={busy}>
              {busy && <Spinner className="text-primary-foreground" />} {t('Create')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
