import { AlertTriangle, Sparkles } from 'lucide-react';
import { useState } from 'react';
import { Spinner } from '@/components/common';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { type Check, type ReviewOutput, useApi, useApiMutation } from '@/lib/api';
import { formatDateTime } from '@/lib/format';
import { cn } from '@/lib/utils';

type Risk = { severity?: string; title?: string; detail?: string; page?: number; suggestion?: string };
type Term = { label?: string; term?: string; name?: string; value?: string; detail?: string };
type Review = {
  summary?: string;
  documentType?: string;
  risks?: Risk[];
  keyTerms?: Term[];
  missing?: string[];
  questions?: string[];
};

const SEVERITY: Record<string, string> = {
  high: 'border-destructive/40 bg-destructive/5',
  medium: 'border-warning/50 bg-warning/5',
  low: 'border-border',
};

export function Checks({ checks }: { checks: Check[] }) {
  if (!checks.length) return <p className="text-muted-foreground">Everything needed to send is in place.</p>;
  return (
    <ul className="grid gap-1.5">
      {checks.map((c, i) => (
        <li key={i} className="flex gap-2">
          <AlertTriangle className={cn('mt-0.5 size-3.5 shrink-0', c.severity === 'error' ? 'text-destructive' : 'text-warning')} />
          <span>{c.message}</span>
        </li>
      ))}
    </ul>
  );
}

function strings(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((v): v is string => typeof v === 'string') : [];
}

function ReviewResult({ result }: { result: ReviewOutput }) {
  const review = (result.review ?? {}) as Review;
  return (
    <div className="grid gap-5">
      {review.summary && (
        <div>
          {review.documentType && (
            <Badge variant="outline" className="mb-2">
              {review.documentType}
            </Badge>
          )}
          <p className="leading-relaxed">{review.summary}</p>
        </div>
      )}
      {!!review.risks?.length && (
        <section className="grid gap-2">
          <h3 className="font-medium">Risks</h3>
          {review.risks.map((r, i) => (
            <div key={i} className={cn('rounded-md border p-3', SEVERITY[r.severity ?? 'low'])}>
              <div className="flex items-center gap-2">
                <span className="font-medium">{r.title}</span>
                <span className="ml-auto text-xs text-muted-foreground capitalize">
                  {r.severity}
                  {r.page ? ` · page ${r.page}` : ''}
                </span>
              </div>
              {r.detail && <p className="mt-1 text-muted-foreground">{r.detail}</p>}
              {r.suggestion && <p className="mt-1.5">Suggestion: {r.suggestion}</p>}
            </div>
          ))}
        </section>
      )}
      {!!review.keyTerms?.length && (
        <section className="grid gap-1.5">
          <h3 className="font-medium">Key Terms</h3>
          <dl className="grid gap-1">
            {review.keyTerms.map((t, i) => (
              <div key={i} className="grid grid-cols-[minmax(0,2fr)_minmax(0,3fr)] gap-2">
                <dt className="text-muted-foreground">{t.label ?? t.term ?? t.name}</dt>
                <dd>{t.value ?? t.detail}</dd>
              </div>
            ))}
          </dl>
        </section>
      )}
      {strings(review.missing).length > 0 && (
        <section className="grid gap-1.5">
          <h3 className="font-medium">Possibly Missing</h3>
          <ul className="list-disc pl-4">
            {strings(review.missing).map((m, i) => (
              <li key={i}>{m}</li>
            ))}
          </ul>
        </section>
      )}
      {strings(review.questions).length > 0 && (
        <section className="grid gap-1.5">
          <h3 className="font-medium">Questions to Ask</h3>
          <ul className="list-disc pl-4">
            {strings(review.questions).map((m, i) => (
              <li key={i}>{m}</li>
            ))}
          </ul>
        </section>
      )}
      <p className="text-xs text-muted-foreground">
        Reviewed {formatDateTime(result.createdAt)} by {result.model}. An AI review can miss things and is not legal advice.
      </p>
    </div>
  );
}

/** Setup checks always; the LLM review only when an Admin connected a model and allowed document text to be sent. */
export function ReviewPanel({ documentId, draft }: { documentId: string; draft: boolean }) {
  const status = useApi('ai.status');
  const checks = useApi('documents.check', { id: documentId }, { enabled: draft });
  const latest = useApi('documents.latest_review', { id: documentId }, { enabled: !!status.data?.configured });
  const [focus, setFocus] = useState('');
  const review = useApiMutation('documents.review', { invalidates: ['documents.latest_review'] });
  const result = review.data ?? latest.data;
  const canReview = status.data?.configured && status.data.canUse && status.data.allowDocumentText;

  return (
    <div className="grid gap-6">
      {draft && (
        <section className="grid gap-2">
          <h3 className="font-medium">Before Sending</h3>
          {checks.data ? <Checks checks={checks.data} /> : <Spinner />}
        </section>
      )}
      <section className="grid gap-3">
        <h3 className="flex items-center gap-2 font-medium">
          <Sparkles className="size-4 text-primary" /> AI Review
        </h3>
        {!status.data?.configured ? (
          <p className="text-muted-foreground">
            An Admin can connect a language model in Settings to review agreements for risks and missing terms.
          </p>
        ) : !status.data.allowDocumentText ? (
          <p className="text-muted-foreground">Reviews are off: an Admin has not allowed document text to be sent to the model.</p>
        ) : (
          canReview && (
            <div className="grid gap-2">
              <Input value={focus} onChange={(e) => setFocus(e.target.value)} placeholder="Your side or focus, e.g. We are the supplier" />
              <Button
                variant="outline"
                disabled={review.isPending}
                onClick={() => review.mutate({ id: documentId, focus: focus.trim() || null })}
              >
                {review.isPending ? <Spinner /> : <Sparkles />} {result ? 'Review Again' : 'Review Document'}
              </Button>
            </div>
          )
        )}
        {result && <ReviewResult result={result} />}
      </section>
    </div>
  );
}
