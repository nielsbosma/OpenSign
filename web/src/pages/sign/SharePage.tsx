import { CheckCircle2, XCircle } from 'lucide-react';
import { type FormEvent, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import { useParams } from 'react-router';
import { Spinner, safeColor } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { call, errorMessage, type PublicShareLink } from '@/lib/api';
import { useServerTextLanguage } from '@/lib/i18n';
import { FormError } from '../auth/AuthLayout';
import { Message, Shell } from './SignPage';
import { LanguageContext, languageFor, TextsContext, textsFor, useTexts } from './texts';

/** The public page behind a template's share link: the visitor confirms their email, then signs a new document. */
export function SharePage() {
  const { slug = '' } = useParams();
  const [language, setLanguage] = useState<string>();
  const [chosen, setChosen] = useState<string | null>(null);
  const code = languageFor(chosen ?? language);
  useServerTextLanguage(code);
  const choose = useCallback((c: string) => setChosen(c), []);
  const choice = useMemo(() => ({ code, choose }), [code, choose]);
  return (
    <LanguageContext.Provider value={choice}>
      <TextsContext.Provider value={textsFor(code)}>
        <ShareFlow slug={slug} onLanguage={setLanguage} />
      </TextsContext.Provider>
    </LanguageContext.Provider>
  );
}

function ShareFlow({ slug, onLanguage }: { slug: string; onLanguage: (language: string) => void }) {
  const t = useTexts();
  const { code: language } = useContext(LanguageContext);
  const [link, setLink] = useState<PublicShareLink>();
  const [error, setError] = useState<string>();
  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [phone, setPhone] = useState('');
  const [requestId, setRequestId] = useState<string | null>(null);
  const [code, setCode] = useState('');
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState<string>();
  const [waiting, setWaiting] = useState(false);

  useEffect(() => {
    call('share_links.open', { slug })
      .then((l) => {
        setLink(l);
        onLanguage(l.language);
      })
      .catch((e) => setError(errorMessage(e)));
  }, [slug, onLanguage]);

  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    setFormError(undefined);
    try {
      await fn();
    } catch (e) {
      setFormError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  const request = (e: FormEvent) => {
    e.preventDefault();
    void run(async () => {
      const out = await call('share_links.request_code', {
        slug,
        name: name.trim(),
        email: email.trim(),
        phone: phone.trim() || null,
        language,
      });
      setCode('');
      setRequestId(out.requestId);
    });
  };
  const confirm = (e: FormEvent) => {
    e.preventDefault();
    if (!requestId) return;
    void run(async () => {
      const out = await call('share_links.confirm', { slug, requestId, code: code.trim() });
      if (out.yourTurn) window.location.assign(out.url);
      else setWaiting(true);
    });
  };

  if (error) {
    return (
      <Shell>
        <Message icon={<XCircle className="size-10 text-destructive" />} title={t.linkBroken}>
          {error}
        </Message>
      </Shell>
    );
  }
  if (!link) {
    return (
      <Shell>
        <div className="flex justify-center pt-20">
          <Spinner className="size-6" />
        </div>
      </Shell>
    );
  }
  if (waiting) {
    return (
      <Shell doc={link}>
        <Message icon={<CheckCircle2 className="size-10 text-success" />} title={t.shareReady}>
          {t.notYourTurn}
        </Message>
      </Shell>
    );
  }
  const brand = safeColor(link.brandColor);
  const sign = link.role !== 'approver';
  return (
    <Shell doc={link}>
      <div className="mx-auto mt-6 grid max-w-md gap-4 rounded-lg border bg-background p-6">
        <div className="grid gap-1">
          <h1 className="text-lg font-semibold" dir="auto">
            {link.title}
          </h1>
          <p className="text-muted-foreground">{t.shareIntro(sign, link.organisation)}</p>
        </div>
        {link.message && (
          <div dir="auto" className="rounded-md bg-muted/50 p-3 whitespace-pre-line">
            {link.message}
          </div>
        )}
        {requestId ? (
          <form onSubmit={confirm} className="grid gap-3">
            <p>{t.shareCodeSent(email.trim())}</p>
            <FormError message={formError} />
            <Input
              value={code}
              onChange={(e) => setCode(e.target.value)}
              inputMode="numeric"
              autoComplete="one-time-code"
              className="font-mono text-lg tracking-widest"
              aria-label={t.code}
              autoFocus
            />
            <Button
              type="submit"
              disabled={busy || code.replace(/\D/g, '').length < 6}
              style={brand ? { backgroundColor: brand } : undefined}
            >
              {busy && <Spinner className="text-primary-foreground" />} {t.shareOpen(sign)}
            </Button>
            <Button type="button" variant="ghost" disabled={busy} onClick={() => setRequestId(null)}>
              {t.shareOtherEmail}
            </Button>
          </form>
        ) : (
          <form onSubmit={request} className="grid gap-3">
            <FormError message={formError} />
            <div className="grid gap-1.5">
              <Label htmlFor="share-name">{t.yourName}</Label>
              <Input id="share-name" value={name} onChange={(e) => setName(e.target.value)} autoComplete="name" dir="auto" required />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="share-email">{t.shareEmail}</Label>
              <Input
                id="share-email"
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                autoComplete="email"
                dir="ltr"
                required
              />
            </div>
            {link.needsPhone && (
              <div className="grid gap-1.5">
                <Label htmlFor="share-phone">{t.phoneField}</Label>
                <Input
                  id="share-phone"
                  type="tel"
                  value={phone}
                  onChange={(e) => setPhone(e.target.value)}
                  autoComplete="tel"
                  placeholder="+46 70 123 45 67"
                  dir="ltr"
                  required
                />
              </div>
            )}
            <p className="text-xs text-muted-foreground">{t.shareEmailNote}</p>
            <Button type="submit" disabled={busy || !name.trim() || !email.trim()} style={brand ? { backgroundColor: brand } : undefined}>
              {busy && <Spinner className="text-primary-foreground" />} {t.shareContinue}
            </Button>
          </form>
        )}
      </div>
    </Shell>
  );
}
