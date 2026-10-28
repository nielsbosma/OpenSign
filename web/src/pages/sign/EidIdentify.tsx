import { Smartphone } from 'lucide-react';
import QRCode from 'qrcode';
import { useEffect, useRef, useState } from 'react';
import { Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { type AuthMethod, call, errorMessage } from '@/lib/api';
import { FormError } from '../auth/AuthLayout';
import { useTexts } from './texts';

type Started = { id: string; qr?: string | null; autoStartToken?: string | null };

const POLL_MS = 2000;

function Qr({ value }: { value: string }) {
  const t = useTexts();
  const [src, setSrc] = useState<string>();
  useEffect(() => {
    void QRCode.toDataURL(value, { margin: 1, width: 200 }).then(setSrc);
  }, [value]);
  return src ? <img src={src} alt={t.qrAlt} className="mx-auto size-[200px] rounded-md border" /> : null;
}

/**
 * Identification with BankID, Freja eID or an eID broker. BankID and Freja are polled from here; the broker sends the
 * browser away and back with `resume` set to the identification to pick up.
 */
export function EidIdentify({
  token,
  session,
  method,
  purpose,
  resume,
  onSession,
}: {
  token: string;
  session: string | null;
  method: AuthMethod;
  purpose: 'view' | 'sign';
  resume?: string;
  onSession: (session: string) => void;
}) {
  const [started, setStarted] = useState<Started | null>(resume ? { id: resume, qr: null, autoStartToken: null } : null);
  const [message, setMessage] = useState<string>();
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [showQr, setShowQr] = useState(method === 'freja');
  const onSessionRef = useRef(onSession);
  onSessionRef.current = onSession;
  const t = useTexts();
  const label = t.methods[method] ?? method;

  const id = started?.id;
  useEffect(() => {
    if (!id) return;
    let stopped = false;
    let timer: number | undefined;
    const poll = async () => {
      try {
        const status = await call('signing.eid_status', { token, id });
        if (stopped) return;
        if (status.status === 'complete' && status.session) {
          onSessionRef.current(status.session);
          return;
        }
        if (status.status === 'failed') {
          setStarted(null);
          setError(status.message ?? t.eidFailed);
          return;
        }
        setMessage(status.message ?? undefined);
        if (status.qr) setStarted((s) => (s && s.id === id ? { ...s, qr: status.qr } : s));
      } catch (e) {
        if (stopped) return;
        setStarted(null);
        setError(errorMessage(e));
        return;
      }
      timer = window.setTimeout(poll, POLL_MS);
    };
    timer = window.setTimeout(poll, resume === id ? 0 : POLL_MS);
    return () => {
      stopped = true;
      window.clearTimeout(timer);
    };
  }, [id, token, resume, t]);

  const start = async (sameDevice: boolean) => {
    setBusy(true);
    setError(undefined);
    setMessage(undefined);
    try {
      const out = await call('signing.eid_start', { token, session, purpose });
      if (out.redirectUrl) {
        window.location.assign(out.redirectUrl);
        return;
      }
      setStarted({ id: out.id, qr: out.qr, autoStartToken: out.autoStartToken });
      setShowQr(!sameDevice || method === 'freja');
      if (sameDevice && out.autoStartToken) {
        window.location.href = `bankid:///?autostarttoken=${encodeURIComponent(out.autoStartToken)}&redirect=null`;
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const cancel = () => {
    if (started) void call('signing.eid_cancel', { token, id: started.id }).catch(() => {});
    setStarted(null);
    setMessage(undefined);
  };

  if (started) {
    return (
      <div className="grid gap-4 text-center">
        {showQr && started.qr ? (
          <>
            <p>{t.scanQr(label)}</p>
            <Qr value={started.qr} />
          </>
        ) : (
          <Smartphone className="mx-auto size-10 text-muted-foreground" />
        )}
        <p className="flex items-center justify-center gap-2 text-sm text-muted-foreground" aria-live="polite">
          <Spinner /> {message ?? t.waitingFor(label)}
        </p>
        {method === 'se_bankid' && !showQr && started.qr && (
          <Button variant="outline" onClick={() => setShowQr(true)}>
            {t.bankidOtherDevice}
          </Button>
        )}
        <Button variant="ghost" onClick={cancel}>
          {t.cancel}
        </Button>
      </div>
    );
  }
  return (
    <div className="grid gap-3">
      <p>{t.eidIntro(purpose === 'view', label)}</p>
      <FormError message={error} />
      {method === 'se_bankid' ? (
        <>
          <Button disabled={busy} onClick={() => start(true)}>
            {busy && <Spinner className="text-primary-foreground" />} {t.bankidThisDevice}
          </Button>
          <Button variant="outline" disabled={busy} onClick={() => start(false)}>
            {t.bankidAnotherDevice}
          </Button>
        </>
      ) : (
        <Button disabled={busy} onClick={() => start(false)}>
          {busy && <Spinner className="text-primary-foreground" />} {t.identifyWith(label)}
        </Button>
      )}
    </div>
  );
}
