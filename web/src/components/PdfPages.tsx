import * as pdfjs from 'pdfjs-dist';
import workerUrl from 'pdfjs-dist/build/pdf.worker.min.mjs?url';
import { type ReactNode, useEffect, useRef, useState } from 'react';
import { Spinner } from '@/components/common';
import { cn } from '@/lib/utils';

pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

type Page = {
  width: number;
  height: number;
  render: (canvas: HTMLCanvasElement, width: number) => { promise: Promise<void>; cancel: () => void };
};

/** Loads a PDF from bytes. Scripts and forms in the file never run: pdf.js only paints it. */
export function usePdf(load: (() => Promise<ArrayBuffer>) | null, key: string) {
  const [pages, setPages] = useState<Page[] | null>(null);
  const [error, setError] = useState<string>();
  // biome-ignore lint/correctness/useExhaustiveDependencies: reloads when the key changes, not on every new loader
  useEffect(() => {
    if (!load) return;
    let cancelled = false;
    let destroy: (() => void) | undefined;
    setError(undefined);
    load()
      .then(async (bytes) => {
        const task = pdfjs.getDocument({ data: new Uint8Array(bytes), enableXfa: false });
        destroy = () => void task.destroy();
        const doc = await task.promise;
        const list: Page[] = [];
        for (let i = 1; i <= doc.numPages; i++) {
          const page = await doc.getPage(i);
          const viewport = page.getViewport({ scale: 1 });
          list.push({
            width: viewport.width,
            height: viewport.height,
            render: (canvas, width) => {
              const ratio = window.devicePixelRatio || 1;
              const scaled = page.getViewport({ scale: (width / viewport.width) * ratio });
              canvas.width = Math.floor(scaled.width);
              canvas.height = Math.floor(scaled.height);
              const task = page.render({ canvas, viewport: scaled, annotationMode: pdfjs.AnnotationMode.DISABLE });
              return { promise: task.promise, cancel: () => task.cancel() };
            },
          });
        }
        if (!cancelled) setPages(list);
      })
      .catch((e) => !cancelled && setError(e instanceof Error ? e.message : 'The document could not be shown'));
    return () => {
      cancelled = true;
      destroy?.();
    };
  }, [key]);
  return { pages, error };
}

function PageCanvas({ page, width }: { page: Page; width: number }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    if (!ref.current || width <= 0) return;
    const task = page.render(ref.current, width);
    task.promise.catch(() => {});
    return task.cancel;
  }, [page, width]);
  return <canvas ref={ref} className="block h-full w-full" />;
}

/** Every page at the container's width, with an overlay per page for fields. */
export function PdfPages({
  pages,
  overlay,
  onPageClick,
  dropType,
  onPageDrop,
}: {
  pages: Page[];
  overlay?: (index: number) => ReactNode;
  onPageClick?: (index: number, x: number, y: number) => void;
  /** The drag data type pages accept, such as a field kind dragged from a toolbar. */
  dropType?: string;
  onPageDrop?: (index: number, x: number, y: number, data: string) => void;
}) {
  const [dropTarget, setDropTarget] = useState<number | null>(null);
  const container = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    const el = container.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => setWidth(Math.floor(entry!.contentRect.width)));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  return (
    <div ref={container} className="grid gap-4">
      {pages.map((page, i) => (
        <div
          key={i}
          data-page={i}
          className={cn(
            'relative overflow-hidden rounded-sm bg-white shadow-sm ring-1 ring-border',
            dropTarget === i && 'ring-2 ring-primary',
          )}
          style={{ aspectRatio: `${page.width} / ${page.height}` }}
          onClick={(e) => {
            if (!onPageClick || e.target !== e.currentTarget.firstChild) return;
            const rect = e.currentTarget.getBoundingClientRect();
            onPageClick(i, (e.clientX - rect.left) / rect.width, (e.clientY - rect.top) / rect.height);
          }}
          onDragOver={(e) => {
            if (!onPageDrop || !dropType || !e.dataTransfer.types.includes(dropType)) return;
            e.preventDefault();
            e.dataTransfer.dropEffect = 'copy';
            setDropTarget(i);
          }}
          onDragLeave={(e) => {
            if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setDropTarget(null);
          }}
          onDrop={(e) => {
            setDropTarget(null);
            const data = dropType && e.dataTransfer.getData(dropType);
            if (!onPageDrop || !data) return;
            e.preventDefault();
            const rect = e.currentTarget.getBoundingClientRect();
            onPageDrop(i, (e.clientX - rect.left) / rect.width, (e.clientY - rect.top) / rect.height, data);
          }}
        >
          <PageCanvas page={page} width={width} />
          {overlay?.(i)}
        </div>
      ))}
    </div>
  );
}

export function PdfLoading({ error }: { error?: string }) {
  return (
    <div className="flex h-96 items-center justify-center rounded-lg border bg-muted/40 text-muted-foreground">
      {error ?? <Spinner className="size-6" />}
    </div>
  );
}
