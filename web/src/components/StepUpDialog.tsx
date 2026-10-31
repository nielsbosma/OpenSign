import { KeyRound } from 'lucide-react';
import { type FormEvent, useEffect, useState } from 'react';
import { Field, Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { ApiError, call, errorMessage, type Output, type Input as ServiceInput } from '@/lib/api';
import { t } from '@/lib/i18n';
import { getPasskey, passkeyCanceled, passkeysSupported } from '@/lib/passkeys';
import { FormError } from '@/pages/auth/AuthLayout';

/** The server asks for this before something that hands out lasting access. */
export function needsStepUp(error: unknown) {
  return error instanceof ApiError && error.code === 'step_up_required';
}

export function StepUpDialog({
  open,
  onOpenChange,
  onConfirmed,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirmed: () => void;
}) {
  const [options, setOptions] = useState<Output<'account.step_up_begin'>>();
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setPassword('');
    setError(undefined);
    call('account.step_up_begin', {}).then(setOptions, (e) => setError(errorMessage(e)));
  }, [open]);

  const confirm = async (input: () => Promise<ServiceInput<'account.step_up'>>) => {
    setBusy(true);
    setError(undefined);
    try {
      await call('account.step_up', await input());
      onConfirmed();
    } catch (e) {
      if (!passkeyCanceled(e)) setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    void confirm(async () => ({ currentPassword: password }));
  };

  // A challenge works for one attempt, so each try at the passkey gets its own.
  const confirmWithPasskey = () =>
    void confirm(async () => {
      const { passkey } = await call('account.step_up_begin', {});
      const response = await getPasskey(passkey!.options as Record<string, unknown>);
      return { passkey: { state: passkey!.state, ...response } };
    });

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <form onSubmit={submit} className="grid gap-4">
          <DialogHeader>
            <DialogTitle>{t('Confirm It Is You')}</DialogTitle>
            <DialogDescription>
              {t('Confirm with your password or passkey before you continue. This lasts for ten minutes.')}
            </DialogDescription>
          </DialogHeader>
          <FormError message={error} />
          {options?.hasPassword && (
            <Field label={t('Password')} htmlFor="step-up-password">
              <Input
                id="step-up-password"
                type="password"
                autoComplete="current-password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                autoFocus
              />
            </Field>
          )}
          <DialogFooter>
            {options?.passkey && passkeysSupported() && (
              <Button type="button" variant="outline" disabled={busy} onClick={confirmWithPasskey}>
                <KeyRound /> {t('Use a Passkey')}
              </Button>
            )}
            {options?.hasPassword && (
              <Button type="submit" disabled={busy || !password}>
                {busy && <Spinner className="text-primary-foreground" />} {t('Continue')}
              </Button>
            )}
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
