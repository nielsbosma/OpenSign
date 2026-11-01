import { Camera, CheckCircle2, RotateCcw } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { type AttachmentRequest, errorMessage, uploadAttachment } from '@/lib/api';
import { FormError } from '../auth/AuthLayout';
import { useTexts } from './texts';

const MAX_SIDE = 1280;

/** A live photo of the party, taken with the camera. There is deliberately no file picker. */
export function WebcamPhoto({
  token,
  session,
  request,
  onChange,
  preview,
}: {
  token: string;
  session: string | null;
  request: AttachmentRequest;
  onChange: (request: AttachmentRequest) => void;
  /** The permissions policy only lets the signing page use the camera, so a preview shows the step without starting it. */
  preview?: boolean;
}) {
  const t = useTexts();
  const video = useRef<HTMLVideoElement>(null);
  const [stream, setStream] = useState<MediaStream | null>(null);
  const [shot, setShot] = useState<{ blob: Blob; url: string; saved: boolean } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(
    () => () => {
      for (const track of stream?.getTracks() ?? []) track.stop();
    },
    [stream],
  );
  useEffect(() => () => (shot ? URL.revokeObjectURL(shot.url) : undefined), [shot]);
  useEffect(() => {
    if (video.current && stream) video.current.srcObject = stream;
  }, [stream]);

  const start = async () => {
    setError(undefined);
    setShot(null);
    try {
      setStream(await navigator.mediaDevices.getUserMedia({ video: { facingMode: 'user', width: { ideal: 1280 } }, audio: false }));
    } catch {
      setError(t.photoNoCamera);
    }
  };

  const capture = () => {
    const v = video.current;
    if (!v?.videoWidth) return;
    const scale = Math.min(1, MAX_SIDE / Math.max(v.videoWidth, v.videoHeight));
    const canvas = document.createElement('canvas');
    canvas.width = Math.round(v.videoWidth * scale);
    canvas.height = Math.round(v.videoHeight * scale);
    canvas.getContext('2d')?.drawImage(v, 0, 0, canvas.width, canvas.height);
    canvas.toBlob(
      (blob) => {
        if (!blob) return;
        setShot({ blob, url: URL.createObjectURL(blob), saved: false });
        setStream(null);
      },
      'image/jpeg',
      0.9,
    );
  };

  const use = async () => {
    if (!shot) return;
    setBusy(true);
    setError(undefined);
    try {
      onChange(await uploadAttachment(token, session, request.id, shot.blob));
      setShot({ ...shot, saved: true });
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const saved = !!request.fileId;
  return (
    <div className="grid gap-2">
      <h3 className="text-sm font-medium">{t.photoTitle}</h3>
      <p className="text-xs text-muted-foreground">{t.photoIntro}</p>
      {stream ? (
        <div className="grid gap-2">
          <video ref={video} autoPlay playsInline muted className="aspect-[4/3] w-full -scale-x-100 rounded-md bg-black object-cover" />
          <Button type="button" onClick={capture}>
            <Camera /> {t.photoCapture}
          </Button>
        </div>
      ) : shot ? (
        <div className="grid gap-2">
          <img src={shot.url} alt={t.photoTitle} className="aspect-[4/3] w-full rounded-md object-cover" />
          <div className="flex items-center gap-2">
            {shot.saved && (
              <span className="flex flex-1 items-center gap-1.5 text-xs text-success">
                <CheckCircle2 className="size-4" /> {t.photoSaved}
              </span>
            )}
            <Button type="button" variant="outline" size="sm" disabled={busy} onClick={start}>
              <RotateCcw /> {t.photoRetake}
            </Button>
            {!shot.saved && (
              <Button type="button" size="sm" disabled={busy} onClick={use}>
                {busy ? <Spinner className="text-primary-foreground" /> : <CheckCircle2 />} {t.photoUse}
              </Button>
            )}
          </div>
        </div>
      ) : saved ? (
        <div className="flex items-center gap-2">
          <CheckCircle2 className="size-4 text-success" />
          <span className="flex-1 text-xs">{t.photoSaved}</span>
          <Button type="button" variant="outline" size="sm" onClick={start}>
            <RotateCcw /> {t.photoRetake}
          </Button>
        </div>
      ) : (
        <Button type="button" variant="outline" disabled={preview} onClick={start}>
          <Camera /> {t.photoStart}
        </Button>
      )}
      <FormError message={error} />
    </div>
  );
}
