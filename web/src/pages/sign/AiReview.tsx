import { Bot, Check, Copy, ExternalLink } from 'lucide-react';
import { useState } from 'react';
import { Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { type ActiveReviewLink, call, errorMessage, type ReviewLinkState } from '@/lib/api';
import { FormError } from '../auth/AuthLayout';
import { useTexts } from './texts';

const AGENTS = [
  { name: 'ChatGPT', url: 'https://chatgpt.com/?q=' },
  { name: 'Claude', url: 'https://claude.ai/new?q=' },
  { name: 'Copilot', url: 'https://copilot.microsoft.com/?q=' },
];

/** Lets the signer hand their own AI a read-only review link. The URL is shown once; only a hash is stored. */
export function AiReview({
  token,
  session,
  state,
  language,
}: {
  token: string;
  session: string | null;
  state: ReviewLinkState;
  language: string;
}) {
  const t = useTexts();
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState<ActiveReviewLink | null>(state.active ?? null);
  const [url, setUrl] = useState<string>();
  const [copied, setCopied] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const date = (value: string) => new Date(value).toLocaleDateString(language);

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
  const create = () =>
    run(async () => {
      const created = await call('signing.create_review_link', { token, session });
      setUrl(created.url);
      setCopied(false);
      setActive({ createdAt: new Date().toISOString(), expiresAt: created.expiresAt, uses: 0, lastUsedAt: null });
    });
  const revoke = () =>
    run(async () => {
      await call('signing.revoke_review_link', { token, session });
      setUrl(undefined);
      setActive(null);
    });
  const prompt = url ? t.reviewPrompt(url) : '';
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(prompt);
      setCopied(true);
    } catch {}
  };

  if (!state.available) return null;
  if (!open) {
    return (
      <Button variant="outline" onClick={() => setOpen(true)}>
        <Bot /> {t.reviewWithAi}
      </Button>
    );
  }
  return (
    <div className="grid gap-3 rounded-lg border bg-background p-5">
      <h2 className="flex items-center gap-2 font-semibold">
        <Bot className="size-4" /> {t.reviewWithAi}
      </h2>
      <p className="text-muted-foreground">{t.reviewIntro}</p>
      <FormError message={error} />
      {url ? (
        <>
          <textarea
            readOnly
            value={prompt}
            rows={6}
            aria-label={t.reviewWithAi}
            onFocus={(e) => e.currentTarget.select()}
            className="w-full resize-none rounded-md border bg-muted/40 p-2 text-xs"
          />
          <p className="text-xs text-muted-foreground">{t.reviewShownOnce}</p>
          <Button onClick={copy}>
            {copied ? <Check /> : <Copy />} {copied ? t.copied : t.copyPrompt}
          </Button>
          <div className="grid gap-2">
            {AGENTS.map((agent) => (
              <Button key={agent.name} variant="outline" asChild>
                <a href={agent.url + encodeURIComponent(prompt)} target="_blank" rel="noopener noreferrer">
                  <ExternalLink /> {t.openIn(agent.name)}
                </a>
              </Button>
            ))}
          </div>
        </>
      ) : active ? (
        <>
          <p>{t.reviewActive(date(active.expiresAt), active.uses)}</p>
          <Button onClick={create} disabled={busy}>
            {busy && <Spinner className="text-primary-foreground" />} {t.newReviewLink}
          </Button>
        </>
      ) : (
        <Button onClick={create} disabled={busy}>
          {busy && <Spinner className="text-primary-foreground" />} {t.createReviewLink}
        </Button>
      )}
      {active && (
        <>
          <p className="text-xs text-muted-foreground">{t.reviewSafety(date(active.expiresAt))}</p>
          <Button variant="ghost" onClick={revoke} disabled={busy}>
            {t.revokeLink}
          </Button>
        </>
      )}
    </div>
  );
}
