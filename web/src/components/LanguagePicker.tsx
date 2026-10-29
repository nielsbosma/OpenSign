import { Languages } from 'lucide-react';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { APP_LANGUAGES, t, useLanguage } from '@/lib/i18n';

/** Switches the app's language in this browser, before anyone is signed in. */
export function LanguagePicker({ className }: { className?: string }) {
  const { language, setLanguage } = useLanguage();
  return (
    <Select value={language} onValueChange={setLanguage}>
      <SelectTrigger size="sm" className={className} aria-label={t('Language')}>
        <Languages />
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {APP_LANGUAGES.map(([code, name]) => (
          <SelectItem key={code} value={code} lang={code}>
            {name}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
