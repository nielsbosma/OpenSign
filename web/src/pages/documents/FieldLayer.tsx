import { X } from 'lucide-react';
import { useRef } from 'react';
import type { PartyView } from '@/lib/api';
import { display } from '@/lib/field-values';
import { t } from '@/lib/i18n';
import { cn } from '@/lib/utils';
import { clamp, type EditableField, kindLabel, partyColor } from './fields';

type Drag = { key: string; mode: 'move' | 'resize'; startX: number; startY: number; field: EditableField; rect: DOMRect };

/** The fields on one page. Editable fields drag to move and resize from the corner; Delete removes the selected one (handled by the page). */
export function FieldLayer({
  page,
  fields,
  parties,
  editable,
  selected,
  onSelect,
  onChange,
  onRemove,
}: {
  page: number;
  fields: EditableField[];
  parties: PartyView[];
  editable: boolean;
  selected: string | null;
  onSelect: (key: string | null) => void;
  onChange: (key: string, patch: Partial<EditableField>, done: boolean) => void;
  onRemove: (key: string) => void;
}) {
  const drag = useRef<Drag | null>(null);

  const start = (e: React.PointerEvent, field: EditableField, mode: Drag['mode']) => {
    if (!editable) return;
    e.stopPropagation();
    e.preventDefault();
    onSelect(field.key);
    const rect = (e.currentTarget.closest('[data-page]') as HTMLElement).getBoundingClientRect();
    drag.current = { key: field.key, mode, startX: e.clientX, startY: e.clientY, field, rect };
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  };
  const move = (e: React.PointerEvent, done = false) => {
    const d = drag.current;
    if (!d) return;
    const dx = (e.clientX - d.startX) / d.rect.width;
    const dy = (e.clientY - d.startY) / d.rect.height;
    const f = d.field;
    const patch =
      d.mode === 'move'
        ? { x: clamp(f.x + dx, 0, 1 - f.width), y: clamp(f.y + dy, 0, 1 - f.height) }
        : { width: clamp(f.width + dx, 0.015, 1 - f.x), height: clamp(f.height + dy, 0.012, 1 - f.y) };
    onChange(d.key, patch, done);
    if (done) drag.current = null;
  };

  return (
    <div className="pointer-events-none absolute inset-0">
      {fields
        .filter((f) => f.page === page)
        .map((f) => {
          const color = partyColor(parties, f.partyId);
          const party = parties.find((p) => p.id === f.partyId);
          const active = selected === f.key;
          return (
            <div
              key={f.key}
              role={editable ? 'button' : undefined}
              tabIndex={editable ? 0 : undefined}
              title={
                party
                  ? t('{field} for {name}', { field: kindLabel(f.kind), name: party.name })
                  : t('{field} for the sender', { field: kindLabel(f.kind) })
              }
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
                      : `${f.label || kindLabel(f.kind)}${f.required === false ? '' : ' *'}`}
                </span>
              )}
              {editable && active && (
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
    </div>
  );
}
