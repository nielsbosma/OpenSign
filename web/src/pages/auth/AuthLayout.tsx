import type { ReactNode } from 'react';
import { BrandLogo, useBranding } from '@/components/common';
import { LanguagePicker } from '@/components/LanguagePicker';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { msg } from '@/lib/i18n';
import { cn } from '@/lib/utils';

export function AuthLayout({
  title,
  description,
  children,
  wide,
}: {
  title: string;
  description?: ReactNode;
  children: ReactNode;
  wide?: boolean;
}) {
  const branding = useBranding();
  return (
    <div className="flex min-h-svh flex-col items-center justify-center bg-muted/40 px-4 py-10">
      <div className="mb-6 flex items-center gap-2.5">
        <BrandLogo className="h-9 max-w-56" markClassName="size-8" />
        {!branding?.logoUrl && <span className="text-base font-semibold">OpenSign</span>}
      </div>
      <Card className={cn('w-full shadow-none', wide ? 'max-w-xl' : 'max-w-sm')}>
        <CardHeader>
          <CardTitle className="text-lg">{title}</CardTitle>
          {description && <CardDescription>{description}</CardDescription>}
        </CardHeader>
        <CardContent>{children}</CardContent>
      </Card>
      <LanguagePicker className="mt-6" />
    </div>
  );
}

export function FormError({ message }: { message?: string }) {
  if (!message) return null;
  return (
    <div role="alert" className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-destructive">
      {message}
    </div>
  );
}

/** The `#token` part of an emailed link. Fragments never reach servers, logs or Referer headers. */
export function useHashToken() {
  return decodeURIComponent(window.location.hash.slice(1));
}

export const PASSWORD_HINT = msg('At least 12 characters. A few unrelated words make a strong one.');
