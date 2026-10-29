import { createElement, useEffect, useRef, useState } from 'react';

const scripts = new Map();

/** Loads embed.js from the OpenSign server that issued the link, once per server. */
function loadElement(url) {
  const origin = new URL(url).origin;
  if (window.customElements.get('opensign-signing')) return Promise.resolve();
  if (!scripts.has(origin)) {
    scripts.set(
      origin,
      new Promise((resolve, reject) => {
        const script = document.createElement('script');
        script.src = `${origin}/embed.js`;
        script.onload = () => resolve();
        script.onerror = () => {
          scripts.delete(origin);
          reject(new Error(`Could not load ${script.src}`));
        };
        document.head.append(script);
      }),
    );
  }
  return scripts.get(origin);
}

const EVENTS = { loaded: 'onLoaded', signed: 'onSigned', declined: 'onDeclined', error: 'onError' };

/** An OpenSign signing page inside your React app. `url` is a party's signing link from the OpenSign API. */
export function OpenSignSigning({ url, onLoaded, onSigned, onDeclined, onError, ...rest }) {
  const ref = useRef(null);
  const [ready, setReady] = useState(false);
  const handlers = useRef({});
  handlers.current = { onLoaded, onSigned, onDeclined, onError };

  useEffect(() => {
    let active = true;
    loadElement(url)
      .then(() => active && setReady(true))
      .catch((e) => handlers.current.onError?.({ message: e.message }));
    return () => {
      active = false;
    };
  }, [url]);

  useEffect(() => {
    const element = ref.current;
    if (!ready || !element) return;
    const listeners = Object.entries(EVENTS).map(([type, prop]) => {
      const listener = (event) => handlers.current[prop]?.(event.detail);
      element.addEventListener(`opensign:${type}`, listener);
      return [type, listener];
    });
    return () => {
      for (const [type, listener] of listeners) element.removeEventListener(`opensign:${type}`, listener);
    };
  }, [ready]);

  return ready ? createElement('opensign-signing', { ref, url, ...rest }) : null;
}
