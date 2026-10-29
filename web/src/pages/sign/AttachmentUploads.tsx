import { CheckCircle2, Paperclip, Upload } from 'lucide-react';
import { useRef, useState } from 'react';
import { Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { type AttachmentRequest, call, errorMessage, uploadAttachment } from '@/lib/api';
import { FormError } from '../auth/AuthLayout';
import { useTexts } from './texts';

/** The files the sender asked this party for. Uploads are cleaned on the server and sealed into the signed PDF. */
export function AttachmentUploads({
  token,
  session,
  requests,
  onChange,
  disabled,
}: {
  token: string;
  session: string | null;
  requests: AttachmentRequest[];
  onChange: (requests: AttachmentRequest[]) => void;
  disabled?: boolean;
}) {
  const t = useTexts();
  const [busy, setBusy] = useState<string>();
  const [error, setError] = useState<string>();
  const input = useRef<HTMLInputElement>(null);
  const target = useRef<string>('');

  const replace = (updated: AttachmentRequest) => onChange(requests.map((r) => (r.id === updated.id ? updated : r)));
  const run = async (id: string, fn: () => Promise<void>) => {
    setBusy(id);
    setError(undefined);
    try {
      await fn();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(undefined);
    }
  };
  const pick = (id: string) => {
    target.current = id;
    input.current?.click();
  };
  const upload = (file: File | undefined) => {
    const id = target.current;
    if (!file || !id) return;
    void run(id, async () => replace(await uploadAttachment(token, session, id, file)));
  };
  const remove = (r: AttachmentRequest) =>
    run(r.id, async () => {
      await call('signing.remove_attachment', { token, session, id: r.id });
      replace({ ...r, fileId: null, fileName: null, uploadedAt: null });
    });

  return (
    <div className="grid gap-2">
      <h3 className="text-sm font-medium">{t.attachments}</h3>
      <input
        ref={input}
        type="file"
        accept="application/pdf,image/jpeg,image/png"
        className="hidden"
        onChange={(e) => {
          upload(e.target.files?.[0]);
          e.target.value = '';
        }}
      />
      {requests.map((r) => (
        <div key={r.id} className="rounded-md border p-2.5">
          <div className="flex items-start gap-2">
            {r.fileId ? (
              <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-success" />
            ) : (
              <Paperclip className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
            )}
            <div className="min-w-0 flex-1">
              <div className="font-medium">
                {r.name}
                {!r.required && <span className="font-normal text-muted-foreground"> ({t.optional})</span>}
              </div>
              {r.description && <div className="text-xs text-muted-foreground">{r.description}</div>}
              {r.fileName && <div className="truncate text-xs">{r.fileName}</div>}
            </div>
          </div>
          <div className="mt-2 flex gap-2">
            <Button type="button" variant="outline" size="sm" disabled={disabled || !!busy} onClick={() => pick(r.id)}>
              {busy === r.id ? <Spinner /> : <Upload />} {r.fileId ? t.replaceFile : t.uploadFile}
            </Button>
            {r.fileId && (
              <Button type="button" variant="ghost" size="sm" disabled={disabled || !!busy} onClick={() => remove(r)}>
                {t.removeFile}
              </Button>
            )}
          </div>
        </div>
      ))}
      <p className="text-xs text-muted-foreground">{t.attachmentsNote}</p>
      <FormError message={error} />
    </div>
  );
}
