import { CheckCircle2, ShieldCheck, XCircle } from 'lucide-react';
import { useState } from 'react';
import { Spinner } from '@/components/common';
import { errorMessage, type VerifyOutput, verifyFile } from '@/lib/api';
import { formatDateTime } from '@/lib/format';
import { AuthLayout, FormError } from './auth/AuthLayout';

type Signature = {
  kind: 'signature' | 'documentTimestamp';
  signer?: string | null;
  reason?: string | null;
  coversWholeFile: boolean;
  onlyValidationDataAdded: boolean;
  intact: boolean;
  timestamp?: string | null;
  problems: string[];
};

/** Public: anyone holding a signed PDF can check that it is intact and that OpenSign sealed it. */
export function VerifyPage() {
  const [result, setResult] = useState<VerifyOutput | null>(null);
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const check = async (file?: File) => {
    if (!file) return;
    setBusy(true);
    setError(undefined);
    setResult(null);
    try {
      setResult(await verifyFile(file));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  const signatures = (result?.signatures ?? []) as Signature[];
  return (
    <AuthLayout title="Verify a Signed Document" description="The file is checked on this server and not stored." wide>
      <div className="grid gap-4">
        <label
          onDragOver={(e) => e.preventDefault()}
          onDrop={(e) => {
            e.preventDefault();
            void check(e.dataTransfer.files[0]);
          }}
          className="flex cursor-pointer flex-col items-center gap-2 rounded-lg border border-dashed px-6 py-10 text-center hover:bg-accent/50"
        >
          {busy ? <Spinner className="size-6" /> : <ShieldCheck className="size-6 text-muted-foreground" />}
          <span className="font-medium">Drop the signed PDF here</span>
          <span className="text-xs text-muted-foreground">or click to choose it</span>
          <input type="file" accept="application/pdf,.pdf" className="sr-only" onChange={(e) => check(e.target.files?.[0])} />
        </label>
        <FormError message={error} />
        {result && (
          <div className="grid gap-3">
            {result.document ? (
              <div className="flex gap-3 rounded-md border border-success/40 bg-success/5 p-3">
                <CheckCircle2 className="mt-0.5 size-5 shrink-0 text-success" />
                <div>
                  <div className="font-medium">Sealed by this OpenSign</div>
                  <div className="text-muted-foreground">
                    “{result.document.title}”, completed {formatDateTime(result.document.completedAt)}. The file is exactly as sealed.
                  </div>
                </div>
              </div>
            ) : (
              <div className="flex gap-3 rounded-md border border-warning/50 bg-warning/5 p-3">
                <XCircle className="mt-0.5 size-5 shrink-0 text-warning" />
                <div>
                  <div className="font-medium">Not a document this server sealed</div>
                  <div className="text-muted-foreground">It was changed after sealing, or it comes from somewhere else.</div>
                </div>
              </div>
            )}
            {signatures.length > 0 && (
              <ul className="grid gap-1.5">
                {signatures.map((s, i) => (
                  <li key={i} className="rounded-md border px-3 py-2">
                    <div className="flex items-center gap-2 font-medium">
                      {s.intact ? <CheckCircle2 className="size-4 text-success" /> : <XCircle className="size-4 text-warning" />}
                      {s.kind === 'documentTimestamp' ? 'Archive Time Stamp' : (s.signer ?? 'Digital Signature')}
                    </div>
                    <div className="text-xs text-muted-foreground">
                      {[
                        s.kind === 'documentTimestamp' ? s.signer : s.reason,
                        s.timestamp ? `timestamped ${formatDateTime(s.timestamp)}` : null,
                        s.onlyValidationDataAdded ? 'only validation data was added after it' : null,
                        ...s.problems,
                      ]
                        .filter(Boolean)
                        .join(' · ')}
                    </div>
                  </li>
                ))}
              </ul>
            )}
            <code className="break-all text-xs text-muted-foreground">SHA-256 {result.sha256}</code>
          </div>
        )}
      </div>
    </AuthLayout>
  );
}
