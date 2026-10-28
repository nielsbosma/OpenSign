import { CheckCircle2, Download, FileSignature, ShieldCheck, XCircle } from 'lucide-react';
import { type FormEvent, useEffect, useMemo, useState } from 'react';
import { Logo, Spinner } from '@/components/common';
import { PdfLoading, PdfPages, usePdf } from '@/components/PdfPages';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { type AuthMethod, call, errorMessage, fetchPdf, type SignerView } from '@/lib/api';
import { AUTH_LABELS, saveBytes } from '@/lib/format';
import { cn } from '@/lib/utils';
import { FormError } from '../auth/AuthLayout';
import { type Signature, SignaturePad } from './SignaturePad';

/**
 * The page behind a signing link. The token lives in the URL fragment, so it never reaches a server log or a Referer
 * header; we read it once and take it out of the address bar.
 */
function useSigningToken() {
  const [token] = useState(() => {
    const value = decodeURIComponent(window.location.hash.slice(1));
    if (value) {
      try {
        sessionStorage.setItem('opensign-sign', value);
      } catch {}
      history.replaceState(null, '', window.location.pathname);
      return value;
    }
    try {
      return sessionStorage.getItem('opensign-sign') ?? '';
    } catch {
      return '';
    }
  });
  return token;
}

function safeColor(color?: string) {
  return color && /^#[0-9a-fA-F]{6}$/.test(color) ? color : undefined;
}

function Shell({ view, children }: { view?: SignerView; children: React.ReactNode }) {
  return (
    <div className="min-h-svh bg-muted/40">
      <header className="border-b bg-background">
        <div className="mx-auto flex h-14 max-w-5xl items-center gap-3 px-4">
          <Logo className="size-6" />
          <div className="min-w-0 flex-1">
            <div className="truncate text-sm font-semibold">{view?.document.title ?? 'OpenSign'}</div>
            {view && (
              <div className="truncate text-xs text-muted-foreground">
                From {view.document.sender ? `${view.document.sender}, ` : ''}
                {view.document.organisation}
              </div>
            )}
          </div>
          <ShieldCheck className="size-4 text-success" aria-label="Secure connection" />
        </div>
      </header>
      <main className="mx-auto max-w-5xl px-4 py-6">{children}</main>
    </div>
  );
}

function Message({ icon, title, children }: { icon: React.ReactNode; title: string; children?: React.ReactNode }) {
  return (
    <div className="mx-auto mt-10 flex max-w-md flex-col items-center gap-3 rounded-lg border bg-background p-8 text-center">
      {icon}
      <h1 className="text-lg font-semibold">{title}</h1>
      {children && <div className="text-muted-foreground">{children}</div>}
    </div>
  );
}

/** Proves who the party is before they may see or sign. SMS codes work today; eID methods start at the provider. */
function Identify({
  token,
  method,
  purpose,
  onSession,
}: {
  token: string;
  method: AuthMethod;
  purpose: 'view' | 'sign';
  onSession: (session: string) => void;
}) {
  const [sent, setSent] = useState<{ phoneHint: string } | null>(null);
  const [code, setCode] = useState('');
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    setError(undefined);
    try {
      await fn();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  const verify = (e: FormEvent) => {
    e.preventDefault();
    void run(async () => onSession((await call('signing.verify_code', { token, code: code.trim() })).session));
  };

  if (method !== 'sms_pin') {
    return (
      <div className="grid gap-3">
        <p>
          Identify yourself with {AUTH_LABELS[method]} to {purpose === 'view' ? 'open' : 'sign'} the document.
        </p>
        <FormError message={error ?? `${AUTH_LABELS[method]} is not available on this server yet. Ask the sender for another method.`} />
      </div>
    );
  }
  return sent ? (
    <form onSubmit={verify} className="grid gap-3">
      <p>We sent a 6-digit code to the number ending {sent.phoneHint}.</p>
      <FormError message={error} />
      <Input
        value={code}
        onChange={(e) => setCode(e.target.value)}
        inputMode="numeric"
        autoComplete="one-time-code"
        className="font-mono text-lg tracking-widest"
        aria-label="Code"
        autoFocus
      />
      <Button type="submit" disabled={busy || code.trim().length < 6}>
        {busy && <Spinner className="text-primary-foreground" />} Confirm
      </Button>
      <Button
        type="button"
        variant="ghost"
        disabled={busy}
        onClick={() => run(async () => setSent(await call('signing.request_code', { token })))}
      >
        Send a New Code
      </Button>
    </form>
  ) : (
    <div className="grid gap-3">
      <p>To {purpose === 'view' ? 'open' : 'sign'} this document, confirm it is you with a code sent by SMS.</p>
      <FormError message={error} />
      <Button disabled={busy} onClick={() => run(async () => setSent(await call('signing.request_code', { token })))}>
        {busy && <Spinner className="text-primary-foreground" />} Text Me a Code
      </Button>
    </div>
  );
}

export function SignPage() {
  const token = useSigningToken();
  const [session, setSession] = useState<string | null>(null);
  const [view, setView] = useState<SignerView>();
  const [error, setError] = useState<string>();
  const [values, setValues] = useState<Record<string, string>>({});
  const [signature, setSignature] = useState<Signature | null>(null);
  const [consent, setConsent] = useState(false);
  const [busy, setBusy] = useState(false);
  const [signError, setSignError] = useState<string>();
  const [result, setResult] = useState<'signed' | 'completed' | 'declined' | null>(null);
  const [declining, setDeclining] = useState(false);
  const [reason, setReason] = useState('');

  useEffect(() => {
    if (!token) return;
    call('signing.open', { token, session })
      .then(setView)
      .catch((e) => setError(errorMessage(e)));
  }, [token, session]);

  const locked = !!view?.needsAuthentication;
  const loader = useMemo(
    () => (view && !locked ? () => fetchPdf('/api/sign/file', { token, session }) : null),
    [view, locked, token, session],
  );
  const pdf = usePdf(loader, `${!!view}:${locked}:${session}`);
  const brand = safeColor(view?.document.brandColor);

  if (!token) return <Message icon={<XCircle className="size-10 text-destructive" />} title="This link is incomplete" />;
  if (error) {
    return (
      <Shell>
        <Message icon={<XCircle className="size-10 text-destructive" />} title="This link does not work">
          {error}
        </Message>
      </Shell>
    );
  }
  if (!view) {
    return (
      <Shell>
        <div className="flex justify-center pt-20">
          <Spinner className="size-6" />
        </div>
      </Shell>
    );
  }
  if (result) {
    return (
      <Shell view={view}>
        <Message
          icon={
            result === 'declined' ? (
              <XCircle className="size-10 text-muted-foreground" />
            ) : (
              <CheckCircle2 className="size-10 text-success" />
            )
          }
          title={result === 'declined' ? 'You declined' : view.party.role === 'approver' ? 'Approved' : 'Signed'}
        >
          {result === 'completed'
            ? 'Everyone has signed. The sealed document is on its way to you by email.'
            : result === 'signed'
              ? 'Thank you. You will receive the signed document when everyone has signed.'
              : 'The sender has been told. You can close this page.'}
        </Message>
      </Shell>
    );
  }
  if (view.needsAuthentication) {
    return (
      <Shell view={view}>
        <div className="mx-auto mt-6 grid max-w-md gap-4 rounded-lg border bg-background p-6">
          <h1 className="text-lg font-semibold">Hello {view.party.name}</h1>
          <Identify token={token} method={view.needsAuthentication} purpose="view" onSession={setSession} />
        </div>
      </Shell>
    );
  }

  const done = view.party.status === 'signed' || view.party.status === 'declined';
  const canAct = view.yourTurn && !done && view.party.role !== 'viewer' && view.document.status === 'pending';
  const own = view.fields.filter((f) => f.partyId === view.party.id);
  const inputs = own.filter((f) => ['text', 'initials', 'company', 'checkbox'].includes(f.kind));
  const isSigner = view.party.role === 'signer';

  const sign = async () => {
    setBusy(true);
    setSignError(undefined);
    try {
      const out = await call('signing.sign', {
        token,
        session,
        consent,
        signature: isSigner ? signature : null,
        values: Object.entries(values).map(([fieldId, value]) => ({ fieldId, value })),
      });
      setResult(out.completed ? 'completed' : 'signed');
    } catch (e) {
      setSignError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  const decline = async () => {
    setBusy(true);
    try {
      await call('signing.decline', { token, session, reason: reason.trim() || null });
      setResult('declined');
    } catch (e) {
      setSignError(errorMessage(e));
    } finally {
      setBusy(false);
      setDeclining(false);
    }
  };
  const downloadSigned = async () => {
    try {
      saveBytes(await fetchPdf('/api/sign/file', { token, session }), `${view.document.title}.pdf`);
    } catch (e) {
      setSignError(errorMessage(e));
    }
  };

  return (
    <Shell view={view}>
      <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_320px]">
        <div className="min-w-0">
          {view.document.message && (
            <div className="mb-4 rounded-lg border bg-background p-4 whitespace-pre-line">{view.document.message}</div>
          )}
          {pdf.pages ? (
            <PdfPages
              pages={pdf.pages}
              overlay={(page) => (
                <div className="pointer-events-none absolute inset-0">
                  {view.fields
                    .filter((f) => f.page === page)
                    .map((f) => {
                      const mine = f.partyId === view.party.id;
                      const style = {
                        left: `${f.x * 100}%`,
                        top: `${f.y * 100}%`,
                        width: `${f.width * 100}%`,
                        height: `${f.height * 100}%`,
                      };
                      if (!mine) return null;
                      if (canAct && f.kind === 'checkbox') {
                        return (
                          <label key={f.id} className="pointer-events-auto absolute flex items-center justify-center" style={style}>
                            <Checkbox
                              checked={values[f.id] === 'true'}
                              onCheckedChange={(c) => setValues({ ...values, [f.id]: c ? 'true' : 'false' })}
                              aria-label={f.label || 'Checkbox'}
                            />
                          </label>
                        );
                      }
                      if (canAct && ['text', 'initials', 'company'].includes(f.kind)) {
                        return (
                          <input
                            key={f.id}
                            aria-label={f.label || f.kind}
                            placeholder={f.label || (f.kind === 'initials' ? 'Initials' : f.kind === 'company' ? 'Company' : 'Text')}
                            value={values[f.id] ?? (f.kind === 'company' ? (view.party.company ?? '') : '')}
                            onChange={(e) => setValues({ ...values, [f.id]: e.target.value })}
                            className="pointer-events-auto absolute rounded-[3px] border border-primary bg-primary/10 px-1 text-[11px] text-zinc-900 outline-none focus:bg-white"
                            style={style}
                          />
                        );
                      }
                      return (
                        <div
                          key={f.id}
                          className={cn(
                            'absolute flex items-center rounded-[3px] border px-1 text-[10px] text-zinc-700',
                            canAct ? 'border-primary bg-primary/10' : 'border-transparent',
                          )}
                          style={style}
                        >
                          {canAct &&
                            (f.kind === 'signature'
                              ? 'Your signature'
                              : f.kind === 'name'
                                ? view.party.name
                                : f.kind === 'date'
                                  ? 'Date of signing'
                                  : f.kind)}
                        </div>
                      );
                    })}
                </div>
              )}
            />
          ) : (
            <PdfLoading error={pdf.error} />
          )}
        </div>
        <aside className="lg:sticky lg:top-6 lg:self-start">
          <div className="grid gap-4 rounded-lg border bg-background p-5">
            <div>
              <h2 className="font-semibold">{view.party.name}</h2>
              <p className="text-muted-foreground">
                {view.party.role === 'viewer'
                  ? 'You receive a copy of this document.'
                  : view.party.role === 'approver'
                    ? 'You are asked to approve this document.'
                    : 'You are asked to sign this document.'}
              </p>
            </div>
            {view.document.status === 'completed' && view.canDownload ? (
              <Button onClick={downloadSigned}>
                <Download /> Download Signed Document
              </Button>
            ) : done ? (
              <p>You have already {view.party.status === 'declined' ? 'declined' : 'responded'}.</p>
            ) : view.party.role === 'viewer' ? null : !view.yourTurn ? (
              <p className="text-muted-foreground">Others sign before you. We will email you when it is your turn.</p>
            ) : view.signAuthentication ? (
              <Identify token={token} method={view.signAuthentication} purpose="sign" onSession={setSession} />
            ) : (
              <>
                {inputs.length > 0 && <p className="text-xs text-muted-foreground">Fill in the highlighted fields in the document.</p>}
                {isSigner && <SignaturePad defaultName={view.party.name} onChange={setSignature} />}
                <label className="flex items-start gap-2.5">
                  <Checkbox checked={consent} onCheckedChange={(c) => setConsent(c === true)} className="mt-0.5" />
                  <span>
                    I have read the document and agree to {isSigner ? 'sign it' : 'approve it'} electronically. This is as binding as a
                    signature on paper.
                  </span>
                </label>
                <FormError message={signError} />
                <Button
                  onClick={sign}
                  disabled={busy || !consent || (isSigner && !signature)}
                  style={brand ? { backgroundColor: brand } : undefined}
                  size="lg"
                >
                  {busy ? <Spinner className="text-primary-foreground" /> : <FileSignature />} {isSigner ? 'Sign' : 'Approve'}
                </Button>
                <Button variant="ghost" onClick={() => setDeclining(true)} disabled={busy}>
                  Decline
                </Button>
              </>
            )}
            {!canAct && signError && <FormError message={signError} />}
          </div>
          <p className="mt-3 px-1 text-xs text-muted-foreground">
            Every step is recorded in a tamper-evident audit trail and the finished document is sealed, so anyone can verify it later.
          </p>
        </aside>
      </div>
      <Dialog open={declining} onOpenChange={setDeclining}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Decline to Sign?</DialogTitle>
            <DialogDescription>The document stops for everyone and the sender is told.</DialogDescription>
          </DialogHeader>
          <div className="grid gap-1.5">
            <Label htmlFor="reason">Reason (Optional)</Label>
            <Textarea id="reason" value={reason} onChange={(e) => setReason(e.target.value)} />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeclining(false)}>
              Back
            </Button>
            <Button variant="destructive" onClick={decline} disabled={busy}>
              Decline
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Shell>
  );
}
