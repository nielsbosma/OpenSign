import { CheckCircle2, XCircle } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { call, errorMessage } from '@/lib/api';
import { FormError } from '../auth/AuthLayout';
import { Message, Shell } from './SignPage';
import { LanguageContext, languageFor, TextsContext, textsFor, useTexts } from './texts';

type Pending = { title: string; organisation: string; method: string };

/**
 * A signing page in a frame cannot leave a cookie, so its eID identification starts here, on our own origin in a window
 * of its own. The person says go first: this page may have reached them through someone else.
 */
export function EidContinuePage() {
  const [language, setLanguage] = useState<string>();
  const [chosen, setChosen] = useState<string | null>(null);
  const code = languageFor(chosen ?? language);
  const choose = useCallback((c: string) => setChosen(c), []);
  const choice = useMemo(() => ({ code, choose }), [code, choose]);
  return (
    <LanguageContext.Provider value={choice}>
      <TextsContext.Provider value={textsFor(code)}>
        <Flow onLanguage={setLanguage} />
      </TextsContext.Provider>
    </LanguageContext.Provider>
  );
}

function Flow({ onLanguage }: { onLanguage: (language: string) => void }) {
  const t = useTexts();
  const [hash] = useState(() => {
    const value = decodeURIComponent(window.location.hash.slice(1));
    if (value) history.replaceState(null, '', window.location.pathname);
    return value;
  });
  const releaseCode = hash.startsWith('done=') ? hash.slice('done='.length) : undefined;
  const ticket = releaseCode ? '' : hash;
  const [pending, setPending] = useState<Pending>();
  const [error, setError] = useState<string>();
  const [formError, setFormError] = useState<string>();
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!ticket) return;
    call('signing.eid_continue', { ticket, confirm: false })
      .then((out) => {
        setPending(out);
        onLanguage(out.language);
      })
      .catch((e) => setError(errorMessage(e)));
  }, [ticket, onLanguage]);

  const go = async () => {
    setBusy(true);
    setFormError(undefined);
    try {
      const out = await call('signing.eid_continue', { ticket, confirm: true });
      if (out.redirectUrl) window.location.assign(out.redirectUrl);
    } catch (e) {
      setFormError(errorMessage(e));
      setBusy(false);
    }
  };

  if (releaseCode) {
    return (
      <Shell>
        <Message icon={<CheckCircle2 className="size-10 text-success" />} title={t.eidCodeTitle}>
          <span className="my-3 block font-mono text-3xl tracking-widest" dir="ltr">
            {releaseCode}
          </span>
          {t.eidCodeNote}
        </Message>
      </Shell>
    );
  }
  if (!ticket || error) {
    return (
      <Shell>
        <Message icon={<XCircle className="size-10 text-destructive" />} title={t.linkIncomplete}>
          {error}
        </Message>
      </Shell>
    );
  }
  if (!pending) {
    return (
      <Shell>
        <div className="flex justify-center pt-20">
          <Spinner className="size-6" />
        </div>
      </Shell>
    );
  }
  const label = t.methods[pending.method] ?? pending.method;
  return (
    <Shell>
      <div className="mx-auto mt-6 grid max-w-md gap-4 rounded-lg border bg-background p-6">
        <p dir="auto">{t.eidContinue(label, pending.title, pending.organisation)}</p>
        <p className="text-sm text-muted-foreground">{t.eidContinueNote}</p>
        <FormError message={formError} />
        <Button disabled={busy} onClick={go}>
          {busy && <Spinner className="text-primary-foreground" />} {t.identifyWith(label)}
        </Button>
      </div>
    </Shell>
  );
}
