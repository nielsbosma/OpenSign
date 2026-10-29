import { Download, QrCode, RefreshCw } from 'lucide-react';
import QRCode from 'qrcode';
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { ConfirmButton, CopyButton, Field } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { type DocumentView, type Input as ServiceInput, useApi, useApiMutation } from '@/lib/api';
import { saveBytes } from '@/lib/format';
import { t } from '@/lib/i18n';

type SetInput = ServiceInput<'share_links.set'>;

function QrDialog({
  url,
  title,
  open,
  onOpenChange,
}: {
  url: string;
  title: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const [src, setSrc] = useState<string>();
  useEffect(() => {
    if (open) void QRCode.toDataURL(url, { margin: 2, width: 1024 }).then(setSrc);
  }, [open, url]);
  const name = title.replace(/[\\/:*?"<>|]+/g, ' ').trim() || 'share-link';
  const png = async () =>
    saveBytes(await (await fetch(await QRCode.toDataURL(url, { margin: 2, width: 1024 }))).arrayBuffer(), `${name}.png`, 'image/png');
  const svg = async () =>
    saveBytes(new TextEncoder().encode(await QRCode.toString(url, { type: 'svg', margin: 2 })), `${name}.svg`, 'image/svg+xml');
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>{t('QR Code')}</DialogTitle>
          <DialogDescription>{t('Print it on a poster, a counter sign or a letter. Scanning it opens the public link.')}</DialogDescription>
        </DialogHeader>
        {src && <img src={src} alt={t('QR code for the public link')} className="mx-auto size-64 rounded-md border bg-white" />}
        <div className="flex justify-center gap-2">
          <Button variant="outline" onClick={png}>
            <Download /> PNG
          </Button>
          <Button variant="outline" onClick={svg}>
            <Download /> SVG
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** A template's public link: anyone who opens it confirms their email and then signs their own copy. */
export function ShareLinkSection({ doc }: { doc: DocumentView }) {
  const query = useApi('share_links.get', { templateId: doc.id });
  const set = useApiMutation('share_links.set', { invalidates: ['share_links.'] });
  const rotate = useApiMutation('share_links.rotate', {
    invalidates: ['share_links.'],
    onSuccess: () => toast.success(t('The link has a new address. The old one and its QR code no longer work.')),
  });
  const [showQr, setShowQr] = useState(false);
  const link = query.data;
  const actors = doc.parties.filter((p) => p.role !== 'viewer');
  const save = (patch: Partial<SetInput>) =>
    set.mutate({
      templateId: doc.id,
      enabled: link?.enabled ?? false,
      partyId: link?.partyId ?? actors[0]?.id ?? null,
      perHour: link?.perHour ?? 10,
      maxTotal: link?.maxTotal ?? null,
      expiresAt: link?.expiresAt ?? null,
      ...patch,
    });
  const number = (value: string) => (value.trim() ? Math.max(1, Math.round(Number(value))) : null);

  return (
    <section className="grid gap-2.5">
      <h3 className="font-medium">{t('Public Link')}</h3>
      <label className="flex items-center justify-between gap-3 rounded-md bg-muted/50 px-3 py-2">
        <span>
          <span className="block">{t('Share With a Link or QR Code')}</span>
          <span className="block text-xs text-muted-foreground">
            {actors.length
              ? t(
                  'Anyone with the link can fill in and sign their own copy, after confirming their email address. The documents are sent from you.',
                )
              : t('Add a party who signs first. Whoever opens the link takes their place.')}
          </span>
        </span>
        <Switch
          checked={!!link?.enabled}
          disabled={set.isPending || query.isPending || (!link?.enabled && !actors.length)}
          onCheckedChange={(enabled) => save({ enabled })}
        />
      </label>
      {link?.enabled && (
        <div className="grid gap-3 rounded-md border p-3">
          <div className="flex items-center gap-2">
            <Input readOnly value={link.url} aria-label={t('Public link')} className="h-8 font-mono text-xs" dir="ltr" />
            <CopyButton value={link.url} />
          </div>
          <Field label={t('Filled In by the Visitor')} htmlFor="share-party">
            <Select value={link.partyId ?? ''} onValueChange={(partyId) => save({ partyId })}>
              <SelectTrigger id="share-party" className="w-full">
                <SelectValue placeholder={t('Choose a party')} />
              </SelectTrigger>
              <SelectContent>
                {actors.map((p) => (
                  <SelectItem key={p.id} value={p.id}>
                    {p.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <div className="grid grid-cols-2 gap-3">
            <Field label={t('Per Hour')} htmlFor="share-per-hour">
              <Input
                id="share-per-hour"
                key={`h${link.perHour}`}
                type="number"
                min={1}
                max={1000}
                defaultValue={link.perHour}
                onBlur={(e) => {
                  const perHour = number(e.target.value);
                  if (perHour && perHour !== link.perHour) save({ perHour: Math.min(perHour, 1000) });
                }}
              />
            </Field>
            <Field label={t('In Total')} htmlFor="share-total">
              <Input
                id="share-total"
                key={`t${link.maxTotal}`}
                type="number"
                min={1}
                placeholder={t('No limit')}
                defaultValue={link.maxTotal ?? ''}
                onBlur={(e) => {
                  const maxTotal = number(e.target.value);
                  if (maxTotal !== (link.maxTotal ?? null)) save({ maxTotal });
                }}
              />
            </Field>
          </div>
          <Field label={t('Last Day')} htmlFor="share-ends">
            <Input
              id="share-ends"
              type="date"
              value={link.expiresAt ? link.expiresAt.slice(0, 10) : ''}
              onChange={(e) => save({ expiresAt: e.target.value ? new Date(`${e.target.value}T23:59:59`).toISOString() : null })}
            />
          </Field>
          <p className="text-xs text-muted-foreground">
            {t('{count} documents made so far. Each visitor confirms their email with a code before anything is made.', {
              count: link.documentsCreated,
            })}
          </p>
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" size="sm" onClick={() => setShowQr(true)}>
              <QrCode /> {t('QR Code')}
            </Button>
            <ConfirmButton
              label={t('New Address')}
              title={t('Give the Link a New Address?')}
              description={t('The current link and any printed QR codes stop working at once.')}
              onConfirm={() => rotate.mutate({ templateId: doc.id })}
              disabled={rotate.isPending}
              icon={<RefreshCw />}
            />
          </div>
          <QrDialog url={link.url} title={doc.title} open={showQr} onOpenChange={setShowQr} />
        </div>
      )}
    </section>
  );
}
