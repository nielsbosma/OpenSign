import { Bug } from 'lucide-react';
import { type RefObject, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { t } from '@/lib/i18n';
import { useIsolatedRoot } from './isolate';

/** Where the top bar's buttons sit when the page is not scrolled (top-2.5). */
const TOP = 10;

/** True while a modal (dialog, sheet, menu) has made the rest of the page unclickable. */
function isPageBlocked(): boolean {
  return getComputedStyle(document.body).pointerEvents === 'none';
}

/** Report a Bug in the top bar. While a modal dims the page, a copy sits in the same place above it. */
export function BugButton({ active, onClick }: { active: boolean; onClick: () => void }) {
  const ref = useRef<HTMLButtonElement>(null);
  return (
    <>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            ref={ref}
            variant={active ? 'secondary' : 'ghost'}
            size="icon"
            onClick={onClick}
            aria-label={t('Report a Bug')}
            className="text-muted-foreground"
          >
            <Bug />
          </Button>
        </TooltipTrigger>
        <TooltipContent>{t('Report a Bug')}</TooltipContent>
      </Tooltip>
      {!active && <AboveModals target={ref} onClick={onClick} />}
    </>
  );
}

function AboveModals({ target, onClick }: { target: RefObject<HTMLButtonElement | null>; onClick: () => void }) {
  const root = useIsolatedRoot('opensign-bugbutton-root');
  const [rect, setRect] = useState<DOMRect | null>(null);

  useEffect(() => {
    const update = () => {
      const button = target.current;
      setRect(button && isPageBlocked() ? button.getBoundingClientRect() : null);
    };
    update();
    const observer = new MutationObserver(update);
    observer.observe(document.body, { attributes: true, attributeFilter: ['style', 'data-scroll-locked'], childList: true });
    window.addEventListener('resize', update);
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', update);
    };
  }, [target]);

  if (!root || !rect || rect.width === 0) return null;
  return createPortal(
    <Button
      variant="outline"
      size="icon"
      data-bugreport-chrome=""
      className="pointer-events-auto fixed text-muted-foreground shadow-md"
      // The top bar scrolls with the page; when it has scrolled away, the copy stays in the top corner.
      style={{ top: Math.max(rect.top, TOP), left: rect.left, width: rect.width, height: rect.height }}
      onClick={onClick}
      aria-label={t('Report a Bug')}
      title={t('Report a Bug')}
    >
      <Bug />
    </Button>,
    root,
  );
}
