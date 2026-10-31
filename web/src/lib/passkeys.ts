/** Browser side of passkeys: turns the server's JSON options into WebAuthn calls and back. */

type Json = Record<string, unknown>;

function fromBase64Url(value: string): ArrayBuffer {
  const base64 = value
    .replace(/-/g, '+')
    .replace(/_/g, '/')
    .padEnd(Math.ceil(value.length / 4) * 4, '=');
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes.buffer;
}

function toBase64Url(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  let binary = '';
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

export function passkeysSupported() {
  return typeof window !== 'undefined' && !!window.PublicKeyCredential && !!navigator.credentials;
}

/** Ignores the browser's "you closed the dialog" error, which is not worth a message. */
export function passkeyCanceled(e: unknown) {
  return e instanceof DOMException && (e.name === 'NotAllowedError' || e.name === 'AbortError');
}

export async function createPasskey(options: Json) {
  const user = options.user as Json;
  const publicKey = {
    ...options,
    challenge: fromBase64Url(options.challenge as string),
    user: { ...user, id: fromBase64Url(user.id as string) },
    excludeCredentials: ((options.excludeCredentials as Json[]) ?? []).map((c) => ({ ...c, id: fromBase64Url(c.id as string) })),
  } as unknown as PublicKeyCredentialCreationOptions;
  const credential = (await navigator.credentials.create({ publicKey })) as PublicKeyCredential | null;
  if (!credential) throw new DOMException('No passkey was created', 'NotAllowedError');
  const response = credential.response as AuthenticatorAttestationResponse;
  return {
    clientDataJson: toBase64Url(response.clientDataJSON),
    attestationObject: toBase64Url(response.attestationObject),
    transports: response.getTransports?.() ?? [],
  };
}

export async function getPasskey(options: Json) {
  const publicKey = {
    ...options,
    challenge: fromBase64Url(options.challenge as string),
    allowCredentials: ((options.allowCredentials as Json[]) ?? []).map((c) => ({ ...c, id: fromBase64Url(c.id as string) })),
  } as unknown as PublicKeyCredentialRequestOptions;
  const credential = (await navigator.credentials.get({ publicKey })) as PublicKeyCredential | null;
  if (!credential) throw new DOMException('No passkey was chosen', 'NotAllowedError');
  const response = credential.response as AuthenticatorAssertionResponse;
  return {
    credentialId: toBase64Url(credential.rawId),
    clientDataJson: toBase64Url(response.clientDataJSON),
    authenticatorData: toBase64Url(response.authenticatorData),
    signature: toBase64Url(response.signature),
    userHandle: response.userHandle ? toBase64Url(response.userHandle) : null,
  };
}
