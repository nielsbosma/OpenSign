import { File as FileIcon, Upload, X } from 'lucide-react';
import { useRef, useState } from 'react';
import { cn } from '@/lib/utils';

export interface FilePickerProps {
  /** The chosen files, shown under the drop zone. */
  files: File[];
  onChange: (files: File[]) => void;
  accept?: string;
  multiple?: boolean;
  disabled?: boolean;
  /** Short hint under the button, such as the accepted formats. */
  hint?: string;
  id?: string;
  'aria-label'?: string;
  className?: string;
}

/** A drop zone in place of the browser's own file input. */
export function FilePicker({ files, onChange, accept, multiple, disabled, hint, id, className, ...props }: FilePickerProps) {
  const input = useRef<HTMLInputElement>(null);
  const [over, setOver] = useState(false);
  const take = (list: FileList | null) => {
    const picked = [...(list ?? [])];
    if (picked.length) onChange(multiple ? [...files, ...picked] : picked.slice(0, 1));
  };
  return (
    <div className={cn('grid gap-2', className)}>
      <button
        type="button"
        id={id}
        aria-label={props['aria-label']}
        disabled={disabled}
        onClick={() => input.current?.click()}
        onDragOver={(e) => {
          e.preventDefault();
          setOver(true);
        }}
        onDragLeave={() => setOver(false)}
        onDrop={(e) => {
          e.preventDefault();
          setOver(false);
          if (!disabled) take(e.dataTransfer.files);
        }}
        className={cn(
          'flex flex-col items-center gap-1 rounded-lg border border-dashed px-4 py-6 text-sm outline-none transition-colors hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50',
          over && 'border-primary bg-accent',
        )}
      >
        <Upload className="size-5 text-muted-foreground" />
        <span className="font-medium">{multiple ? 'Choose Files' : 'Choose File'}</span>
        <span className="text-xs text-muted-foreground">{hint ?? 'or drag and drop here'}</span>
      </button>
      <input
        ref={input}
        type="file"
        hidden
        accept={accept}
        multiple={multiple}
        onChange={(e) => {
          take(e.target.files);
          e.target.value = '';
        }}
      />
      {files.length > 0 && (
        <ul className="grid gap-1">
          {files.map((f, i) => (
            <li key={`${f.name}-${i}`} className="flex items-center gap-2 rounded-md border bg-card px-2.5 py-1.5 text-sm">
              <FileIcon className="size-4 shrink-0 text-muted-foreground" />
              <span className="min-w-0 flex-1 truncate">{f.name}</span>
              <button
                type="button"
                className="rounded-sm p-0.5 text-muted-foreground hover:bg-accent"
                aria-label={`Remove ${f.name}`}
                onClick={() => onChange(files.filter((_, j) => j !== i))}
              >
                <X className="size-3.5" />
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
