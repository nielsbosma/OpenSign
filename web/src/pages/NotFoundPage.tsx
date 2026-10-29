import { Link } from 'react-router';
import { EmptyState } from '@/components/common';
import { Button } from '@/components/ui/button';
import { t } from '@/lib/i18n';

export function NotFoundPage() {
  return (
    <div className="pt-16">
      <EmptyState title={t('There is nothing here')}>
        <p className="mb-4">{t('The page may have moved, or the link is wrong.')}</p>
        <Button asChild variant="outline">
          <Link to="/documents">{t('Go to Documents')}</Link>
        </Button>
      </EmptyState>
    </div>
  );
}
