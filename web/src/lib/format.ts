const dateTime = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' });
const dateOnly = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' });

export function formatDateTime(value?: string | null) {
  return value ? dateTime.format(new Date(value)) : '';
}

export function formatDate(value?: string | null) {
  return value ? dateOnly.format(new Date(value)) : '';
}

export function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export const STATUS_LABELS: Record<string, string> = {
  draft: 'Draft',
  pending: 'Out for signing',
  completed: 'Signed',
  declined: 'Declined',
  canceled: 'Canceled',
  expired: 'Expired',
};

export const STATUS_TONES: Record<string, 'default' | 'secondary' | 'destructive' | 'outline'> = {
  draft: 'outline',
  pending: 'secondary',
  completed: 'default',
  declined: 'destructive',
  canceled: 'outline',
  expired: 'outline',
};

export const PARTY_STATUS_LABELS: Record<string, string> = {
  waiting: 'Waiting',
  notified: 'Notified',
  viewed: 'Viewed',
  signed: 'Signed',
  declined: 'Declined',
  canceled: 'Canceled',
};

export const AUTH_LABELS: Record<string, string> = {
  none: 'None',
  standard: 'Link only',
  sms_pin: 'SMS code',
  se_bankid: 'Swedish BankID',
  freja: 'Freja eID',
  broker: 'eID broker',
};

/** Languages signers can get their emails, SMS and signing page in. */
export const LANGUAGES: [string, string][] = [
  ['en', 'English'],
  ['zh', 'Chinese (中文)'],
  ['hi', 'Hindi (हिन्दी)'],
  ['es', 'Spanish (Español)'],
  ['fr', 'French (Français)'],
  ['ar', 'Arabic (العربية)'],
  ['bn', 'Bengali (বাংলা)'],
  ['pt', 'Portuguese (Português)'],
  ['ru', 'Russian (Русский)'],
  ['ur', 'Urdu (اردو)'],
  ['sv', 'Swedish (Svenska)'],
];

export const languageName = (code: string) => LANGUAGES.find(([c]) => c === code)?.[1] ?? code;

export const DELIVERY_LABELS: Record<string, string> = {
  email: 'Email',
  sms: 'SMS',
  link: 'Link (you hand it over)',
  in_person: 'In Person (on your device)',
  none: 'None',
};

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
