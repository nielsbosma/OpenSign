/** Whether another site shows this page in a frame. */
export const framed = (() => {
  try {
    return window.self !== window.top;
  } catch {
    return true;
  }
})();

export type EmbedEvent = 'loaded' | 'signed' | 'declined' | 'error';

/**
 * Tells the embedding site what happened. A message goes to each allowed origin by name, so the browser delivers it only
 * when that site is the one framing us; it never carries the link or any document content.
 */
export function notifyHost(origins: string[] | undefined, type: EmbedEvent, detail: Record<string, string | boolean>) {
  if (!framed || !origins) return;
  for (const origin of origins) {
    window.parent.postMessage({ source: 'opensign', type, ...detail }, origin);
  }
}

/** The redirect after signing, with DocuSign's `event` values so existing integrations keep working. */
export function redirectAfter(url: string, declined: boolean) {
  const target = new URL(url);
  if (target.protocol !== 'https:' && target.protocol !== 'http:') return;
  target.searchParams.set('event', declined ? 'decline' : 'signing_complete');
  window.location.assign(target.toString());
}
