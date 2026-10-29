import { Eraser } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useTexts } from './texts';

export type Signature = { imagePngBase64: string | null; typedName: string | null };

// The sealed PDF types names in a standard font that only covers Western European letters; other names are drawn here.
const NON_LATIN = /[^\u0020-\u00ff]/;

function typedImage(name: string) {
  const c = document.createElement('canvas');
  c.width = 900;
  c.height = 220;
  const ctx = c.getContext('2d');
  if (!ctx) return null;
  let size = 96;
  ctx.font = `italic ${size}px serif`;
  const width = ctx.measureText(name).width;
  if (width > c.width - 40) size = Math.max(24, Math.floor((size * (c.width - 40)) / width));
  ctx.font = `italic ${size}px serif`;
  ctx.fillStyle = '#1e1b4b';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.fillText(name, c.width / 2, c.height / 2);
  return c.toDataURL('image/png').split(',')[1]!;
}

/** Draw with a finger, pen or mouse, or type the name. Drawn strokes are exported as a PNG with a transparent background. */
export function SignaturePad({ defaultName, onChange }: { defaultName: string; onChange: (s: Signature | null) => void }) {
  const t = useTexts();
  const canvas = useRef<HTMLCanvasElement>(null);
  const drawing = useRef(false);
  const [mode, setMode] = useState<'draw' | 'type'>('draw');
  const [drawn, setDrawn] = useState(false);
  const [typed, setTyped] = useState(defaultName);

  useEffect(() => {
    const c = canvas.current;
    if (!c) return;
    const ratio = window.devicePixelRatio || 1;
    c.width = c.offsetWidth * ratio;
    c.height = c.offsetHeight * ratio;
    const ctx = c.getContext('2d')!;
    ctx.scale(ratio, ratio);
    ctx.lineWidth = 2.2;
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    ctx.strokeStyle = '#1e1b4b';
  }, []);

  // biome-ignore lint/correctness/useExhaustiveDependencies: report when the signature itself changes
  useEffect(() => {
    const name = typed.trim();
    if (mode === 'type') onChange(name ? { imagePngBase64: NON_LATIN.test(name) ? typedImage(name) : null, typedName: name } : null);
    else
      onChange(drawn && canvas.current ? { imagePngBase64: canvas.current.toDataURL('image/png').split(',')[1]!, typedName: null } : null);
  }, [mode, typed, drawn]);

  const point = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    return [e.clientX - rect.left, e.clientY - rect.top] as const;
  };
  const down = (e: React.PointerEvent<HTMLCanvasElement>) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    drawing.current = true;
    const ctx = e.currentTarget.getContext('2d')!;
    const [x, y] = point(e);
    ctx.beginPath();
    ctx.moveTo(x, y);
    ctx.lineTo(x + 0.1, y + 0.1);
    ctx.stroke();
  };
  const move = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (!drawing.current) return;
    const ctx = e.currentTarget.getContext('2d')!;
    const [x, y] = point(e);
    ctx.lineTo(x, y);
    ctx.stroke();
  };
  const up = () => {
    if (!drawing.current) return;
    drawing.current = false;
    setDrawn(true);
    if (canvas.current) onChange({ imagePngBase64: canvas.current.toDataURL('image/png').split(',')[1]!, typedName: null });
  };
  const clear = () => {
    const c = canvas.current;
    if (!c) return;
    c.getContext('2d')!.clearRect(0, 0, c.width, c.height);
    setDrawn(false);
  };

  return (
    <Tabs value={mode} onValueChange={(v) => setMode(v as 'draw' | 'type')}>
      <TabsList>
        <TabsTrigger value="draw">{t.draw}</TabsTrigger>
        <TabsTrigger value="type">{t.type}</TabsTrigger>
      </TabsList>
      <TabsContent value="draw" forceMount className="data-[state=inactive]:hidden">
        <div className="relative">
          <canvas
            ref={canvas}
            onPointerDown={down}
            onPointerMove={move}
            onPointerUp={up}
            onPointerCancel={up}
            aria-label={t.drawingArea}
            className="h-36 w-full touch-none rounded-md border bg-white"
          />
          <div className="pointer-events-none absolute inset-x-6 bottom-8 border-b border-dashed border-zinc-300" />
          {drawn && (
            <Button type="button" variant="ghost" size="sm" onClick={clear} className="absolute end-1 top-1 text-zinc-600">
              <Eraser /> {t.clear}
            </Button>
          )}
        </div>
      </TabsContent>
      <TabsContent value="type">
        <Input value={typed} onChange={(e) => setTyped(e.target.value)} aria-label={t.yourName} dir="auto" />
        {typed.trim() && (
          <div
            dir="auto"
            className="mt-2 flex h-20 items-center rounded-md border bg-white px-4 font-serif text-3xl text-indigo-950 italic"
          >
            {typed}
          </div>
        )}
      </TabsContent>
    </Tabs>
  );
}
