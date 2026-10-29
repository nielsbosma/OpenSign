import { Plus, Trash2 } from 'lucide-react';
import { type FormEvent, useEffect, useState } from 'react';
import { Field } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import type { AttachmentRequest, AttachmentRequestInput, AuthMethod, Delivery, PartyInput, PartyRole, PartyView } from '@/lib/api';
import { AUTH_LABELS, DELIVERY_LABELS, languageName, languages } from '@/lib/format';
import { labels, msg, t } from '@/lib/i18n';

const ROLES = labels({ signer: msg('Signs'), approver: msg('Approves'), viewer: msg('Receives a copy') }) as Record<PartyRole, string>;
const VIEW_AUTH: AuthMethod[] = ['none', 'sms_pin', 'se_bankid', 'freja', 'broker'];
const SIGN_AUTH: AuthMethod[] = ['standard', 'sms_pin', 'se_bankid', 'freja', 'broker'];
const EID: AuthMethod[] = ['se_bankid', 'freja', 'broker'];

/** Pass the document's attachment requests to edit them; leave them out to keep what is stored. */
export function partyToInput(p: PartyView, requests?: AttachmentRequest[]): PartyInput {
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
    language: p.language ?? null,
    attachments: requests
      ?.filter((r) => r.partyId === p.id)
      .map((r) => ({ name: r.name, description: r.description, required: r.required })),
  };
}

function AttachmentRequests({ value, onChange }: { value: AttachmentRequestInput[]; onChange: (v: AttachmentRequestInput[]) => void }) {
  const update = (i: number, patch: Partial<AttachmentRequestInput>) => onChange(value.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  return (
    <div className="grid gap-2 sm:col-span-2">
      <div className="flex items-center justify-between">
        <span className="text-sm font-medium">{t('Ask for Attachments')}</span>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={value.length >= 10}
          onClick={() => onChange([...value, { name: '', description: '', required: true }])}
        >
          <Plus /> {t('Add')}
        </Button>
      </div>
      {value.length === 0 && (
        <p className="text-xs text-muted-foreground">{t('For example a copy of an ID or a certificate. PDF, JPEG or PNG.')}</p>
      )}
      {value.map((r, i) => (
        <div key={i} className="grid gap-2 rounded-md border p-2 sm:grid-cols-[1fr_1fr_auto_auto] sm:items-center">
          <Input
            aria-label={t('Attachment name')}
            placeholder={t('Copy of ID')}
            value={r.name}
            required
            onChange={(e) => update(i, { name: e.target.value })}
          />
          <Input
            aria-label={t('Attachment description')}
            placeholder={t('Description (optional)')}
            value={r.description ?? ''}
            onChange={(e) => update(i, { description: e.target.value })}
          />
          <label className="flex items-center gap-1.5 text-xs">
            <Checkbox checked={r.required ?? true} onCheckedChange={(c) => update(i, { required: c === true })} /> {t('Required')}
          </label>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label={t('Remove attachment')}
            onClick={() => onChange(value.filter((_, j) => j !== i))}
          >
            <Trash2 />
          </Button>
        </div>
      ))}
    </div>
  );
}

const NOT_SET_UP = msg('Not set up yet. An admin adds it in Settings.');

function Choice<T extends string>({
  value,
  onChange,
  options,
  unavailable = [],
}: {
  value: T;
  onChange: (v: T) => void;
  options: [T, string][];
  unavailable?: T[];
}) {
  return (
    <Select value={value} onValueChange={(v) => onChange(v as T)}>
      <SelectTrigger className="w-full">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {options.map(([v, label]) => (
          <SelectItem
            key={v}
            value={v}
            disabled={unavailable.includes(v)}
            description={unavailable.includes(v) ? t(NOT_SET_UP) : undefined}
          >
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
  available,
  documentLanguage,
}: {
  party: PartyInput | null;
  maskedNumber?: string | null;
  sequential: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSave: (party: PartyInput) => void;
  /** Methods set up on this server; a party keeps a method it already has. */
  available: AuthMethod[];
  documentLanguage: string;
}) {
  const [p, setP] = useState<PartyInput>({ name: '' });
  useEffect(() => {
    if (open)
      setP(
        party ?? { name: '', role: 'signer', delivery: 'email', authToView: 'none', authToSign: 'standard', signOrder: 1, attachments: [] },
      );
  }, [open, party]);
  const set = (patch: Partial<PartyInput>) => setP({ ...p, ...patch });
  const needsNumber = EID.includes(p.authToSign ?? 'standard') || EID.includes(p.authToView ?? 'none');
  const authOptions = (list: AuthMethod[]) => list.map((m) => [m, AUTH_LABELS[m]!] as [AuthMethod, string]);
  const unavailableAuth = [...VIEW_AUTH, ...SIGN_AUTH].filter((m) => !available.includes(m) && m !== p.authToSign && m !== p.authToView);
  const unavailableDelivery: Delivery[] = available.includes('sms_pin') || p.delivery === 'sms' ? [] : ['sms'];

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
            <DialogTitle>{party ? t('Edit Party') : t('Add Party')}</DialogTitle>
            <DialogDescription>{t('Who takes part, how they get their link and how they prove who they are.')}</DialogDescription>
          </DialogHeader>
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label={t('Name')} htmlFor="party-name">
              <Input id="party-name" value={p.name} onChange={(e) => set({ name: e.target.value })} required autoFocus />
            </Field>
            <Field label={t('Role')}>
              <Choice
                value={p.role ?? 'signer'}
                onChange={(role) => set({ role })}
                options={Object.entries(ROLES) as [PartyRole, string][]}
              />
            </Field>
            <Field label={t('Email')} htmlFor="party-email">
              <Input id="party-email" type="email" value={p.email ?? ''} onChange={(e) => set({ email: e.target.value })} />
            </Field>
            <Field label={t('Mobile')} htmlFor="party-phone" hint={t('With country code, e.g. +46 70 123 45 67')}>
              <Input id="party-phone" type="tel" value={p.phone ?? ''} onChange={(e) => set({ phone: e.target.value })} />
            </Field>
            <Field label={t('Company')} htmlFor="party-company">
              <Input id="party-company" value={p.company ?? ''} onChange={(e) => set({ company: e.target.value })} />
            </Field>
            <Field label={t('Send Link By')}>
              <Choice
                value={p.delivery ?? 'email'}
                onChange={(delivery) => set({ delivery })}
                options={Object.entries(DELIVERY_LABELS) as [Delivery, string][]}
                unavailable={unavailableDelivery}
              />
            </Field>
            <Field label={t('Identify to View')}>
              <Choice
                value={p.authToView ?? 'none'}
                onChange={(authToView) => set({ authToView })}
                options={authOptions(VIEW_AUTH)}
                unavailable={unavailableAuth}
              />
            </Field>
            <Field label={t('Identify to Sign')}>
              <Choice
                value={p.authToSign ?? 'standard'}
                onChange={(authToSign) => set({ authToSign })}
                options={authOptions(SIGN_AUTH)}
                unavailable={unavailableAuth}
              />
            </Field>
            <Field label={t('Language')} hint={t('Their emails, SMS and signing page.')}>
              <Choice
                value={p.language ?? 'document'}
                onChange={(language) => set({ language: language === 'document' ? null : language })}
                options={[['document', t('Same as Document ({language})', { language: languageName(documentLanguage) })], ...languages()]}
              />
            </Field>
            {needsNumber && (
              <Field
                label={t('Personal Number')}
                htmlFor="party-pnr"
                hint={
                  maskedNumber
                    ? t('Stored: {number}. Leave empty to keep it.', { number: maskedNumber })
                    : t('The eID must belong to this person.')
                }
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
              <Field
                label={t('Signing Order')}
                htmlFor="party-order"
                hint={t('Lower numbers go first; equal numbers sign at the same time.')}
              >
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
            {(p.role ?? 'signer') !== 'viewer' && (
              <AttachmentRequests value={p.attachments ?? []} onChange={(attachments) => set({ attachments })} />
            )}
          </div>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              {t('Cancel')}
            </Button>
            <Button type="submit">{party ? t('Save') : t('Add')}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
