import { Link } from 'react-router';
import { EmptyState } from '@/components/common';
import { Button } from '@/components/ui/button';

export function NotFoundPage() {
  return (
    <div className="pt-16">
      <EmptyState title="There is nothing here">
        <p className="mb-4">The page may have moved, or the link is wrong.</p>
        <Button asChild variant="outline">
          <Link to="/documents">Go to Documents</Link>
        </Button>
      </EmptyState>
    </div>
  );
}
