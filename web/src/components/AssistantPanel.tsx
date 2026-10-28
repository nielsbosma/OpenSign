import { useQueryClient } from '@tanstack/react-query';
import { ArrowUp, Check, Sparkles, X } from 'lucide-react';
import { type FormEvent, useEffect, useRef, useState } from 'react';
import Markdown from 'react-markdown';
import { useLocation } from 'react-router';
import remarkGfm from 'remark-gfm';
import { Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { type ChatMessage, useApiMutation } from '@/lib/api';
import { cn } from '@/lib/utils';

type Turn = ChatMessage & { actions?: { tool: string; ok: boolean }[] };

const SUGGESTIONS = [
  'Which documents are waiting for signatures?',
  'Remind everyone who has not signed this week',
  'Create a draft NDA for Bo Berg',
];

/** The optional assistant. It acts as you, with your permissions, through the same services as the app. */
export function AssistantPanel({ onClose }: { onClose: () => void }) {
  const [turns, setTurns] = useState<Turn[]>([]);
  const [text, setText] = useState('');
  const location = useLocation();
  const queryClient = useQueryClient();
  const end = useRef<HTMLDivElement>(null);
  const chat = useApiMutation('ai.chat', {
    onSuccess: async (output) => {
      setTurns((t) => [...t, { role: 'assistant', content: output.reply, actions: output.actions }]);
      if (output.actions.some((a) => a.ok)) await queryClient.invalidateQueries();
    },
  });

  // biome-ignore lint/correctness/useExhaustiveDependencies: scroll when a message arrives
  useEffect(() => end.current?.scrollIntoView({ block: 'end' }), [turns.length, chat.isPending]);

  const send = (content: string) => {
    if (!content.trim() || chat.isPending) return;
    const next = [...turns, { role: 'user', content: content.trim() }];
    setTurns(next);
    setText('');
    chat.mutate({ messages: next.map(({ role, content }) => ({ role, content })), page: location.pathname });
  };
  const submit = (e: FormEvent) => {
    e.preventDefault();
    send(text);
  };

  return (
    <div className="flex h-full w-full flex-col bg-background">
      <div className="flex h-14 shrink-0 items-center gap-2 border-b px-4">
        <Sparkles className="size-4 text-primary" />
        <span className="flex-1 text-sm font-semibold">Assistant</span>
        <Button variant="ghost" size="icon-sm" onClick={onClose} aria-label="Close Assistant">
          <X />
        </Button>
      </div>
      <div className="flex-1 overflow-y-auto px-4 py-4">
        {turns.length === 0 && (
          <div className="grid gap-2">
            <p className="mb-2 text-muted-foreground">
              Ask about your documents, or have it do things for you. It can only do what you can.
            </p>
            {SUGGESTIONS.map((s) => (
              <button key={s} type="button" onClick={() => send(s)} className="rounded-md border px-3 py-2 text-left hover:bg-accent">
                {s}
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
                  turn.role === 'user' ? 'bg-primary text-primary-foreground' : 'bg-muted [&_p+p]:mt-2 [&_ul]:list-disc [&_ul]:pl-4',
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
          placeholder="Ask or tell the assistant…"
          className="min-h-0 resize-none"
        />
        <Button type="submit" size="icon" disabled={!text.trim() || chat.isPending} aria-label="Send">
          <ArrowUp />
        </Button>
      </form>
    </div>
  );
}
