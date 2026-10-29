import { Camera } from 'lucide-react';
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { type AttachmentRequest, errorMessage, fetchPdf } from '@/lib/api';
import { formatDateTime, saveBytes } from '@/lib/format';
import { t } from '@/lib/i18n';

/** The webcam photo a party took while signing, fetched through the authenticated download so it never has a public URL. */
export function PartyPhoto({ documentId, request }: { documentId: string; request: AttachmentRequest }) {
  const [url, setUrl] = useState<string>();
  const [bytes, setBytes] = useState<ArrayBuffer>();
  useEffect(() => {
    if (!request.fileId) return;
    let objectUrl: string | undefined;
    let live = true;
    fetchPdf(`/api/documents/${documentId}/attachments/${request.id}`)
      .then((b) => {
        if (!live) return;
        setBytes(b);
        objectUrl = URL.createObjectURL(new Blob([b], { type: 'image/png' }));
        setUrl(objectUrl);
      })
      .catch((e) => toast.error(errorMessage(e)));
    return () => {
      live = false;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [documentId, request.id, request.fileId]);

  if (!request.fileId) {
    return (
      <div className="mt-1.5 flex items-center gap-1.5 text-xs text-muted-foreground">
        <Camera className="size-3 shrink-0" /> {t('Webcam photo not taken yet')}
      </div>
    );
  }
  return (
    <div className="mt-2 flex items-start gap-2.5">
      {url ? (
        <img src={url} alt={t('The party, taken with their camera while signing')} className="h-20 w-auto rounded border object-cover" />
      ) : (
        <div className="h-20 w-26 animate-pulse rounded border bg-muted" />
      )}
      <div className="grid gap-0.5 text-xs">
        <span className="flex items-center gap-1.5 font-medium">
          <Camera className="size-3 shrink-0 text-muted-foreground" /> {t('Webcam photo')}
        </span>
        {request.uploadedAt && (
          <span className="text-muted-foreground">{t('Taken {time}', { time: formatDateTime(request.uploadedAt) })}</span>
        )}
        {bytes && (
          <button
            type="button"
            className="justify-self-start underline"
            onClick={() => saveBytes(bytes, request.fileName ?? 'webcam-photo.png', 'image/png')}
          >
            {t('Download')}
          </button>
        )}
      </div>
    </div>
  );
}
