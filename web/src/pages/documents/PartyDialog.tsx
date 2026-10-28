import { type FormEvent, useEffect, useState } from 'react';
import { Field } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import type { AuthMethod, Delivery, PartyInput, PartyRole, PartyView } from '@/lib/api';
import { AUTH_LABELS, DELIVERY_LABELS } from '@/lib/format';

const ROLES: Record<PartyRole, string> = { signer: 'Signs', approver: 'Approves', viewer: 'Receives a copy' };
const VIEW_AUTH: AuthMethod[] = ['none', 'sms_pin', 'se_bankid', 'freja', 'broker'];
const SIGN_AUTH: AuthMethod[] = ['standard', 'sms_pin', 'se_bankid', 'freja', 'broker'];
const EID: AuthMethod[] = ['se_bankid', 'freja', 'broker'];

export function partyToInput(p: PartyView): PartyInput {
  return {
    id: p.id,
    name: p.name,
    email: p.email ?? null,
    phone: p.phone ?? null,
    company: p.company ?? null,
    role: p.role as PartyRole,
    delivery: p.delivery as Delivery,
    authToView: p.authToView as AuthMethod,
    authToSign: p.authToSign as AuthMethod,
    signOrder: p.signOrder,
  };
}

function Choice<T extends string>({ value, onChange, options }: { value: T; onChange: (v: T) => void; options: [T, string][] }) {
  return (
    <Select value={value} onValueChange={(v) => onChange(v as T)}>
      <SelectTrigger className="w-full">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {options.map(([v, label]) => (
          <SelectItem key={v} value={v}>
            {label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

export function PartyDialog({
  party,
  maskedNumber,
  sequential,
  open,
  onOpenChange,
  onSave,
  eidAvailable,
}: {
  party: PartyInput | null;
  maskedNumber?: string | null;
  sequential: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSave: (party: PartyInput) => void;
  eidAvailable: boolean;
}) {
  const [p, setP] = useState<PartyInput>({ name: '' });
  useEffect(() => {
    if (open) setP(party ?? { name: '', role: 'signer', delivery: 'email', authToView: 'none', authToSign: 'standard', signOrder: 1 });
  }, [open, party]);
  const set = (patch: Partial<PartyInput>) => setP({ ...p, ...patch });
  const needsNumber = EID.includes(p.authToSign ?? 'standard') || EID.includes(p.authToView ?? 'none');
  const authOptions = (list: AuthMethod[]) =>
    list
      .filter((m) => eidAvailable || !EID.includes(m) || m === p.authToSign || m === p.authToView)
      .map((m) => [m, AUTH_LABELS[m]!] as [AuthMethod, string]);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    onSave(p);
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-xl">
        <form onSubmit={submit} className="grid gap-4">
          <DialogHeader>
            <DialogTitle>{party ? 'Edit Party' : 'Add Party'}</DialogTitle>
            <DialogDescription>Who takes part, how they get their link and how they prove who they are.</DialogDescription>
          </DialogHeader>
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Name" htmlFor="party-name">
              <Input id="party-name" value={p.name} onChange={(e) => set({ name: e.target.value })} required autoFocus />
            </Field>
            <Field label="Role">
              <Choice
                value={p.role ?? 'signer'}
                onChange={(role) => set({ role })}
                options={Object.entries(ROLES) as [PartyRole, string][]}
              />
            </Field>
            <Field label="Email" htmlFor="party-email">
              <Input id="party-email" type="email" value={p.email ?? ''} onChange={(e) => set({ email: e.target.value })} />
            </Field>
            <Field label="Mobile" htmlFor="party-phone" hint="With country code, e.g. +46 70 123 45 67">
              <Input id="party-phone" type="tel" value={p.phone ?? ''} onChange={(e) => set({ phone: e.target.value })} />
            </Field>
            <Field label="Company" htmlFor="party-company">
              <Input id="party-company" value={p.company ?? ''} onChange={(e) => set({ company: e.target.value })} />
            </Field>
            <Field label="Send Link By">
              <Choice
                value={p.delivery ?? 'email'}
                onChange={(delivery) => set({ delivery })}
                options={Object.entries(DELIVERY_LABELS) as [Delivery, string][]}
              />
            </Field>
            <Field label="Identify to View">
              <Choice value={p.authToView ?? 'none'} onChange={(authToView) => set({ authToView })} options={authOptions(VIEW_AUTH)} />
            </Field>
            <Field label="Identify to Sign">
              <Choice value={p.authToSign ?? 'standard'} onChange={(authToSign) => set({ authToSign })} options={authOptions(SIGN_AUTH)} />
            </Field>
            {needsNumber && (
              <Field
                label="Personal Number"
                htmlFor="party-pnr"
                hint={maskedNumber ? `Stored: ${maskedNumber}. Leave empty to keep it.` : 'The eID must belong to this person.'}
              >
                <Input
                  id="party-pnr"
                  value={p.personalNumber ?? ''}
                  onChange={(e) => set({ personalNumber: e.target.value || null })}
                  className="font-mono"
                  autoComplete="off"
                  required={!maskedNumber}
                />
              </Field>
            )}
            {sequential && (
              <Field label="Signing Order" htmlFor="party-order" hint="Lower numbers go first; equal numbers sign at the same time.">
                <Input
                  id="party-order"
                  type="number"
                  min={1}
                  max={100}
                  value={p.signOrder ?? 1}
                  onChange={(e) => set({ signOrder: Number(e.target.value) || 1 })}
                />
              </Field>
            )}
          </div>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit">{party ? 'Save' : 'Add'}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
