import { describe, expect, it } from 'vitest';
import { align, distribute, snap, snapEdges } from './field-arrange';

const a = { key: 'a', x: 0.1, y: 0.1, width: 0.2, height: 0.1 };
const b = { key: 'b', x: 0.4, y: 0.3, width: 0.1, height: 0.1 };
const c = { key: 'c', x: 0.8, y: 0.7, width: 0.1, height: 0.1 };

describe('align', () => {
  it('uses the outermost edge', () => {
    expect(align([a, b], 'left')).toEqual({ a: { x: 0.1 }, b: { x: 0.1 } });
    expect(align([a, b], 'right').a?.x).toBeCloseTo(0.3);
    expect(align([a, b], 'bottom').a?.y).toBeCloseTo(0.3);
  });
  it('centers on the area the boxes cover', () => {
    const patch = align([a, b], 'centerX');
    expect(patch.a?.x).toBeCloseTo(0.2);
    expect(patch.b?.x).toBeCloseTo(0.25);
  });
});

describe('distribute', () => {
  it('leaves equal gaps and keeps the ends', () => {
    const patch = distribute([c, a, b], 'x');
    expect(patch.a?.x).toBeCloseTo(0.1);
    expect(patch.c?.x).toBeCloseTo(0.8);
    expect((patch.b?.x ?? 0) - 0.3).toBeCloseTo(0.8 - ((patch.b?.x ?? 0) + 0.1));
  });
  it('needs three boxes', () => {
    expect(distribute([a, b], 'y')).toEqual({});
  });
});

describe('snap', () => {
  it('nudges to the nearest line and draws a guide', () => {
    const result = snap({ x: 0.103, y: 0.5, width: 0.1, height: 0.1 }, [a], 0.005, 0.005);
    expect(result.dx).toBeCloseTo(-0.003);
    expect(result.dy).toBe(0);
    expect(result.guides).toEqual([{ axis: 'x', at: 0.1 }]);
  });
  it('leaves a box alone outside the threshold', () => {
    expect(snap({ x: 0.6, y: 0.6, width: 0.1, height: 0.1 }, [a], 0.005, 0.005).guides).toEqual([]);
  });
  it('moves only the far edges when resizing', () => {
    const result = snapEdges({ x: 0.5, y: 0.5, width: 0.102, height: 0.05 }, [{ ...a, x: 0.6 }], 0.005, 0.005);
    expect(result.dw).toBeCloseTo(-0.002);
  });
});
