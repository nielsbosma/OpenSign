import { useQueryClient } from '@tanstack/react-query';
import { ArrowUp, Check, Sparkles, X } from 'lucide-react';
import { type FormEvent, useEffect, useRef, useState } from 'react';
import Markdown from 'react-markdown';
import { useLocation } from 'react-router';
import remarkGfm from 'remark-gfm';
import { toast } from 'sonner';
import { Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { type ChatMessage, call, errorMessage, type Output, type ServiceName, useApiMutation } from '@/lib/api';
import { msg, t } from '@/lib/i18n';
import { cn } from '@/lib/utils';

type Proposal = Output<'ai.chat'>['proposals'][number];
type Turn = ChatMessage & { actions?: { tool: string; ok: boolean }[]; proposals?: Proposal[] };
type Answer = 'busy' | 'done' | 'dismissed';

/** The server's summaries of what the assistant proposes, so they can be translated. */
export const PROPOSAL_SUMMARIES = [
  msg('Change your language'),
  msg('Cancel the rest of this bulk send'),
  msg('Archive or restore this document'),
  msg('Cancel this document and its signing links'),
  msg('Create a new draft'),
  msg('Delete this document permanently'),
  msg('Move this document to another folder'),
  msg('Send a reminder to those who have not signed'),
  msg('Remove a file from this draft'),
  msg('Send this document for signing'),
  msg('Change the fields on this draft'),
  msg('Change who signs this draft'),
  msg('Change the details of this draft'),
  msg('Create a folder'),
  msg('Delete a folder'),
  msg('Change a folder'),
  msg('Restore an earlier version of this template'),
  msg('Save a new version of this template'),
];

const SUGGESTIONS = [
  msg('Which documents are waiting for signatures?'),
  msg('Remind everyone who has not signed this week'),
  msg('Create a draft NDA for Bo Berg'),
];

/** The optional assistant. It acts as you, with your permissions, through the same services as the app. */
export function AssistantPanel({ onClose }: { onClose: () => void }) {
  const [turns, setTurns] = useState<Turn[]>([]);
  const [text, setText] = useState('');
  const [answers, setAnswers] = useState<Record<string, Answer>>({});
  const location = useLocation();
  const queryClient = useQueryClient();
  const end = useRef<HTMLDivElement>(null);
  const chat = useApiMutation('ai.chat', {
    onSuccess: async (output) => {
      setTurns((prev) => [...prev, { role: 'assistant', content: output.reply, actions: output.actions, proposals: output.proposals }]);
      if (output.actions.some((a) => a.ok)) await queryClient.invalidateQueries();
    },
  });

  // biome-ignore lint/correctness/useExhaustiveDependencies: scroll when a message arrives
  useEffect(() => {
    end.current?.scrollIntoView({ block: 'end' });
  }, [turns.length, chat.isPending]);

  const busy = Object.values(answers).includes('busy');
  const answer = (key: string, value: Answer) => setAnswers((prev) => ({ ...prev, [key]: value }));

  const send = (content: string) => {
    if (!content.trim() || chat.isPending || busy) return;
    const next = [...turns, { role: 'user', content: content.trim() }];
    setTurns(next);
    setText('');
    chat.mutate({ messages: next.map(({ role, content }) => ({ role, content })), page: location.pathname });
  };
  // An ordinary call as the signed-in user. The model only hears that it was done and the new id.
  const confirm = async (key: string, proposal: Proposal) => {
    answer(key, 'busy');
    try {
      const result = (await call(proposal.tool as ServiceName, proposal.input as never)) as {
        id?: unknown;
        document?: { id?: unknown };
      } | null;
      answer(key, 'done');
      await queryClient.invalidateQueries();
      const id = result?.id ?? result?.document?.id;
      send(`${t('Confirmed: {action}', { action: t(proposal.summary) })}${typeof id === 'string' ? ` (${id})` : ''}`);
    } catch (error) {
      setAnswers(({ [key]: _, ...rest }) => rest);
      toast.error(errorMessage(error));
    }
  };
  const submit = (e: FormEvent) => {
    e.preventDefault();
    send(text);
  };

  return (
    <div className="flex h-full w-full flex-col bg-background">
      <div className="flex h-14 shrink-0 items-center gap-2 border-b px-4">
        <Sparkles className="size-4 text-primary" />
        <span className="flex-1 text-sm font-semibold">{t('Assistant')}</span>
        <Button variant="ghost" size="icon-sm" onClick={onClose} aria-label={t('Close Assistant')}>
          <X />
        </Button>
      </div>
      <div className="flex-1 overflow-y-auto px-4 py-4">
        {turns.length === 0 && (
          <div className="grid gap-2">
            <p className="mb-2 text-muted-foreground">
              {t('Ask about your documents, or have it do things for you. It can only do what you can.')}
            </p>
            {SUGGESTIONS.map((s) => (
              <button key={s} type="button" onClick={() => send(t(s))} className="rounded-md border px-3 py-2 text-start hover:bg-accent">
                {t(s)}
              </button>
            ))}
          </div>
        )}
        <div className="grid gap-4">
          {turns.map((turn, i) => (
            <div key={i} className={cn('grid gap-1.5', turn.role === 'user' && 'justify-items-end')}>
              <div
                className={cn(
                  'max-w-[90%] rounded-lg px-3 py-2',
                  turn.role === 'user' ? 'bg-primary text-primary-foreground' : 'bg-muted [&_p+p]:mt-2 [&_ul]:list-disc [&_ul]:ps-4',
                )}
              >
                {turn.role === 'user' ? turn.content : <Markdown remarkPlugins={[remarkGfm]}>{turn.content}</Markdown>}
              </div>
              {turn.actions && turn.actions.length > 0 && (
                <div className="flex flex-wrap gap-1">
                  {turn.actions.map((a, j) => (
                    <span
                      key={j}
                      className={cn(
                        'inline-flex items-center gap-1 rounded border px-1.5 py-0.5 font-mono text-[11px]',
                        a.ok ? 'text-muted-foreground' : 'border-destructive/40 text-destructive',
                      )}
                    >
                      {a.ok ? <Check className="size-3" /> : <X className="size-3" />} {a.tool}
                    </span>
                  ))}
                </div>
              )}
              {turn.proposals?.map((proposal, j) => {
                const key = `${i}.${j}`;
                const state = answers[key];
                return (
                  <div key={key} className="grid max-w-[90%] gap-2 rounded-lg border p-3">
                    <div className="text-xs font-medium text-muted-foreground">{t('Needs Your Confirmation')}</div>
                    <div className="font-medium">{t(proposal.summary)}</div>
                    <dl className="grid max-h-60 gap-1 overflow-y-auto text-xs">
                      {proposal.details.map((detail, n) => (
                        <div key={n} className="grid gap-0.5">
                          <dt className="font-mono text-muted-foreground">{detail.label}</dt>
                          <dd className="whitespace-pre-wrap break-words">{detail.value}</dd>
                        </div>
                      ))}
                    </dl>
                    {state === 'done' || state === 'dismissed' ? (
                      <div className="text-xs text-muted-foreground">{state === 'done' ? t('Done') : t('Dismissed')}</div>
                    ) : (
                      <div className="flex gap-2">
                        <Button
                          size="sm"
                          variant={proposal.destructive ? 'destructive' : 'default'}
                          disabled={busy || chat.isPending}
                          onClick={() => void confirm(key, proposal)}
                        >
                          {t('Confirm')}
                        </Button>
                        <Button size="sm" variant="outline" disabled={busy} onClick={() => answer(key, 'dismissed')}>
                          {t('Dismiss')}
                        </Button>
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          ))}
          {chat.isPending && <Spinner />}
        </div>
        <div ref={end} />
      </div>
      <form onSubmit={submit} className="flex items-end gap-2 border-t p-3">
        <Textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault();
              send(text);
            }
          }}
          rows={2}
          placeholder={t('Ask or tell the assistant…')}
          className="min-h-0 resize-none"
        />
        <Button type="submit" size="icon" disabled={!text.trim() || chat.isPending} aria-label={t('Send')}>
          <ArrowUp />
        </Button>
      </form>
    </div>
  );
}
