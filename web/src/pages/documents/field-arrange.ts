export type Box = { key: string; x: number; y: number; width: number; height: number };
export type Alignment = 'left' | 'centerX' | 'right' | 'top' | 'centerY' | 'bottom';
export type Patch = Record<string, { x?: number; y?: number }>;
export type Guide = { axis: 'x' | 'y'; at: number };

/** Lines up the boxes on the outermost edge, or on the middle of the area they cover. */
export function align(boxes: Box[], how: Alignment): Patch {
  const left = Math.min(...boxes.map((b) => b.x));
  const right = Math.max(...boxes.map((b) => b.x + b.width));
  const top = Math.min(...boxes.map((b) => b.y));
  const bottom = Math.max(...boxes.map((b) => b.y + b.height));
  const patch: Patch = {};
  for (const b of boxes) {
    if (how === 'left') patch[b.key] = { x: left };
    else if (how === 'right') patch[b.key] = { x: right - b.width };
    else if (how === 'centerX') patch[b.key] = { x: (left + right - b.width) / 2 };
    else if (how === 'top') patch[b.key] = { y: top };
    else if (how === 'bottom') patch[b.key] = { y: bottom - b.height };
    else patch[b.key] = { y: (top + bottom - b.height) / 2 };
  }
  return patch;
}

/** Equal gaps between the boxes; the first and last stay where they are. */
export function distribute(boxes: Box[], axis: 'x' | 'y'): Patch {
  const size = axis === 'x' ? 'width' : 'height';
  const sorted = [...boxes].sort((a, b) => a[axis] - b[axis]);
  const first = sorted[0];
  const last = sorted[sorted.length - 1];
  if (!first || !last || sorted.length < 3) return {};
  const used = sorted.reduce((sum, b) => sum + b[size], 0);
  const gap = (last[axis] + last[size] - first[axis] - used) / (sorted.length - 1);
  const patch: Patch = {};
  let at = first[axis];
  for (const b of sorted) {
    patch[b.key] = { [axis]: at };
    at += b[size] + gap;
  }
  return patch;
}

const closest = (lines: number[], targets: number[], threshold: number) => {
  let best: { offset: number; at: number } | null = null;
  for (const line of lines) {
    for (const target of targets) {
      const offset = target - line;
      if (Math.abs(offset) <= threshold && (!best || Math.abs(offset) < Math.abs(best.offset))) best = { offset, at: target };
    }
  }
  return best;
};

/** How far to nudge a moving box so its edges or middle meet those of another box, and the lines to draw there. */
export function snap(box: Omit<Box, 'key'>, others: Box[], thresholdX: number, thresholdY: number) {
  const linesX = (b: Omit<Box, 'key'>) => [b.x, b.x + b.width / 2, b.x + b.width];
  const linesY = (b: Omit<Box, 'key'>) => [b.y, b.y + b.height / 2, b.y + b.height];
  const x = closest(linesX(box), others.flatMap(linesX), thresholdX);
  const y = closest(linesY(box), others.flatMap(linesY), thresholdY);
  const guides: Guide[] = [];
  if (x) guides.push({ axis: 'x', at: x.at });
  if (y) guides.push({ axis: 'y', at: y.at });
  return { dx: x?.offset ?? 0, dy: y?.offset ?? 0, guides };
}

/** Nudges only a box's right and bottom edges, for resizing. */
export function snapEdges(box: Omit<Box, 'key'>, others: Box[], thresholdX: number, thresholdY: number) {
  const x = closest(
    [box.x + box.width],
    others.flatMap((b) => [b.x, b.x + b.width / 2, b.x + b.width]),
    thresholdX,
  );
  const y = closest(
    [box.y + box.height],
    others.flatMap((b) => [b.y, b.y + b.height / 2, b.y + b.height]),
    thresholdY,
  );
  const guides: Guide[] = [];
  if (x) guides.push({ axis: 'x', at: x.at });
  if (y) guides.push({ axis: 'y', at: y.at });
  return { dw: x?.offset ?? 0, dh: y?.offset ?? 0, guides };
}
