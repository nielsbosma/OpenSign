import { useIsMutating } from '@tanstack/react-query';
import { Eye, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Spinner } from '@/components/common';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import type { DocumentView } from '@/lib/api';
import { t } from '@/lib/i18n';
import { SigningPreview } from '../sign/SignPage';

export function SigningPreviewDialog({
  doc,
  open,
  onOpenChange,
}: {
  doc: DocumentView;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const [partyId, setPartyId] = useState<string>();
  const [ready, setReady] = useState(false);
  const saving = useIsMutating() > 0;
  const party = doc.parties.find((p) => p.id === partyId) ?? doc.parties.find((p) => p.role !== 'viewer') ?? doc.parties[0];

  useEffect(() => {
    if (!open) setReady(false);
    else if (!saving) setReady(true);
  }, [open, saving]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showCloseButton={false}
        className="top-0 left-0 block h-svh max-w-none translate-x-0 translate-y-0 gap-0 overflow-y-auto rounded-none border-0 p-0 sm:max-w-none"
      >
        <div className="sticky top-0 z-10 flex flex-wrap items-center gap-3 border-b bg-background px-4 py-2.5 shadow-sm">
          <Eye className="size-4 text-muted-foreground" />
          <DialogTitle className="text-sm font-semibold">{t('Preview')}</DialogTitle>
          {doc.parties.length > 1 && party && (
            <Select value={party.id} onValueChange={setPartyId}>
              <SelectTrigger size="sm" className="w-56" aria-label={t('Party')}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {doc.parties.map((p) => (
                  <SelectItem key={p.id} value={p.id}>
                    {p.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
          <DialogDescription className="min-w-0 flex-1 text-xs">
            {party
              ? t('How {name} will see the signing page. Nothing can be signed or sent from here.', { name: party.name })
              : t('How the party will see the signing page. Nothing can be signed or sent from here.')}
          </DialogDescription>
          <Button variant="ghost" size="icon" aria-label={t('Close')} onClick={() => onOpenChange(false)}>
            <X />
          </Button>
        </div>
        {party && ready ? (
          <SigningPreview documentId={doc.id} partyId={party.id} />
        ) : (
          <div className="flex justify-center pt-20">
            <Spinner className="size-6" />
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
