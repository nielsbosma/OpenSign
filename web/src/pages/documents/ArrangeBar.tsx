import {
  AlignCenterHorizontal,
  AlignCenterVertical,
  AlignEndHorizontal,
  AlignEndVertical,
  AlignHorizontalSpaceBetween,
  AlignStartHorizontal,
  AlignStartVertical,
  AlignVerticalSpaceBetween,
  type LucideIcon,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { msg, t } from '@/lib/i18n';
import { type Alignment, align, distribute, type Patch } from './field-arrange';
import type { EditableField } from './fields';

const ALIGN: { how: Alignment; label: string; icon: LucideIcon }[] = [
  { how: 'left', label: msg('Align Left'), icon: AlignStartVertical },
  { how: 'centerX', label: msg('Align Horizontal Centers'), icon: AlignCenterVertical },
  { how: 'right', label: msg('Align Right'), icon: AlignEndVertical },
  { how: 'top', label: msg('Align Top'), icon: AlignStartHorizontal },
  { how: 'centerY', label: msg('Align Vertical Centers'), icon: AlignCenterHorizontal },
  { how: 'bottom', label: msg('Align Bottom'), icon: AlignEndHorizontal },
];
const DISTRIBUTE: { axis: 'x' | 'y'; label: string; icon: LucideIcon }[] = [
  { axis: 'x', label: msg('Distribute Horizontally'), icon: AlignHorizontalSpaceBetween },
  { axis: 'y', label: msg('Distribute Vertically'), icon: AlignVerticalSpaceBetween },
];

function Tool({ label, icon: Icon, disabled, onClick }: { label: string; icon: LucideIcon; disabled?: boolean; onClick: () => void }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon-sm" aria-label={t(label)} disabled={disabled} onClick={onClick}>
          <Icon />
        </Button>
      </TooltipTrigger>
      <TooltipContent>{t(label)}</TooltipContent>
    </Tooltip>
  );
}

/** Lines up or spaces out the selected fields. Fields only arrange with others on their own page. */
export function ArrangeBar({ fields, onArrange }: { fields: EditableField[]; onArrange: (patches: Patch) => void }) {
  const pages = [...new Set(fields.map((f) => f.page))].map((page) => fields.filter((f) => f.page === page));
  const each = (arrange: (boxes: EditableField[]) => Patch) => onArrange(Object.assign({}, ...pages.map(arrange)));
  const canDistribute = pages.some((p) => p.length > 2);
  return (
    <div className="grid gap-2 rounded-lg border p-3">
      <span className="font-medium">{t('{count} fields selected', { count: fields.length })}</span>
      <div className="flex flex-wrap items-center gap-0.5">
        {ALIGN.map((a) => (
          <Tool key={a.how} label={a.label} icon={a.icon} onClick={() => each((boxes) => align(boxes, a.how))} />
        ))}
        <span className="mx-1 h-5 w-px bg-border" />
        {DISTRIBUTE.map((d) => (
          <Tool
            key={d.axis}
            label={d.label}
            icon={d.icon}
            disabled={!canDistribute}
            onClick={() => each((boxes) => distribute(boxes, d.axis))}
          />
        ))}
      </div>
    </div>
  );
}
