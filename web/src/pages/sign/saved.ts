import type { Signature } from './SignaturePad';

const DEVICE_KEY = 'opensign-saved-signature';

type DeviceSignature = { name: string; signature: Signature };

/** A signature an outside signer asked this browser to remember. Only offered to someone with the same name. */
export function deviceSignature(name: string): Signature | null {
  try {
    const saved = JSON.parse(localStorage.getItem(DEVICE_KEY) ?? 'null') as DeviceSignature | null;
    if (!saved?.signature || saved.name.trim().toLowerCase() !== name.trim().toLowerCase()) return null;
    return saved.signature;
  } catch {
    return null;
  }
}

export function rememberSignature(name: string, signature: Signature) {
  try {
    localStorage.setItem(DEVICE_KEY, JSON.stringify({ name, signature } satisfies DeviceSignature));
  } catch {}
}

export function forgetSignature() {
  try {
    localStorage.removeItem(DEVICE_KEY);
  } catch {}
}
