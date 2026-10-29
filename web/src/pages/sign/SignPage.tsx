import { CheckCircle2, Download, FileSignature, HandHelping, Languages, ShieldCheck, XCircle } from 'lucide-react';
import { Direction } from 'radix-ui';
import { type FormEvent, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import { BrandLogo, Spinner, safeColor } from '@/components/common';
import { PdfLoading, PdfPages, usePdf } from '@/components/PdfPages';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Textarea } from '@/components/ui/textarea';
import { ApiError, type AttachmentRequest, type AuthMethod, call, errorMessage, fetchPdf, type SignerView } from '@/lib/api';
import { display } from '@/lib/field-values';
import { saveBytes } from '@/lib/format';
import { cn } from '@/lib/utils';
import { FormError } from '../auth/AuthLayout';
import { AiReview } from './AiReview';
import { AttachmentUploads } from './AttachmentUploads';
import { EidIdentify } from './EidIdentify';
import { framed, notifyHost, redirectAfter } from './embed';
import { type Signature, SignaturePad } from './SignaturePad';
import { INPUT_KINDS, initialValues, outcomes, problems, SignerField, type Upload } from './SignerFields';
import { isRtl, LanguageContext, languageFor, SIGNER_LANGUAGES, TextsContext, textsFor, useTexts } from './texts';
import { WebcamPhoto } from './WebcamPhoto';

const REDIRECT_KEY = 'opensign-sign-redirect';

/**
 * The page behind a signing link. The token lives in the URL fragment, so it never reaches a server log or a Referer
 * header; we read it once and take it out of the address bar, with the `redirect` an embedding site may have added.
 */
function useSigningLink() {
  const [link] = useState(() => {
    const value = decodeURIComponent(window.location.hash.slice(1));
    if (value) {
      const redirect = new URLSearchParams(window.location.search).get('redirect');
      try {
        sessionStorage.setItem('opensign-sign', value);
        if (redirect) sessionStorage.setItem(REDIRECT_KEY, redirect);
        else sessionStorage.removeItem(REDIRECT_KEY);
      } catch {}
      history.replaceState(null, '', window.location.pathname);
      return { token: value, redirect };
    }
    try {
      return { token: sessionStorage.getItem('opensign-sign') ?? '', redirect: sessionStorage.getItem(REDIRECT_KEY) };
    } catch {
      return { token: '', redirect: null };
    }
  });
  return link;
}

/** The eID broker sends the browser back with `?eid=<id>`; we take it once and clean the address bar. */
function useEidResume() {
  const [resume] = useState(() => {
    const value = new URLSearchParams(window.location.search).get('eid') ?? undefined;
    if (value) history.replaceState(null, '', window.location.pathname);
    return value;
  });
  return resume;
}

const SESSION_KEY = 'opensign-sign-session';

/** The signer session survives the round trip to an eID broker, for this tab and this link only. */
function useSignerSession(token: string) {
  const [session, setSession] = useState<string | null>(() => {
    try {
      const saved = JSON.parse(sessionStorage.getItem(SESSION_KEY) ?? 'null') as { token: string; session: string } | null;
      return saved?.token === token ? saved.session : null;
    } catch {
      return null;
    }
  });
  const update = (value: string) => {
    setSession(value);
    try {
      sessionStorage.setItem(SESSION_KEY, JSON.stringify({ token, session: value }));
    } catch {}
  };
  return [session, update] as const;
}

const LANGUAGE_KEY = 'opensign-sign-language';

/** A language the signer picked on this page, for this tab and this link only. */
function useChosenLanguage(token: string) {
  const [chosen, setChosen] = useState<string | null>(() => {
    try {
      const saved = JSON.parse(sessionStorage.getItem(LANGUAGE_KEY) ?? 'null') as { token: string; code: string } | null;
      return saved?.token === token ? saved.code : null;
    } catch {
      return null;
    }
  });
  const choose = useCallback(
    (code: string) => {
      setChosen(code);
      try {
        sessionStorage.setItem(LANGUAGE_KEY, JSON.stringify({ token, code }));
      } catch {}
    },
    [token],
  );
  return [chosen, choose] as const;
}

function LanguagePicker() {
  const t = useTexts();
  const { code, choose } = useContext(LanguageContext);
  return (
    <Select value={code} onValueChange={choose}>
      <SelectTrigger size="sm" aria-label={t.language}>
        <Languages />
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {SIGNER_LANGUAGES.map(([c, name]) => (
          <SelectItem key={c} value={c} lang={c} dir={isRtl(c) ? 'rtl' : 'ltr'}>
            {name}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

function Shell({ view, children }: { view?: SignerView; children: React.ReactNode }) {
  const t = useTexts();
  const { code } = useContext(LanguageContext);
  const dir = isRtl(code) ? 'rtl' : 'ltr';
  useEffect(() => {
    document.documentElement.lang = code;
    document.documentElement.dir = dir;
  }, [code, dir]);
  return (
    <Direction.Provider dir={dir}>
      <div className="min-h-svh bg-muted/40" lang={code} dir={dir}>
        <header className="border-b bg-background">
          <div className="mx-auto flex h-14 max-w-5xl items-center gap-3 px-4">
            <BrandLogo className="h-6" markClassName="size-6" branding={view?.document} />
            <div className="min-w-0 flex-1">
              <div className="truncate text-sm font-semibold">{view?.document.title ?? 'OpenSign'}</div>
              {view && (
                <div className="truncate text-xs text-muted-foreground">{t.from(view.document.sender, view.document.organisation)}</div>
              )}
            </div>
            <LanguagePicker />
            <ShieldCheck className="size-4 shrink-0 text-success" aria-label={t.secureConnection} />
          </div>
        </header>
        <main className="mx-auto max-w-5xl px-4 py-6">{children}</main>
        {framed && (
          <footer className="flex items-center justify-center gap-1.5 pb-4 text-xs text-muted-foreground">
            <ShieldCheck className="size-3.5" /> {t.securedBy}
          </footer>
        )}
      </div>
    </Direction.Provider>
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

/** Proves who the party is before they may see or sign: an SMS code, or an eID. */
function Identify({
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
  const t = useTexts();
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

  if (method === 'se_bankid' || method === 'freja' || method === 'broker') {
    return <EidIdentify token={token} session={session} method={method} purpose={purpose} resume={resume} onSession={onSession} />;
  }
  if (method !== 'sms_pin') {
    return <FormError message={t.methodUnavailable(t.methods[method] ?? method)} />;
  }
  return sent ? (
    <form onSubmit={verify} className="grid gap-3">
      <p>{t.codeSent(sent.phoneHint)}</p>
      <FormError message={error} />
      <Input
        value={code}
        onChange={(e) => setCode(e.target.value)}
        inputMode="numeric"
        autoComplete="one-time-code"
        className="font-mono text-lg tracking-widest"
        aria-label={t.code}
        autoFocus
      />
      <Button type="submit" disabled={busy || code.trim().length < 6}>
        {busy && <Spinner className="text-primary-foreground" />} {t.confirm}
      </Button>
      <Button
        type="button"
        variant="ghost"
        disabled={busy}
        onClick={() => run(async () => setSent(await call('signing.request_code', { token })))}
      >
        {t.newCode}
      </Button>
    </form>
  ) : (
    <div className="grid gap-3">
      <p>{t.smsIntro(purpose === 'view')}</p>
      <FormError message={error} />
      <Button disabled={busy} onClick={() => run(async () => setSent(await call('signing.request_code', { token })))}>
        {busy && <Spinner className="text-primary-foreground" />} {t.textMe}
      </Button>
    </div>
  );
}

export function SignPage() {
  const { token, redirect } = useSigningLink();
  const [language, setLanguage] = useState<string>();
  const [chosen, choose] = useChosenLanguage(token);
  const code = languageFor(chosen ?? language);
  const choice = useMemo(() => ({ code, choose }), [code, choose]);
  return (
    <LanguageContext.Provider value={choice}>
      <TextsContext.Provider value={textsFor(code)}>
        <SigningFlow token={token} redirect={redirect} onLanguage={setLanguage} />
      </TextsContext.Provider>
    </LanguageContext.Provider>
  );
}

function SigningFlow({ token, redirect, onLanguage }: { token: string; redirect: string | null; onLanguage: (language: string) => void }) {
  const t = useTexts();
  const resume = useEidResume();
  const [session, setSession] = useSignerSession(token);
  const [view, setView] = useState<SignerView>();
  const [error, setError] = useState<string>();
  const [result, setResult] = useState<'signed' | 'completed' | 'declined' | null>(null);
  const [handedOver, setHandedOver] = useState(false);

  useEffect(() => {
    if (!token) return;
    call('signing.open', { token, session, redirectUrl: redirect })
      .then((v) => {
        setView(v);
        onLanguage(v.document.language);
      })
      .catch((e) => setError(errorMessage(e)));
  }, [token, session, redirect, onLanguage]);

  const origins = view?.embed?.origins;
  const documentId = view?.document.id;
  const partyId = view?.party.id;
  useEffect(() => {
    if (documentId && partyId) notifyHost(origins, 'loaded', { documentId, partyId });
  }, [origins, documentId, partyId]);
  useEffect(() => {
    if (!token && resume) window.close();
  }, [token, resume]);

  const finish = (outcome: 'signed' | 'completed' | 'declined') => {
    setResult(outcome);
    if (!view) return;
    const ids = { documentId: view.document.id, partyId: view.party.id };
    if (outcome === 'declined') notifyHost(view.embed?.origins, 'declined', ids);
    else notifyHost(view.embed?.origins, 'signed', { ...ids, completed: outcome === 'completed' });
    if (view.embed?.redirectUrl) redirectAfter(view.embed.redirectUrl, outcome === 'declined');
  };
  const failed = (message: string) => {
    if (view) notifyHost(view.embed?.origins, 'error', { documentId: view.document.id, partyId: view.party.id, message });
  };

  const locked = !!view?.needsAuthentication;
  const loader = useMemo(
    () => (view && !locked ? () => fetchPdf('/api/sign/file', { token, session }) : null),
    [view, locked, token, session],
  );
  const pdf = usePdf(loader, `${!!view}:${locked}:${session}`);
  if (!token && resume) {
    return (
      <Message icon={<CheckCircle2 className="size-10 text-success" />} title={t.eidWindowDone}>
        {t.eidWindowDoneNote}
      </Message>
    );
  }
  if (!token) return <Message icon={<XCircle className="size-10 text-destructive" />} title={t.linkIncomplete} />;
  if (error) {
    return (
      <Shell>
        <Message icon={<XCircle className="size-10 text-destructive" />} title={t.linkBroken}>
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
  if (framed && !view.embed) {
    return (
      <Shell>
        <Message icon={<XCircle className="size-10 text-destructive" />} title={t.embedRefused}>
          {t.embedRefusedNote}
        </Message>
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
          title={result === 'declined' ? t.youDeclined : view.party.role === 'approver' ? t.approved : t.signed}
        >
          {result === 'completed'
            ? view.party.email
              ? t.completedNote
              : t.completedNoEmail
            : result === 'signed'
              ? t.signedNote
              : t.declinedNote}
          {view.party.inPerson && (
            <div className="mt-4 grid justify-items-center gap-3">
              <p className="font-medium text-foreground">{t.handBack(view.document.sender ?? view.document.organisation)}</p>
              <Button variant="outline" onClick={() => window.location.assign(`/documents/${view.document.id}`)}>
                {t.backToDocument}
              </Button>
            </div>
          )}
        </Message>
      </Shell>
    );
  }
  if (view.party.inPerson && !handedOver && view.yourTurn) {
    return (
      <Shell view={view}>
        <Message icon={<HandHelping className="size-10 text-muted-foreground" />} title={t.handOver(view.party.name)}>
          {t.handOverNote(view.document.sender ?? view.document.organisation)}
          <div className="mt-4">
            <Button onClick={() => setHandedOver(true)}>{t.iAm(view.party.name)}</Button>
          </div>
        </Message>
      </Shell>
    );
  }
  if (view.needsAuthentication) {
    return (
      <Shell view={view}>
        <div className="mx-auto mt-6 grid max-w-md gap-4 rounded-lg border bg-background p-6">
          <h1 className="text-lg font-semibold">{t.hello(view.party.name)}</h1>
          {resume === 'invalid' && <FormError message={t.eidFailed} />}
          <Identify
            token={token}
            session={session}
            method={view.needsAuthentication}
            purpose="view"
            resume={resume !== 'invalid' ? resume : undefined}
            onSession={setSession}
          />
        </div>
      </Shell>
    );
  }

  return (
    <Shell view={view}>
      <SignerScreen
        view={view}
        pdf={pdf}
        token={token}
        session={session}
        resume={resume}
        onSession={setSession}
        onResult={finish}
        onError={failed}
      />
    </Shell>
  );
}

type Pdf = ReturnType<typeof usePdf>;

/** The document and the signing panel. In a preview everything is shown but nothing reaches the server. */
function SignerScreen({
  view,
  pdf,
  preview,
  token = '',
  session = null,
  resume,
  onSession = () => {},
  onResult = () => {},
  onError = () => {},
}: {
  view: SignerView;
  pdf: Pdf;
  preview?: boolean;
  token?: string;
  session?: string | null;
  resume?: string;
  onSession?: (session: string) => void;
  onResult?: (result: 'signed' | 'completed' | 'declined') => void;
  onError?: (message: string) => void;
}) {
  const t = useTexts();
  const own = view.fields.filter((f) => f.partyId === view.party.id);
  const [initial] = useState(() => initialValues(own, view.party.company));
  const [values, setValues] = useState<Record<string, string>>(initial.values);
  const [uploads, setUploads] = useState<Record<string, Upload>>(initial.uploads);
  const [touched, setTouched] = useState<Set<string>>(new Set());
  const [signature, setSignature] = useState<Signature | null>(null);
  const [consent, setConsent] = useState(false);
  const [busy, setBusy] = useState(false);
  const [signError, setSignError] = useState<string>();
  const [declining, setDeclining] = useState(false);
  const [reason, setReason] = useState('');
  const [attachments, setAttachments] = useState<AttachmentRequest[]>(view.attachments);
  const brand = safeColor(view.document.brandColor);

  const done = view.party.status === 'signed' || view.party.status === 'declined';
  const canAct = view.party.role !== 'viewer' && (preview || (view.yourTurn && !done && view.document.status === 'pending'));
  const logic = useMemo(() => outcomes(view.fields, view.party, values, uploads), [view.fields, view.party, values, uploads]);
  const shown = (f: { id: string }) => logic.get(f.id)?.visible !== false;
  const inputs = own.filter((f) => INPUT_KINDS.includes(f.kind) && !f.readOnly && shown(f));
  const issues = problems(own, values, uploads, t, shown);
  const touch = (id: string) => setTouched((s) => (s.has(id) ? s : new Set(s).add(id)));
  const isSigner = view.party.role === 'signer';

  const sign = async () => {
    const first = own.find((f) => issues[f.id]);
    if (first) {
      setTouched(new Set(Object.keys(issues)));
      setSignError(issues[first.id]);
      document.getElementById(`field-${first.id}`)?.scrollIntoView({ behavior: 'smooth', block: 'center' });
      return;
    }
    setBusy(true);
    setSignError(undefined);
    try {
      const out = await call('signing.sign', {
        token,
        session,
        consent,
        signature: isSigner ? signature : null,
        values: Object.entries(values)
          .filter(([id]) => own.some((f) => f.id === id))
          .map(([fieldId, value]) => ({ fieldId, value })),
      });
      onResult(out.completed ? 'completed' : 'signed');
    } catch (e) {
      setSignError(errorMessage(e));
      onError(errorMessage(e));
      if (e instanceof ApiError && e.status === 401) onSession('');
    } finally {
      setBusy(false);
    }
  };
  const decline = async () => {
    setBusy(true);
    try {
      await call('signing.decline', { token, session, reason: reason.trim() || null });
      onResult('declined');
    } catch (e) {
      setSignError(errorMessage(e));
      onError(errorMessage(e));
      if (e instanceof ApiError && e.status === 401) onSession('');
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
    <>
      <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_320px]">
        <div className="min-w-0">
          {view.document.message && (
            <div dir="auto" className="mb-4 rounded-lg border bg-background p-4 whitespace-pre-line">
              {view.document.message}
            </div>
          )}
          <div dir="ltr">
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
                        if (!mine || !shown(f)) return null;
                        if (f.kind === 'formula') {
                          const value = logic.get(f.id)?.value;
                          return (
                            <div
                              key={f.id}
                              id={`field-${f.id}`}
                              role="status"
                              aria-label={f.label || undefined}
                              className="absolute flex items-center rounded-[3px] border border-dashed border-zinc-400 px-1 text-[11px] text-zinc-800"
                              style={style}
                            >
                              <span dir="ltr" className="truncate">
                                {value != null ? display('formula', f.format, value) : '—'}
                              </span>
                            </div>
                          );
                        }
                        if (canAct && INPUT_KINDS.includes(f.kind)) {
                          return (
                            <SignerField
                              key={f.id}
                              field={f}
                              fields={own}
                              style={style}
                              values={values}
                              onValues={setValues}
                              uploads={uploads}
                              onUpload={(id, upload) =>
                                setUploads(({ [id]: old, ...rest }) => {
                                  if (old?.preview) URL.revokeObjectURL(old.preview);
                                  return upload ? { ...rest, [id]: upload } : rest;
                                })
                              }
                              problem={issues[f.id]}
                              showProblem={touched.has(f.id)}
                              onTouch={touch}
                              token={token}
                              session={session}
                              preview={preview}
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
                                ? t.yourSignature
                                : f.kind === 'name'
                                  ? view.party.name
                                  : f.kind === 'date'
                                    ? t.dateOfSigning
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
        </div>
        <aside className="lg:sticky lg:top-6 lg:self-start">
          <div className="grid gap-4 rounded-lg border bg-background p-5">
            <div>
              <h2 className="font-semibold">{view.party.name}</h2>
              <p className="text-muted-foreground">
                {view.party.role === 'viewer' ? t.roleViewer : view.party.role === 'approver' ? t.roleApprover : t.roleSigner}
              </p>
            </div>
            {preview ? (
              <>
                {view.signAuthentication && <IdentifyPreview method={view.signAuthentication} />}
                {inputs.length > 0 && <p className="text-xs text-muted-foreground">{t.fillFields}</p>}
                {attachments.length > 0 && (
                  <AttachmentUploads token="" session={null} requests={attachments} onChange={() => {}} disabled />
                )}
                {isSigner && <SignaturePad defaultName={view.party.name} onChange={setSignature} />}
                <label className="flex items-start gap-2.5">
                  <Checkbox checked={consent} onCheckedChange={(c) => setConsent(c === true)} className="mt-0.5" />
                  <span>{t.consent(isSigner)}</span>
                </label>
                <Button disabled style={brand ? { backgroundColor: brand } : undefined} size="lg">
                  <FileSignature /> {isSigner ? t.sign : t.approve}
                </Button>
                <Button variant="ghost" disabled>
                  {t.decline}
                </Button>
              </>
            ) : view.document.status === 'completed' && view.canDownload ? (
              <Button onClick={downloadSigned}>
                <Download /> {t.downloadSigned}
              </Button>
            ) : done ? (
              <p>{view.party.status === 'declined' ? t.alreadyDeclined : t.alreadyResponded}</p>
            ) : view.party.role === 'viewer' ? null : !view.yourTurn ? (
              <p className="text-muted-foreground">{t.notYourTurn}</p>
            ) : view.signAuthentication ? (
              <Identify
                token={token}
                session={session}
                method={view.signAuthentication}
                purpose="sign"
                resume={resume !== 'invalid' ? resume : undefined}
                onSession={onSession}
              />
            ) : (
              <>
                {inputs.length > 0 && <p className="text-xs text-muted-foreground">{t.fillFields}</p>}
                {attachments
                  .filter((a) => a.kind === 'photo')
                  .map((a) => (
                    <WebcamPhoto
                      key={a.id}
                      token={token}
                      session={session}
                      request={a}
                      onChange={(updated) => setAttachments(attachments.map((r) => (r.id === updated.id ? updated : r)))}
                    />
                  ))}
                {attachments.some((a) => a.kind !== 'photo') && (
                  <AttachmentUploads
                    token={token}
                    session={session}
                    requests={attachments.filter((a) => a.kind !== 'photo')}
                    onChange={(files) => setAttachments([...attachments.filter((a) => a.kind === 'photo'), ...files])}
                  />
                )}
                {isSigner && <SignaturePad defaultName={view.party.name} onChange={setSignature} />}
                <label className="flex items-start gap-2.5">
                  <Checkbox checked={consent} onCheckedChange={(c) => setConsent(c === true)} className="mt-0.5" />
                  <span>{t.consent(isSigner)}</span>
                </label>
                <FormError message={signError} />
                <Button
                  onClick={sign}
                  disabled={busy || !consent || (isSigner && !signature) || attachments.some((a) => a.required && !a.fileId)}
                  style={brand ? { backgroundColor: brand } : undefined}
                  size="lg"
                >
                  {busy ? <Spinner className="text-primary-foreground" /> : <FileSignature />} {isSigner ? t.sign : t.approve}
                </Button>
                <Button variant="ghost" onClick={() => setDeclining(true)} disabled={busy}>
                  {t.decline}
                </Button>
              </>
            )}
            {!canAct && signError && <FormError message={signError} />}
          </div>
          {view.review && view.document.status === 'pending' && (
            <div className="mt-4 grid">
              <AiReview token={token} session={session} state={view.review} language={view.document.language} />
            </div>
          )}
          <p className="mt-3 px-1 text-xs text-muted-foreground">{t.auditNote}</p>
        </aside>
      </div>
      <Dialog open={declining} onOpenChange={setDeclining}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t.declineTitle}</DialogTitle>
            <DialogDescription>{t.declineDescription}</DialogDescription>
          </DialogHeader>
          <div className="grid gap-1.5">
            <Label htmlFor="reason">{t.reason}</Label>
            <Textarea id="reason" value={reason} onChange={(e) => setReason(e.target.value)} />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeclining(false)}>
              {t.back}
            </Button>
            <Button variant="destructive" onClick={decline} disabled={busy}>
              {t.decline}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

/** What the party is asked to do before signing, as they will see it, with nothing that can be pressed. */
function IdentifyPreview({ method }: { method: AuthMethod }) {
  const t = useTexts();
  const label = t.methods[method] ?? method;
  return (
    <div className="grid gap-3">
      <p>{method === 'sms_pin' ? t.smsIntro(false) : t.eidIntro(false, label)}</p>
      <Button disabled>{method === 'sms_pin' ? t.textMe : t.identifyWith(label)}</Button>
    </div>
  );
}

/** The signing page as a party will see it, for the sender. Read-only: it never calls a signing service. */
export function SigningPreview({ documentId, partyId }: { documentId: string; partyId: string }) {
  const [view, setView] = useState<SignerView>();
  const [error, setError] = useState<string>();
  useEffect(() => {
    setView(undefined);
    setError(undefined);
    call('documents.signing_preview', { documentId, partyId })
      .then(setView)
      .catch((e) => setError(errorMessage(e)));
  }, [documentId, partyId]);
  const loader = useMemo(() => () => fetchPdf(`/api/documents/${documentId}/preview`), [documentId]);
  const pdf = usePdf(loader, documentId);

  return (
    <TextsContext.Provider value={textsFor(view?.document.language)}>
      <Shell view={view}>
        {error ? (
          <Message icon={<XCircle className="size-10 text-destructive" />} title="The preview could not be shown">
            {error}
          </Message>
        ) : view ? (
          <>
            {view.needsAuthentication && <OpenGatePreview method={view.needsAuthentication} />}
            <SignerScreen key={view.party.id} view={view} pdf={pdf} preview />
          </>
        ) : (
          <div className="flex justify-center pt-20">
            <Spinner className="size-6" />
          </div>
        )}
      </Shell>
    </TextsContext.Provider>
  );
}

function OpenGatePreview({ method }: { method: AuthMethod }) {
  const t = useTexts();
  return (
    <div className="mb-4 flex items-center gap-2 rounded-lg border border-dashed bg-background p-3 text-muted-foreground">
      <ShieldCheck className="size-4 shrink-0" />
      {method === 'sms_pin' ? t.smsIntro(true) : t.eidIntro(true, t.methods[method] ?? method)}
    </div>
  );
}
