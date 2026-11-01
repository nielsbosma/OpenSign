import { X } from 'lucide-react';
import { useRef, useState } from 'react';
import type { PartyView } from '@/lib/api';
import { display } from '@/lib/field-values';
import { t } from '@/lib/i18n';
import { cn } from '@/lib/utils';
import { type Guide, snap, snapEdges } from './field-arrange';
import { clamp, type EditableField, kindLabel, partyColor } from './fields';

type Drag = {
  mode: 'move' | 'resize';
  startX: number;
  startY: number;
  key: string;
  originals: EditableField[];
  rect: DOMRect;
  moved: boolean;
};
type Marquee = { startX: number; startY: number; x: number; y: number; rect: DOMRect };

const SNAP_PIXELS = 5;

/**
 * The fields on one page. Editable fields drag to move and resize from the corner, and snap to the other fields unless Alt is held.
 * Shift-click or drag across the page selects several; Delete removes the selection (handled by the page).
 */
export function FieldLayer({
  page,
  fields,
  parties,
  editable,
  selected,
  marquee = false,
  onSelect,
  onChange,
  onRemove,
}: {
  page: number;
  fields: EditableField[];
  parties: PartyView[];
  editable: boolean;
  selected: string[];
  /** Dragging across the empty page selects the fields it touches. */
  marquee?: boolean;
  onSelect: (keys: string[]) => void;
  onChange: (patches: Record<string, Partial<EditableField>>, done: boolean) => void;
  onRemove: (key: string) => void;
}) {
  const drag = useRef<Drag | null>(null);
  const [guides, setGuides] = useState<Guide[]>([]);
  const [box, setBox] = useState<Marquee | null>(null);
  const onPage = fields.filter((f) => f.page === page);

  const start = (e: React.PointerEvent, field: EditableField, mode: Drag['mode']) => {
    if (!editable) return;
    e.stopPropagation();
    e.preventDefault();
    if (mode === 'move' && (e.shiftKey || e.ctrlKey || e.metaKey)) {
      onSelect(selected.includes(field.key) ? selected.filter((k) => k !== field.key) : [...selected, field.key]);
      return;
    }
    const keys = mode === 'move' && selected.includes(field.key) ? selected : [field.key];
    onSelect(keys);
    const rect = (e.currentTarget.closest('[data-page]') as HTMLElement).getBoundingClientRect();
    const originals = onPage.filter((f) => keys.includes(f.key));
    drag.current = { mode, startX: e.clientX, startY: e.clientY, key: field.key, originals, rect, moved: false };
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  };
  const move = (e: React.PointerEvent, done = false) => {
    const d = drag.current;
    if (!d) return;
    const rawX = (e.clientX - d.startX) / d.rect.width;
    const rawY = (e.clientY - d.startY) / d.rect.height;
    d.moved ||= rawX !== 0 || rawY !== 0;
    if (done) {
      drag.current = null;
      setGuides([]);
      if (!d.moved) {
        onSelect([d.key]);
        return;
      }
    }
    if (!d.moved) return;
    const others = e.altKey ? [] : onPage.filter((f) => !d.originals.some((o) => o.key === f.key));
    const tx = SNAP_PIXELS / d.rect.width;
    const ty = SNAP_PIXELS / d.rect.height;
    const patches: Record<string, Partial<EditableField>> = {};
    let lines: Guide[];
    if (d.mode === 'move') {
      const left = Math.min(...d.originals.map((f) => f.x));
      const top = Math.min(...d.originals.map((f) => f.y));
      const width = Math.max(...d.originals.map((f) => f.x + f.width)) - left;
      const height = Math.max(...d.originals.map((f) => f.y + f.height)) - top;
      const x = clamp(left + rawX, 0, 1 - width);
      const y = clamp(top + rawY, 0, 1 - height);
      const snapped = snap({ x, y, width, height }, others, tx, ty);
      lines = snapped.guides;
      const dx = clamp(x + snapped.dx, 0, 1 - width) - left;
      const dy = clamp(y + snapped.dy, 0, 1 - height) - top;
      for (const f of d.originals) patches[f.key] = { x: f.x + dx, y: f.y + dy };
    } else {
      const f = d.originals[0]!;
      const width = clamp(f.width + rawX, 0.015, 1 - f.x);
      const height = clamp(f.height + rawY, 0.012, 1 - f.y);
      const snapped = snapEdges({ x: f.x, y: f.y, width, height }, others, tx, ty);
      lines = snapped.guides;
      patches[f.key] = { width: clamp(width + snapped.dw, 0.015, 1 - f.x), height: clamp(height + snapped.dh, 0.012, 1 - f.y) };
    }
    if (!done) setGuides(lines);
    onChange(patches, done);
  };

  const marqueeStart = (e: React.PointerEvent) => {
    if (e.pointerType !== 'mouse' || e.button !== 0) return;
    const rect = (e.currentTarget.closest('[data-page]') as HTMLElement).getBoundingClientRect();
    const x = (e.clientX - rect.left) / rect.width;
    const y = (e.clientY - rect.top) / rect.height;
    setBox({ startX: x, startY: y, x, y, rect });
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  };
  const marqueeMove = (e: React.PointerEvent) => {
    if (!box) return;
    setBox({
      ...box,
      x: clamp((e.clientX - box.rect.left) / box.rect.width, 0, 1),
      y: clamp((e.clientY - box.rect.top) / box.rect.height, 0, 1),
    });
  };
  const area = box && {
    left: Math.min(box.startX, box.x),
    right: Math.max(box.startX, box.x),
    top: Math.min(box.startY, box.y),
    bottom: Math.max(box.startY, box.y),
  };
  const marqueeEnd = () => {
    if (!area) return;
    onSelect(
      onPage
        .filter((f) => f.x < area.right && f.x + f.width > area.left && f.y < area.bottom && f.y + f.height > area.top)
        .map((f) => f.key),
    );
    setBox(null);
  };

  return (
    <div className="pointer-events-none absolute inset-0">
      {editable && marquee && (
        <div
          className="pointer-events-auto absolute inset-0"
          onPointerDown={marqueeStart}
          onPointerMove={marqueeMove}
          onPointerUp={marqueeEnd}
        />
      )}
      {onPage.map((f) => {
        const color = partyColor(parties, f.partyId);
        const party = parties.find((p) => p.id === f.partyId);
        const active = selected.includes(f.key);
        return (
          <div
            key={f.key}
            role={editable ? 'button' : undefined}
            tabIndex={editable ? 0 : undefined}
            title={[
              party
                ? t('{field} for {name}', { field: kindLabel(f.kind), name: party.name })
                : t('{field} for the sender', { field: kindLabel(f.kind) }),
              f.formula && `= ${f.formula}`,
              f.conditions?.length && t('Shown only when its conditions hold'),
            ]
              .filter(Boolean)
              .join('\n')}
            onPointerDown={(e) => start(e, f, 'move')}
            onPointerMove={(e) => move(e)}
            onPointerUp={(e) => move(e, true)}
            className={cn(
              'pointer-events-auto absolute flex items-center overflow-hidden rounded-[3px] border text-[10px] leading-none select-none',
              editable && 'cursor-move touch-none',
              active && 'ring-2 ring-offset-1',
            )}
            style={{
              left: `${f.x * 100}%`,
              top: `${f.y * 100}%`,
              width: `${f.width * 100}%`,
              height: `${f.height * 100}%`,
              borderColor: color,
              borderStyle: f.conditions?.length ? 'dashed' : undefined,
              backgroundColor: `${color}1f`,
              color,
              ['--tw-ring-color' as string]: color,
            }}
          >
            {f.kind === 'radio' ? (
              <span
                className="mx-auto aspect-square h-[70%] rounded-full border"
                style={{ borderColor: color, backgroundColor: f.value ? color : undefined }}
              />
            ) : (
              <span className="truncate px-1 font-medium">
                {f.value && (!f.partyId || f.readOnly)
                  ? display(f.kind, f.format, f.value)
                  : f.kind === 'checkbox'
                    ? ''
                    : `${f.kind === 'formula' ? 'ƒ ' : ''}${f.label || kindLabel(f.kind)}${f.required === false || f.kind === 'formula' ? '' : ' *'}`}
              </span>
            )}
            {editable && active && selected.length === 1 && (
              <>
                <button
                  type="button"
                  aria-label={t('Remove Field')}
                  onPointerDown={(e) => e.stopPropagation()}
                  onClick={() => onRemove(f.key)}
                  className="absolute top-0 right-0 flex size-3.5 items-center justify-center bg-white/80"
                >
                  <X className="size-2.5" />
                </button>
                <span
                  onPointerDown={(e) => start(e, f, 'resize')}
                  onPointerMove={(e) => move(e)}
                  onPointerUp={(e) => move(e, true)}
                  className="absolute right-0 bottom-0 size-2.5 cursor-nwse-resize"
                  style={{ backgroundColor: color }}
                />
              </>
            )}
          </div>
        );
      })}
      {guides.map((g) => (
        <div
          key={`${g.axis}${g.at}`}
          className="absolute bg-primary"
          style={
            g.axis === 'x'
              ? { left: `${g.at * 100}%`, top: 0, bottom: 0, width: 1 }
              : { top: `${g.at * 100}%`, left: 0, right: 0, height: 1 }
          }
        />
      ))}
      {area && (
        <div
          className="absolute border border-primary bg-primary/10"
          style={{
            left: `${area.left * 100}%`,
            top: `${area.top * 100}%`,
            width: `${(area.right - area.left) * 100}%`,
            height: `${(area.bottom - area.top) * 100}%`,
          }}
        />
      )}
    </div>
  );
}
