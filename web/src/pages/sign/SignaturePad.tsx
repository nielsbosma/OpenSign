import { Eraser } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';

export type Signature = { imagePngBase64: string | null; typedName: string | null };

/** Draw with a finger, pen or mouse, or type the name. Drawn strokes are exported as a PNG with a transparent background. */
export function SignaturePad({ defaultName, onChange }: { defaultName: string; onChange: (s: Signature | null) => void }) {
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
    if (mode === 'type') onChange(typed.trim() ? { imagePngBase64: null, typedName: typed.trim() } : null);
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
        <TabsTrigger value="draw">Draw</TabsTrigger>
        <TabsTrigger value="type">Type</TabsTrigger>
      </TabsList>
      <TabsContent value="draw" forceMount className="data-[state=inactive]:hidden">
        <div className="relative">
          <canvas
            ref={canvas}
            onPointerDown={down}
            onPointerMove={move}
            onPointerUp={up}
            onPointerCancel={up}
            aria-label="Signature drawing area"
            className="h-36 w-full touch-none rounded-md border bg-white"
          />
          <div className="pointer-events-none absolute inset-x-6 bottom-8 border-b border-dashed border-zinc-300" />
          {drawn && (
            <Button type="button" variant="ghost" size="sm" onClick={clear} className="absolute top-1 right-1 text-zinc-600">
              <Eraser /> Clear
            </Button>
          )}
        </div>
      </TabsContent>
      <TabsContent value="type">
        <Input value={typed} onChange={(e) => setTyped(e.target.value)} aria-label="Your name" />
        {typed.trim() && (
          <div className="mt-2 flex h-20 items-center rounded-md border bg-white px-4 font-serif text-3xl text-indigo-950 italic">
            {typed}
          </div>
        )}
      </TabsContent>
    </Tabs>
  );
}
