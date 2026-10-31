import { Smartphone } from 'lucide-react';
import QRCode from 'qrcode';
import { type FormEvent, useEffect, useRef, useState } from 'react';
import { Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { type AuthMethod, call, errorMessage } from '@/lib/api';
import { FormError } from '../auth/AuthLayout';
import { framed } from './embed';
import { useTexts } from './texts';

type Started = { id: string; qr?: string | null; autoStartToken?: string | null; popupUrl?: string };

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
  const [needsCode, setNeedsCode] = useState(false);
  const [code, setCode] = useState('');
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
        if (status.needsCode) {
          setNeedsCode(true);
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
      const out = await call('signing.eid_start', { token, session, purpose, framed });
      if (out.redirectUrl && framed) {
        // Identity providers refuse to run in frames, so the broker gets its own window, through a page of ours that
        // can leave a cookie there, and we wait for it here.
        window.open(out.redirectUrl, '_blank', 'noopener');
        setStarted({ id: out.id, popupUrl: out.redirectUrl });
        return;
      }
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
    setNeedsCode(false);
    setCode('');
  };

  const submitCode = async (e: FormEvent) => {
    e.preventDefault();
    if (!id) return;
    setBusy(true);
    setError(undefined);
    try {
      const status = await call('signing.eid_status', { token, id, code: code.trim() });
      if (status.status === 'complete' && status.session) {
        onSessionRef.current(status.session);
      } else if (status.status === 'failed') {
        setNeedsCode(false);
        setStarted(null);
        setError(status.message ?? t.eidFailed);
      } else {
        setCode('');
        setError(t.eidCodeWrong);
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  if (needsCode) {
    return (
      <form onSubmit={submitCode} className="grid gap-4">
        <p>{t.eidCodeEnter}</p>
        <FormError message={error} />
        <Input
          aria-label={t.eidCodeLabel}
          inputMode="numeric"
          autoComplete="one-time-code"
          maxLength={6}
          value={code}
          onChange={(e) => setCode(e.target.value)}
          autoFocus
        />
        <Button type="submit" disabled={busy || code.trim().length !== 6}>
          {busy && <Spinner className="text-primary-foreground" />} {t.eidCodeSubmit}
        </Button>
        <Button type="button" variant="ghost" onClick={cancel}>
          {t.cancel}
        </Button>
      </form>
    );
  }
  if (started?.popupUrl) {
    return (
      <div className="grid gap-4 text-center">
        <p>{t.eidPopup(label)}</p>
        <p className="flex items-center justify-center gap-2 text-sm text-muted-foreground" aria-live="polite">
          <Spinner /> {t.waitingFor(label)}
        </p>
        <Button variant="outline" asChild>
          <a href={started.popupUrl} target="_blank" rel="noopener noreferrer">
            {t.identifyWith(label)}
          </a>
        </Button>
        <p className="text-xs text-muted-foreground">{t.eidPopupBlocked}</p>
        <Button variant="ghost" onClick={cancel}>
          {t.cancel}
        </Button>
      </div>
    );
  }
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
