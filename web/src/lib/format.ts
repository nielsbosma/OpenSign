import { APP_LANGUAGES, currentLanguage, labels, msg } from '@/lib/i18n';

const formatters = new Map<string, Intl.DateTimeFormat>();

function formatter(options: Intl.DateTimeFormatOptions) {
  const key = `${currentLanguage()}:${JSON.stringify(options)}`;
  let f = formatters.get(key);
  if (!f) {
    f = new Intl.DateTimeFormat(currentLanguage(), options);
    formatters.set(key, f);
  }
  return f;
}

export function formatDateTime(value?: string | null) {
  return value ? formatter({ dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value)) : '';
}

export function formatDate(value?: string | null) {
  return value ? formatter({ dateStyle: 'medium' }).format(new Date(value)) : '';
}

export function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export const STATUS_LABELS = labels({
  draft: msg('Draft'),
  pending: msg('Out for signing'),
  completed: msg('Signed'),
  declined: msg('Declined'),
  canceled: msg('Canceled'),
  expired: msg('Expired'),
});

export const STATUS_TONES: Record<string, 'default' | 'secondary' | 'destructive' | 'outline'> = {
  draft: 'outline',
  pending: 'secondary',
  completed: 'default',
  declined: 'destructive',
  canceled: 'outline',
  expired: 'outline',
};

export const PARTY_STATUS_LABELS = labels({
  waiting: msg('Waiting'),
  notified: msg('Notified'),
  viewed: msg('Viewed'),
  signed: msg('Signed'),
  declined: msg('Declined'),
  canceled: msg('Canceled'),
});

export const AUTH_LABELS = labels({
  none: msg('None'),
  standard: msg('Link only'),
  sms_pin: msg('SMS code'),
  se_bankid: msg('Swedish BankID'),
  freja: msg('Freja eID'),
  broker: msg('eID broker'),
});

export const ROLE_LABELS = labels({
  admin: msg('Admin'),
  manager: msg('Manager'),
  sender: msg('Sender'),
  auditor: msg('Auditor'),
  viewer: msg('Viewer'),
});

const LANGUAGE_NAMES = labels({
  en: msg('English'),
  zh: msg('Chinese'),
  hi: msg('Hindi'),
  es: msg('Spanish'),
  fr: msg('French'),
  ar: msg('Arabic'),
  bn: msg('Bengali'),
  pt: msg('Portuguese'),
  ru: msg('Russian'),
  ur: msg('Urdu'),
  sv: msg('Swedish'),
});

/** A language's name in the app's language, with its own name when that differs. */
export function languageName(code: string) {
  const own = APP_LANGUAGES.find(([c]) => c === code)?.[1];
  const name = LANGUAGE_NAMES[code] ?? code;
  return own && own !== name ? `${name} (${own})` : name;
}

/** Languages signers can get their emails, SMS and signing page in. */
export const languages = (): [string, string][] => APP_LANGUAGES.map(([code]) => [code, languageName(code)]);

export const DELIVERY_LABELS = labels({
  email: msg('Email'),
  sms: msg('SMS'),
  link: msg('Link (you hand it over)'),
  in_person: msg('In Person (on your device)'),
  none: msg('None'),
});

export function base64ToBytes(base64: string) {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

export function saveBytes(bytes: Uint8Array<ArrayBuffer> | ArrayBuffer, name: string, type = 'application/pdf') {
  const url = URL.createObjectURL(new Blob([bytes], { type }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
