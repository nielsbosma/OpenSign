import { Check, ChevronsUpDown } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Command, CommandEmpty, CommandInput, CommandItem, CommandList } from '@/components/ui/command';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { t } from '@/lib/i18n';
import type { Option } from '@/lib/regions';
import { cn } from '@/lib/utils';

/** A select with a search box, for long lists such as countries and time zones. */
export function SearchSelect({
  id,
  value,
  onChange,
  options,
  placeholder,
  searchPlaceholder,
  disabled,
}: {
  id?: string;
  value: string;
  onChange: (value: string) => void;
  options: Option[];
  placeholder?: string;
  searchPlaceholder?: string;
  disabled?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const selected = options.find((o) => o.value === value);
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          id={id}
          type="button"
          variant="outline"
          role="combobox"
          aria-expanded={open}
          disabled={disabled}
          className="w-full justify-between px-3 font-normal"
        >
          {selected ? (
            <span className="flex min-w-0 items-center gap-2">
              <span className="truncate">{selected.label}</span>
              {selected.detail && <span className="text-xs text-muted-foreground">{selected.detail}</span>}
            </span>
          ) : (
            <span className="truncate text-muted-foreground">{value || placeholder || t('Choose…')}</span>
          )}
          <ChevronsUpDown className="opacity-50" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-(--radix-popover-trigger-width) p-0" align="start">
        <Command>
          <CommandInput placeholder={searchPlaceholder ?? t('Search…')} />
          <CommandList>
            <CommandEmpty>{t('Nothing found.')}</CommandEmpty>
            {options.map((o) => (
              <CommandItem
                key={o.value}
                value={`${o.label} ${o.value} ${o.detail ?? ''}`}
                onSelect={() => {
                  onChange(o.value);
                  setOpen(false);
                }}
              >
                <Check className={cn(o.value === value ? 'opacity-100' : 'opacity-0')} />
                <span className="truncate">{o.label}</span>
                {o.detail && <span className="ml-auto text-xs text-muted-foreground">{o.detail}</span>}
              </CommandItem>
            ))}
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
