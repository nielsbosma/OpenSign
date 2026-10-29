/*
 * OpenSign embedded signing. Load it from your OpenSign server and place a signing link from the API:
 *
 *   <script src="https://sign.example.com/embed.js"></script>
 *   <opensign-signing url="https://sign.example.com/sign#…"></opensign-signing>
 *
 * The element fires opensign:loaded, opensign:signed, opensign:declined and opensign:error, with ids in event.detail.
 * Your site must be listed under Admin settings, Security, Sites Allowed to Embed.
 */
(() => {
  if (window.customElements.get('opensign-signing')) return;
  const EVENTS = ['loaded', 'signed', 'declined', 'error'];

  class OpenSignSigning extends HTMLElement {
    static observedAttributes = ['url'];

    connectedCallback() {
      if (!this.style.display) this.style.display = 'block';
      window.addEventListener('message', this.onMessage);
      this.render();
    }

    disconnectedCallback() {
      window.removeEventListener('message', this.onMessage);
    }

    attributeChangedCallback() {
      if (this.isConnected) this.render();
    }

    onMessage = (event) => {
      if (!this.frame || event.source !== this.frame.contentWindow || event.origin !== this.origin) return;
      const data = event.data;
      if (data?.source !== 'opensign' || !EVENTS.includes(data.type)) return;
      const { source: _, type, ...detail } = data;
      this.dispatchEvent(new CustomEvent(`opensign:${type}`, { detail, bubbles: true, composed: true }));
    };

    render() {
      let url;
      try {
        url = new URL(this.getAttribute('url') ?? '');
      } catch {
        return;
      }
      if (url.protocol !== 'https:' && url.hostname !== 'localhost') return;
      if (!this.frame) {
        this.frame = document.createElement('iframe');
        this.frame.title = this.getAttribute('title') ?? 'OpenSign';
        this.frame.allow = 'camera';
        this.frame.referrerPolicy = 'no-referrer';
        this.frame.style.cssText = 'display:block;width:100%;height:100%;min-height:640px;border:0';
        this.replaceChildren(this.frame);
      }
      this.origin = url.origin;
      this.frame.src = url.href;
    }
  }

  window.customElements.define('opensign-signing', OpenSignSigning);
})();
