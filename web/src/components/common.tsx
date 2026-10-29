import { BookOpen, Check, Copy, Loader2 } from 'lucide-react';
import { type ReactNode, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Label } from '@/components/ui/label';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { useApi } from '@/lib/api';
import { t } from '@/lib/i18n';
import { cn } from '@/lib/utils';

export function Spinner({ className }: { className?: string }) {
  return <Loader2 className={cn('size-4 animate-spin text-muted-foreground', className)} aria-label={t('Loading')} />;
}

/** A spinner centred in the space the content will take. */
export function Loading({ className }: { className?: string }) {
  return (
    <div className={cn('flex items-center justify-center py-16', className)}>
      <Spinner className="size-5" />
    </div>
  );
}

export function safeColor(color?: string | null) {
  return color && /^#[0-9a-fA-F]{6}$/.test(color) ? color : undefined;
}

/** The organisation's name, brand color and logo. Public, so it works on signing and login pages too. */
export function useBranding() {
  return useApi('branding.get', undefined, { staleTime: 5 * 60_000, retry: false }).data;
}

/** The OpenSign mark, in the organisation's brand color. */
export function Logo({ className, color }: { className?: string; color?: string | null }) {
  const branding = useBranding();
  const fill = safeColor(color ?? branding?.brandColor);
  return (
    <svg viewBox="0 0 32 32" className={cn('size-7 shrink-0', className)} aria-hidden>
      <rect width="32" height="32" rx="8" className={fill ? undefined : 'fill-primary'} style={fill ? { fill } : undefined} />
      <path
        d="M8 21c3-1 4-9 7-9s1 7 4 7 2-4 5-4"
        fill="none"
        stroke="#fff"
        strokeWidth="2.4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path d="M8 25h16" stroke="#fff" strokeOpacity=".6" strokeWidth="1.6" strokeLinecap="round" />
    </svg>
  );
}

/** The organisation's uploaded logo, or the OpenSign mark in its brand color when there is none. */
export function BrandLogo({
  className,
  markClassName,
  branding: given,
}: {
  className?: string;
  markClassName?: string;
  branding?: { logoUrl?: string | null; brandColor?: string; organisation?: string };
}) {
  const fetched = useBranding();
  const branding = given ?? fetched;
  if (branding?.logoUrl?.startsWith('/api/logo/')) {
    return (
      <img
        src={branding.logoUrl}
        alt={branding.organisation ?? ''}
        className={cn('h-7 w-auto max-w-40 shrink-0 object-contain dark:rounded-md dark:bg-white dark:px-1.5 dark:py-0.5', className)}
      />
    );
  }
  return <Logo className={markClassName} color={branding?.brandColor} />;
}

/** Opens a page of the public documentation in a new tab. `page` is a docs path such as `bulk-send#row-checks`. */
export function HelpLink({ page, className }: { page: string; className?: string }) {
  return (
    <Button asChild variant="ghost" size="sm" className={cn('text-muted-foreground', className)}>
      <a href={`/docs/${page}`} target="_blank" rel="noreferrer">
        <BookOpen /> {t('Docs')}
      </a>
    </Button>
  );
}

export function PageHeader({
  title,
  description,
  actions,
  help,
}: {
  title: string;
  description?: ReactNode;
  actions?: ReactNode;
  help?: string;
}) {
  return (
    <div className="mb-6 flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
      <div className="min-w-0">
        <h1 className="text-xl font-semibold tracking-tight">{title}</h1>
        {description && <p className="mt-1 text-muted-foreground">{description}</p>}
      </div>
      {(actions || help) && (
        <div className="flex shrink-0 gap-2">
          {help && <HelpLink page={help} />}
          {actions}
        </div>
      )}
    </div>
  );
}

export function Field({
  label,
  htmlFor,
  error,
  hint,
  children,
  className,
}: {
  label: ReactNode;
  htmlFor?: string;
  error?: string;
  hint?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    // content-start: in a two-column row a field without helper text must not stretch, or its label drifts down.
    <div className={cn('grid min-w-0 content-start gap-2', className)}>
      <Label htmlFor={htmlFor}>{label}</Label>
      {children}
      {error ? <p className="text-xs text-destructive">{error}</p> : hint ? <p className="text-xs text-muted-foreground">{hint}</p> : null}
    </div>
  );
}

export function EmptyState({ icon, title, children }: { icon?: ReactNode; title: string; children?: ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed px-6 py-12 text-center">
      {icon && <div className="text-muted-foreground [&_svg]:size-6">{icon}</div>}
      <p className="font-medium">{title}</p>
      {children && <div className="max-w-sm text-muted-foreground">{children}</div>}
    </div>
  );
}

export function CopyButton({ value, label }: { value: string; label?: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      onClick={async () => {
        await navigator.clipboard.writeText(value);
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      }}
    >
      {copied ? <Check /> : <Copy />}
      {copied ? t('Copied') : (label ?? t('Copy'))}
    </Button>
  );
}

export function StatusDot({ tone }: { tone: 'ok' | 'warn' | 'error' | 'off' }) {
  const color = { ok: 'bg-success', warn: 'bg-warning', error: 'bg-destructive', off: 'bg-muted-foreground/40' }[tone];
  return <span className={cn('inline-block size-2 rounded-full', color)} />;
}

export function IconButton({
  label,
  className,
  children,
  ...props
}: Omit<React.ComponentProps<typeof Button>, 'size' | 'aria-label'> & { label: string }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button type="button" variant="ghost" size="icon" aria-label={label} className={cn('text-muted-foreground', className)} {...props}>
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

/** A destructive action that asks first. */
export function ConfirmButton({
  label,
  title,
  description,
  confirmLabel = label,
  onConfirm,
  disabled,
  icon,
}: {
  label: string;
  title: string;
  description: ReactNode;
  confirmLabel?: string;
  onConfirm: () => void;
  disabled?: boolean;
  icon?: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  return (
    <>
      {icon ? (
        <IconButton label={label} className="hover:text-destructive" onClick={() => setOpen(true)} disabled={disabled}>
          {icon}
        </IconButton>
      ) : (
        <Button type="button" variant="danger" onClick={() => setOpen(true)} disabled={disabled}>
          {label}
        </Button>
      )}
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{title}</DialogTitle>
            <DialogDescription>{description}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setOpen(false)}>
              {t('Cancel')}
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                setOpen(false);
                onConfirm();
              }}
            >
              {confirmLabel}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
