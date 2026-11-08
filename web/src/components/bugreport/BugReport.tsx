import { Bug, EyeOff, Loader2, MessageSquare, Pause, X } from 'lucide-react';
import { domToCanvas } from 'modern-screenshot';
import { type PointerEvent as ReactPointerEvent, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useLocation } from 'react-router';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { call, errorMessage, type Input as ServiceInput } from '@/lib/api';
import { diagnostics } from '@/lib/diagnostics';
import { msg, t, useLanguage } from '@/lib/i18n';
import { useSession } from '@/lib/session';
import { cn } from '@/lib/utils';
import { type Box, browserName, clip, isPrivate, openDialogTitles, privateBoxes, routePattern, scrub } from './report';

/*
 * The report tools live in their own element at the end of <body>, above everything, and stop their
 * events there. Radix dialogs and menus listen on `document` to close on outside clicks, trap focus
 * and lock scrolling; with the events stopped at this root they never notice, so the tools work on
 * top of an open dialog.
 */
const ROOT_ID = 'opensign-bugreport-root';
const ISOLATED_EVENTS = [
  'pointerdown',
  'pointerup',
  'mousedown',
  'mouseup',
  'click',
  'dblclick',
  'contextmenu',
  'touchstart',
  'touchend',
  'touchmove',
  'wheel',
  'focusin',
  'keydown',
  'keyup',
  'keypress',
  'input',
] as const;
/** Left out of the screenshot. */
const CHROME = 'data-bugreport-chrome';
const chrome = { [CHROME]: '' };
const MAX_SCREENSHOT = 10_000_000;
const MASK = '#9ca3af';

type Mode = 'comment' | 'hide' | 'paused';
type Note = { id: number; el: Element; text: string; label: string; tag: string };
type Shot = { type: 'image/png' | 'image/jpeg'; data: string; url: string };
type Line = { id: number; group: 'dialogs' | 'console' | 'requests'; text: string };

function useRoot(): HTMLElement | null {
  const [root, setRoot] = useState<HTMLElement | null>(null);
  useEffect(() => {
    let el = document.getElementById(ROOT_ID);
    if (!el) {
      el = document.createElement('div');
      el.id = ROOT_ID;
      Object.assign(el.style, { position: 'fixed', inset: '0', zIndex: '2147483000', pointerEvents: 'none' });
      document.body.appendChild(el);
    }
    const root = el;
    const stop = (e: Event) => e.stopPropagation();
    for (const type of ISOLATED_EVENTS) root.addEventListener(type, stop);
    // A focus trap pulls focus back when it leaves the dialog; that focusout starts inside the dialog, so it is caught on the way down.
    const keepFocus = (e: FocusEvent) => {
      if (e.relatedTarget instanceof Node && root.contains(e.relatedTarget)) e.stopImmediatePropagation();
    };
    window.addEventListener('focusout', keepFocus, true);
    // Modals hide everything else from screen readers; the report tools stay reachable.
    const unhide = new MutationObserver(() => {
      if (root.hasAttribute('aria-hidden')) root.removeAttribute('aria-hidden');
    });
    unhide.observe(root, { attributes: true, attributeFilter: ['aria-hidden'] });
    setRoot(root);
    return () => {
      for (const type of ISOLATED_EVENTS) root.removeEventListener(type, stop);
      window.removeEventListener('focusout', keepFocus, true);
      unhide.disconnect();
      root.remove();
    };
  }, []);
  return root;
}

const INTERACTIVE = 'button, a[href], [role="button"], [role="menuitem"], [role="tab"], [role="option"], input, select, textarea, label';

function elementAt(x: number, y: number): Element | null {
  const root = document.getElementById(ROOT_ID);
  const hit = document.elementsFromPoint(x, y).find((el) => !root?.contains(el) && el !== document.documentElement && el !== document.body);
  if (!hit) return null;
  const target = hit instanceof SVGElement ? (hit.closest('svg')?.parentElement ?? hit) : hit;
  return target.closest(INTERACTIVE) ?? target;
}

function labelOf(el: Element): string {
  if (isPrivate(el)) return '';
  const text = el.textContent ?? '';
  return scrub(clip(el.getAttribute('aria-label') ?? el.getAttribute('title') ?? text, 80));
}

function useRects(elements: Element[]): DOMRect[] {
  const [rects, setRects] = useState<DOMRect[]>([]);
  const last = useRef('');
  useEffect(() => {
    let frame = 0;
    const tick = () => {
      const next = elements.map((el) => el.getBoundingClientRect());
      const key = next.map((r) => `${r.x},${r.y},${r.width},${r.height}`).join('|') + elements.map((e) => e.isConnected).join();
      if (key !== last.current) {
        last.current = key;
        setRects(next);
      }
      frame = requestAnimationFrame(tick);
    };
    tick();
    return () => cancelAnimationFrame(frame);
  }, [elements]);
  return rects;
}

function boxStyle(r: { left: number; top: number; width: number; height: number }) {
  return { top: r.top - 2, left: r.left - 2, width: r.width + 4, height: r.height + 4 };
}

/** The visible window as pixels, with private parts in grey and the user's boxes in black. */
async function capture(root: HTMLElement, hidden: Box[]): Promise<Shot | null> {
  const masks = privateBoxes(root);
  const canvas = await domToCanvas(document.body, {
    width: window.innerWidth,
    height: window.innerHeight,
    scale: Math.min(window.devicePixelRatio || 1, 2),
    backgroundColor: getComputedStyle(document.body).backgroundColor,
    // Scroll locks restyle <body> with margins that the copy would otherwise show as an offset.
    style: { margin: '0' },
    features: { restoreScrollPosition: true },
    filter: (node) => !(node instanceof Element && node.hasAttribute(CHROME)),
    onCloneEachNode: (node) => {
      if (node instanceof HTMLInputElement || node instanceof HTMLTextAreaElement) {
        node.value = '';
        node.setAttribute('value', '');
        if (node instanceof HTMLTextAreaElement) node.textContent = '';
      }
    },
  });
  const context = canvas.getContext('2d');
  if (!context) return null;
  const ratio = canvas.width / window.innerWidth;
  const paint = (boxes: Box[], color: string) => {
    context.fillStyle = color;
    for (const b of boxes)
      context.fillRect(Math.floor(b.x * ratio), Math.floor(b.y * ratio), Math.ceil(b.width * ratio), Math.ceil(b.height * ratio));
  };
  paint(masks, MASK);
  paint(hidden, '#000');
  const encode = (source: HTMLCanvasElement, type: Shot['type'], quality?: number): Shot => {
    const url = source.toDataURL(type, quality);
    return { type, data: url.slice(url.indexOf(',') + 1), url };
  };
  let shot = encode(canvas, 'image/png');
  if (shot.data.length > MAX_SCREENSHOT) shot = encode(canvas, 'image/jpeg', 0.8);
  if (shot.data.length > MAX_SCREENSHOT) {
    const small = document.createElement('canvas');
    small.width = window.innerWidth;
    small.height = window.innerHeight;
    small.getContext('2d')?.drawImage(canvas, 0, 0, small.width, small.height);
    shot = encode(small, 'image/jpeg', 0.7);
  }
  return shot.data.length > MAX_SCREENSHOT ? null : shot;
}

type Draft = {
  title: string;
  description: string;
  notes: Note[];
  shot: Shot | null;
  lines: Line[];
  route: string;
};

/** Report a Bug: mark the page, review exactly what is sent, then finish on the relay in a new window. */
export default function BugReport({ onClose }: { onClose: () => void }) {
  const root = useRoot();
  const [draft, setDraft] = useState<Draft | null>(null);
  if (!root) return null;
  // The marking stays mounted under the review, so Back returns to the same comments and boxes.
  return createPortal(
    <>
      <Mark root={root} hidden={!!draft} onClose={onClose} onReview={setDraft} />
      {draft && <Review draft={draft} onBack={() => setDraft(null)} onClose={onClose} />}
    </>,
    root,
  );
}

function Mark({
  root,
  hidden,
  onClose,
  onReview,
}: {
  root: HTMLElement;
  hidden: boolean;
  onClose: () => void;
  onReview: (draft: Draft) => void;
}) {
  const location = useLocation();
  const [mode, setMode] = useState<Mode>('comment');
  const [hover, setHover] = useState<Element | null>(null);
  const [notes, setNotes] = useState<Note[]>([]);
  const [editing, setEditing] = useState<number | null>(null);
  const [boxes, setBoxes] = useState<(Box & { id: number })[]>([]);
  const [dragging, setDragging] = useState<{ x: number; y: number; x2: number; y2: number } | null>(null);
  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [withScreenshot, setWithScreenshot] = useState(true);
  const [capturing, setCapturing] = useState(false);
  const [problem, setProblem] = useState('');
  const nextId = useRef(1);

  const elements = useMemo(() => notes.map((n) => n.el), [notes]);
  const rects = useRects(elements);
  const hoverRects = useRects(useMemo(() => (hover && mode === 'comment' ? [hover] : []), [hover, mode]));

  const onEscape = useCallback(() => {
    if (hidden) return;
    if (editing !== null) {
      setNotes((list) => list.filter((n) => n.id !== editing || n.text !== ''));
      setEditing(null);
    } else if (mode !== 'paused') setMode('paused');
    else onClose();
  }, [editing, mode, onClose, hidden]);
  // Radix closes dialogs on Escape from a capture listener on document, so Escape is claimed earlier, on window.
  const escapeRef = useRef(onEscape);
  useLayoutEffect(() => {
    escapeRef.current = onEscape;
  });
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      e.preventDefault();
      e.stopImmediatePropagation();
      escapeRef.current();
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, []);

  const pick = (x: number, y: number) => {
    const el = elementAt(x, y);
    if (!el) return;
    const existing = notes.find((n) => n.el === el);
    if (existing) return setEditing(existing.id);
    if (notes.filter((n) => n.text).length >= 30) return;
    const id = nextId.current++;
    setNotes((list) => [
      ...list.filter((n) => n.text !== '' || n.id !== editing),
      { id, el, text: '', label: labelOf(el), tag: el.tagName.toLowerCase() },
    ]);
    setEditing(id);
  };

  const saveNote = (id: number, text: string) => {
    const clean = text.trim().slice(0, 500);
    setNotes((list) => (clean === '' ? list.filter((n) => n.id !== id) : list.map((n) => (n.id === id ? { ...n, text: clean } : n))));
    setEditing(null);
  };

  const startBox = (e: ReactPointerEvent) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    setDragging({ x: e.clientX, y: e.clientY, x2: e.clientX, y2: e.clientY });
  };
  const moveBox = (e: ReactPointerEvent) => dragging && setDragging({ ...dragging, x2: e.clientX, y2: e.clientY });
  const endBox = () => {
    if (!dragging) return;
    const box = {
      id: nextId.current++,
      x: Math.min(dragging.x, dragging.x2),
      y: Math.min(dragging.y, dragging.y2),
      width: Math.abs(dragging.x2 - dragging.x),
      height: Math.abs(dragging.y2 - dragging.y),
    };
    if (box.width > 4 && box.height > 4) setBoxes((list) => [...list, box]);
    setDragging(null);
  };

  const review = async () => {
    if (!description.trim()) {
      setProblem(t('Describe the problem first.'));
      return;
    }
    setProblem('');
    setEditing(null);
    setHover(null);
    let shot: Shot | null = null;
    if (withScreenshot) {
      setCapturing(true);
      await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
      try {
        shot = await capture(root, boxes);
      } catch (error) {
        console.warn('The bug report screenshot failed', error);
      } finally {
        setCapturing(false);
      }
    }
    const { console: consoleLines, requests } = diagnostics();
    let id = 0;
    const lines: Line[] = [
      ...openDialogTitles().map((text) => ({ id: id++, group: 'dialogs' as const, text: scrub(text) })),
      ...consoleLines.map((text) => ({ id: id++, group: 'console' as const, text: scrub(text) })),
      ...requests.map((text) => ({ id: id++, group: 'requests' as const, text: scrub(text) })),
    ].filter((l) => l.text.trim());
    onReview({
      title: title.trim(),
      description: description.trim(),
      notes: notes.filter((n) => n.text),
      shot,
      lines,
      route: routePattern(location.pathname),
    });
  };

  const editingIndex = notes.findIndex((n) => n.id === editing);
  const editingNote = notes[editingIndex];
  const written = notes.filter((n) => n.text);
  const MODES: [Mode, typeof Bug, string, string][] = [
    ['comment', MessageSquare, t('Comment'), t('Click an element, then say what is wrong.')],
    ['hide', EyeOff, t('Hide'), t('Drag over anything private to black it out.')],
    ['paused', Pause, t('Pause'), t('Use the page as usual, for example to open a menu or dialog first.')],
  ];

  if (hidden) return null;
  return (
    <>
      {mode === 'comment' && !capturing && (
        <div
          {...chrome}
          className="pointer-events-auto fixed inset-0 cursor-crosshair"
          onPointerMove={(e) => setHover(elementAt(e.clientX, e.clientY))}
          onPointerLeave={() => setHover(null)}
          onClick={(e) => pick(e.clientX, e.clientY)}
        />
      )}
      {mode === 'hide' && !capturing && (
        <div
          {...chrome}
          className="pointer-events-auto fixed inset-0 cursor-cell touch-none"
          onPointerDown={startBox}
          onPointerMove={moveBox}
          onPointerUp={endBox}
        />
      )}
      {hover && hoverRects[0] && mode === 'comment' && !capturing && (
        <div
          {...chrome}
          className="pointer-events-none fixed rounded-sm border-2 border-primary/70 bg-primary/10"
          style={boxStyle(hoverRects[0])}
        />
      )}

      {boxes.map((b, i) => (
        <div key={b.id} {...chrome} className="fixed bg-black" style={{ left: b.x, top: b.y, width: b.width, height: b.height }}>
          {!capturing && (
            <button
              type="button"
              className="pointer-events-auto absolute -end-2 -top-2 flex size-5 items-center justify-center rounded-full bg-background text-foreground shadow"
              onClick={() => setBoxes((list) => list.filter((x) => x.id !== b.id))}
              aria-label={t('Remove box {number}', { number: i + 1 })}
            >
              <X className="size-3" />
            </button>
          )}
        </div>
      ))}
      {dragging && (
        <div
          {...chrome}
          className="pointer-events-none fixed bg-black/70"
          style={{
            left: Math.min(dragging.x, dragging.x2),
            top: Math.min(dragging.y, dragging.y2),
            width: Math.abs(dragging.x2 - dragging.x),
            height: Math.abs(dragging.y2 - dragging.y),
          }}
        />
      )}

      {notes.map((note, i) => {
        const r = rects[i];
        if (!r || !note.el.isConnected) return null;
        return (
          <div key={note.id}>
            <div className="pointer-events-none fixed rounded-sm border-2 border-amber-500 bg-amber-400/10" style={boxStyle(r)} />
            <button
              type="button"
              className="pointer-events-auto fixed flex size-6 -translate-x-1/2 -translate-y-1/2 items-center justify-center rounded-full border-2 border-white bg-amber-500 text-xs font-semibold text-white shadow-md"
              style={{ top: Math.max(12, r.top), left: Math.max(12, r.left) }}
              onClick={() => setEditing(note.id)}
              aria-label={note.text || t('Comment')}
            >
              {i + 1}
            </button>
          </div>
        );
      })}

      {editingNote && rects[editingIndex] && !capturing && (
        <NoteEditor
          key={editingNote.id}
          number={editingIndex + 1}
          note={editingNote}
          rect={rects[editingIndex]!}
          onSave={(text) => saveNote(editingNote.id, text)}
        />
      )}

      <div
        {...chrome}
        role="dialog"
        aria-label={t('Report a Bug')}
        className={cn(
          'pointer-events-auto fixed end-4 bottom-4 w-[min(400px,calc(100vw-2rem))] rounded-xl border bg-popover text-popover-foreground shadow-2xl',
          capturing && 'invisible',
        )}
      >
        <div className="flex items-center gap-2 border-b px-4 py-2.5">
          <Bug className="size-4 text-primary" />
          <span className="text-sm font-medium">{t('Report a Bug')}</span>
          <Button variant="ghost" size="icon-sm" className="ms-auto" onClick={onClose} aria-label={t('Close')} disabled={capturing}>
            <X />
          </Button>
        </div>
        <div className="grid gap-3 p-4">
          <div className="grid grid-cols-3 gap-1 rounded-lg bg-muted p-1">
            {MODES.map(([value, Icon, label]) => (
              <button
                key={value}
                type="button"
                aria-pressed={mode === value}
                onClick={() => setMode(value)}
                className={cn(
                  'flex items-center justify-center gap-1.5 rounded-md px-2 py-1.5 text-xs font-medium text-muted-foreground',
                  mode === value && 'bg-background text-foreground shadow-sm',
                )}
              >
                <Icon className="size-3.5" /> {label}
              </button>
            ))}
          </div>
          <p className="text-xs text-muted-foreground">
            {MODES.find(([value]) => value === mode)?.[3]} {mode !== 'paused' && t('Press Esc to pause.')}
          </p>

          {written.length > 0 && (
            <ol className="grid max-h-32 gap-1 overflow-y-auto">
              {notes.map((n, i) =>
                n.text ? (
                  <li key={n.id} className="flex items-start gap-2 rounded-md px-1 py-0.5 text-sm hover:bg-accent">
                    <span className="mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-full bg-amber-500 text-[10px] font-semibold text-white">
                      {i + 1}
                    </span>
                    <button type="button" className="min-w-0 flex-1 truncate text-start" onClick={() => setEditing(n.id)}>
                      {n.text}
                    </button>
                    <button
                      type="button"
                      className="text-muted-foreground hover:text-destructive"
                      onClick={() => setNotes((list) => list.filter((x) => x.id !== n.id))}
                      aria-label={t('Remove comment {number}', { number: i + 1 })}
                    >
                      <X className="size-3.5" />
                    </button>
                  </li>
                ) : null,
              )}
            </ol>
          )}

          <Input placeholder={t('Title (optional)')} value={title} onChange={(e) => setTitle(e.target.value)} maxLength={120} />
          <Textarea
            placeholder={t('What went wrong, and what did you expect?')}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={3}
            maxLength={10_000}
          />
          <label className="flex items-start gap-2 text-xs">
            <Checkbox className="mt-0.5" checked={withScreenshot} onCheckedChange={(v) => setWithScreenshot(v === true)} />
            <span className="grid gap-0.5">
              <span>{t('Attach a Screenshot')}</span>
              <span className="text-muted-foreground">{t('Typed values and private details are covered in grey automatically.')}</span>
            </span>
          </label>
          {problem && <p className="text-xs text-destructive">{problem}</p>}
          <div className="flex items-center gap-2">
            <Button variant="ghost" onClick={onClose} disabled={capturing}>
              {t('Cancel')}
            </Button>
            <Button className="ms-auto flex-1" onClick={review} disabled={capturing}>
              {capturing && <Loader2 className="animate-spin" />}
              {capturing ? t('Taking the screenshot…') : t('Review the Report')}
            </Button>
          </div>
        </div>
      </div>
    </>
  );
}

function NoteEditor({ number, note, rect, onSave }: { number: number; note: Note; rect: DOMRect; onSave: (text: string) => void }) {
  const [text, setText] = useState(note.text);
  const width = 288;
  const below = rect.bottom + 140 < window.innerHeight;
  const top = below ? rect.bottom + 8 : Math.max(8, rect.top - 132);
  const left = Math.min(Math.max(8, rect.left), window.innerWidth - width - 8);
  return (
    <div
      {...chrome}
      className="pointer-events-auto fixed grid grid-cols-1 gap-2 rounded-lg border bg-popover p-2.5 text-popover-foreground shadow-xl"
      style={{ top, left, width }}
    >
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        <span className="flex size-5 items-center justify-center rounded-full bg-amber-500 text-[10px] font-semibold text-white">
          {number}
        </span>
        <span className="truncate font-mono">
          &lt;{note.tag}&gt; {note.label}
        </span>
      </div>
      <Textarea
        autoFocus
        rows={2}
        value={text}
        placeholder={t('What is wrong here?')}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            onSave(text);
          }
        }}
        maxLength={500}
      />
      <div className="flex justify-end gap-1.5">
        <Button variant="ghost" size="sm" onClick={() => onSave('')}>
          {t('Remove')}
        </Button>
        <Button size="sm" onClick={() => onSave(text)}>
          {t('Save')}
        </Button>
      </div>
    </div>
  );
}

const GROUPS: Record<Line['group'], string> = {
  dialogs: msg('Open Dialogs'),
  console: msg('Recent Console Errors'),
  requests: msg('Recent Failed Requests'),
};

function Review({ draft, onBack, onClose }: { draft: Draft; onBack: () => void; onClose: () => void }) {
  const { language } = useLanguage();
  const { user } = useSession();
  const [title, setTitle] = useState(draft.title);
  const [description, setDescription] = useState(draft.description);
  const [notes, setNotes] = useState(draft.notes);
  const [lines, setLines] = useState(draft.lines);
  const [shot, setShot] = useState(draft.shot);
  const [consent, setConsent] = useState(false);
  const [sending, setSending] = useState(false);
  const [problem, setProblem] = useState('');
  const [sent, setSent] = useState<{ url: string; opened: boolean } | null>(null);
  const browser = browserName();
  const viewport = { width: window.innerWidth, height: window.innerHeight, dpr: Math.round((window.devicePixelRatio || 1) * 100) / 100 };

  const send = async () => {
    // Opened now, while the click still counts, so popup blockers allow it; the relay's address follows.
    const win = window.open('about:blank', '_blank');
    if (win) win.opener = null;
    setSending(true);
    setProblem('');
    const pick = (group: Line['group']) => lines.filter((l) => l.group === group && l.text.trim()).map((l) => l.text.trim());
    const input: ServiceInput<'bugreports.prepare'> = {
      title: title.trim(),
      description: description.trim(),
      route: draft.route,
      language,
      browser,
      viewport,
      dialogs: pick('dialogs'),
      comments: notes.map((n) => ({ text: n.text, label: n.label, tag: n.tag })),
      console: pick('console'),
      requests: pick('requests'),
      screenshot: shot ? { type: shot.type, data: shot.data } : null,
      consent,
    };
    try {
      const { continueUrl } = await call('bugreports.prepare', input);
      if (win && !win.closed) win.location.href = continueUrl;
      setSent({ url: continueUrl, opened: !!win && !win.closed });
    } catch (error) {
      win?.close();
      setProblem(errorMessage(error));
    } finally {
      setSending(false);
    }
  };

  return (
    <div
      {...chrome}
      className="pointer-events-auto fixed inset-0 overflow-y-auto bg-black/50 p-4"
      role="dialog"
      aria-modal="true"
      aria-label={t('Check Your Report')}
    >
      <div
        className={cn(
          'mx-auto my-6 grid w-full gap-5 rounded-xl border bg-popover p-5 text-popover-foreground shadow-2xl',
          sent ? 'max-w-2xl' : 'max-w-6xl',
        )}
      >
        {sent ? (
          <div className="grid gap-4">
            <h2 className="text-lg font-semibold">{t('Finish in the New Window')}</h2>
            <p className="text-sm">{t('Check the report there, sign in with GitHub and publish it. You can close this.')}</p>
            <p className="text-sm text-muted-foreground">
              {sent.opened ? t('No window opened?') : t('Your browser blocked the new window.')}{' '}
              <a
                href={sent.url}
                target="_blank"
                rel="noopener noreferrer"
                className="font-medium text-primary underline underline-offset-4"
              >
                {t('Open the Report')}
              </a>
            </p>
            <div className="flex justify-end">
              <Button onClick={onClose}>{t('Close')}</Button>
            </div>
          </div>
        ) : (
          <>
            <div className="grid gap-1">
              <h2 className="text-lg font-semibold">{t('Check Your Report')}</h2>
              <p className="text-sm text-muted-foreground">
                {t('This is exactly what is sent. Remove anything you do not want to share.')}
              </p>
            </div>

            <div className="grid gap-5 lg:grid-cols-[minmax(0,3fr)_minmax(0,2fr)] lg:items-start">
              {shot ? (
                <figure className="grid gap-2 lg:sticky lg:top-4">
                  <img src={shot.url} alt={t('The screenshot that will be published')} className="w-full rounded-md border" />
                  <div className="flex justify-end">
                    <Button variant="outline" size="sm" onClick={() => setShot(null)}>
                      {t('Leave Out the Screenshot')}
                    </Button>
                  </div>
                </figure>
              ) : (
                <div className="rounded-md border border-dashed p-6 text-center text-sm text-muted-foreground">{t('No Screenshot')}</div>
              )}

              <div className="grid gap-5">
                <div className="grid gap-2">
                  <Input
                    value={title}
                    onChange={(e) => setTitle(e.target.value)}
                    placeholder={t('Title (optional)')}
                    maxLength={120}
                    aria-label={t('Title')}
                  />
                  <Textarea
                    value={description}
                    onChange={(e) => setDescription(e.target.value)}
                    rows={4}
                    maxLength={10_000}
                    aria-label={t('Description')}
                  />
                </div>

                {notes.length > 0 && (
                  <section className="grid gap-2">
                    <h3 className="text-sm font-medium">{t('Comments on the Page')}</h3>
                    <ol className="grid gap-1.5">
                      {notes.map((n, i) => (
                        <li key={n.id} className="flex items-center gap-2">
                          <span className="flex size-5 shrink-0 items-center justify-center rounded-full bg-amber-500 text-[10px] font-semibold text-white">
                            {i + 1}
                          </span>
                          <Input
                            value={n.text}
                            maxLength={500}
                            onChange={(e) => setNotes((list) => list.map((x) => (x.id === n.id ? { ...x, text: e.target.value } : x)))}
                            aria-label={t('Comment {number}', { number: i + 1 })}
                          />
                          <Input
                            value={n.label}
                            maxLength={120}
                            className="w-40 font-mono text-xs"
                            onChange={(e) => setNotes((list) => list.map((x) => (x.id === n.id ? { ...x, label: e.target.value } : x)))}
                            aria-label={t('Element')}
                          />
                          <Button
                            variant="ghost"
                            size="icon-sm"
                            onClick={() => setNotes((list) => list.filter((x) => x.id !== n.id))}
                            aria-label={t('Remove comment {number}', { number: i + 1 })}
                          >
                            <X />
                          </Button>
                        </li>
                      ))}
                    </ol>
                  </section>
                )}

                <section className="grid gap-2">
                  <h3 className="text-sm font-medium">{t('Technical Details')}</h3>
                  <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1 text-sm">
                    <dt className="text-muted-foreground">OpenSign</dt>
                    <dd className="font-mono text-xs leading-5">{user?.version}</dd>
                    <dt className="text-muted-foreground">{t('Page')}</dt>
                    <dd className="font-mono text-xs leading-5">{draft.route}</dd>
                    <dt className="text-muted-foreground">{t('Browser')}</dt>
                    <dd>{browser}</dd>
                    <dt className="text-muted-foreground">{t('Window Size')}</dt>
                    <dd className="tabular-nums">
                      {viewport.width}×{viewport.height} @{viewport.dpr}x
                    </dd>
                    <dt className="text-muted-foreground">{t('Language')}</dt>
                    <dd>{language}</dd>
                  </dl>
                  {(Object.keys(GROUPS) as Line['group'][]).map((group) => {
                    const items = lines.filter((l) => l.group === group);
                    if (!items.length) return null;
                    return (
                      <div key={group} className="grid gap-1">
                        <h4 className="text-xs font-medium text-muted-foreground">{t(GROUPS[group])}</h4>
                        {items.map((l) => (
                          <div key={l.id} className="flex items-center gap-2">
                            <Input
                              value={l.text}
                              maxLength={500}
                              className="font-mono text-xs"
                              onChange={(e) => setLines((list) => list.map((x) => (x.id === l.id ? { ...x, text: e.target.value } : x)))}
                              aria-label={t(GROUPS[group])}
                            />
                            <Button
                              variant="ghost"
                              size="icon-sm"
                              onClick={() => setLines((list) => list.filter((x) => x.id !== l.id))}
                              aria-label={t('Remove Line')}
                            >
                              <X />
                            </Button>
                          </div>
                        ))}
                      </div>
                    );
                  })}
                </section>

                <section className="grid gap-2 rounded-lg border border-amber-500/40 bg-amber-500/5 p-4">
                  <h3 className="font-medium">{t('Your report will be public')}</h3>
                  <label className="flex items-start gap-2.5 text-sm">
                    <Checkbox className="mt-0.5" checked={consent} onCheckedChange={(v) => setConsent(v === true)} />
                    <span>
                      {t(
                        'I understand that this report and its screenshot will be published on GitHub, where anyone can see them, and that copies may stay visible even after they are deleted. I have checked that nothing private is on screen: no names, email addresses, document contents or other personal or confidential information.',
                      )}
                    </span>
                  </label>
                </section>

                {problem && <p className="text-sm text-destructive">{problem}</p>}
                <div className="flex items-center gap-2">
                  <Button variant="ghost" onClick={onBack} disabled={sending}>
                    {t('Back')}
                  </Button>
                  <Button className="ms-auto" onClick={send} disabled={!consent || !description.trim() || sending}>
                    {sending && <Loader2 className="animate-spin" />}
                    {t('Continue on GitHub')}
                  </Button>
                </div>
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
