import { useEffect, useState } from 'react';

/*
 * An element at the end of <body>, above everything, that stops its events there. Radix dialogs and
 * menus listen on `document` to close on outside clicks, trap focus and lock scrolling; with the
 * events stopped at this root they never notice, so what is inside works on top of an open dialog.
 */
const ISOLATED_EVENTS = [
  'pointerdown',
  'pointerup',
  'mousedown',
  'mouseup',
  'click',
  'dblclick',
  'contextmenu',
  'touchstart',
  'touchend',
  'touchmove',
  'wheel',
  'focusin',
  'keydown',
  'keyup',
  'keypress',
  'input',
] as const;

export function useIsolatedRoot(id: string): HTMLElement | null {
  const [root, setRoot] = useState<HTMLElement | null>(null);
  useEffect(() => {
    let el = document.getElementById(id);
    if (!el) {
      el = document.createElement('div');
      el.id = id;
      Object.assign(el.style, { position: 'fixed', inset: '0', zIndex: '2147483000', pointerEvents: 'none' });
      document.body.appendChild(el);
    }
    const root = el;
    const stop = (e: Event) => e.stopPropagation();
    for (const type of ISOLATED_EVENTS) root.addEventListener(type, stop);
    // A focus trap pulls focus back when it leaves the dialog; that focusout starts inside the dialog, so it is caught on the way down.
    const keepFocus = (e: FocusEvent) => {
      if (e.relatedTarget instanceof Node && root.contains(e.relatedTarget)) e.stopImmediatePropagation();
    };
    window.addEventListener('focusout', keepFocus, true);
    // Modals hide or disable everything else; what is in this root stays reachable.
    const unhide = new MutationObserver(() => {
      if (root.hasAttribute('aria-hidden')) root.removeAttribute('aria-hidden');
      if (root.hasAttribute('inert')) root.removeAttribute('inert');
    });
    unhide.observe(root, { attributes: true, attributeFilter: ['aria-hidden', 'inert'] });
    setRoot(root);
    return () => {
      for (const type of ISOLATED_EVENTS) root.removeEventListener(type, stop);
      window.removeEventListener('focusout', keepFocus, true);
      unhide.disconnect();
      root.remove();
    };
  }, [id]);
  return root;
}
