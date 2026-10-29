import { defineComponent, h, onMounted, ref, watch } from 'vue';

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

/** An OpenSign signing page inside your Vue app. `url` is a party's signing link from the OpenSign API. */
export const OpenSignSigning = defineComponent({
  name: 'OpenSignSigning',
  props: { url: { type: String, required: true } },
  emits: ['loaded', 'signed', 'declined', 'error'],
  setup(props, { emit }) {
    const ready = ref(false);
    const load = () =>
      loadElement(props.url)
        .then(() => {
          ready.value = true;
        })
        .catch((e) => emit('error', { message: e.message }));
    onMounted(load);
    watch(() => props.url, load);
    return () =>
      ready.value
        ? h('opensign-signing', {
            url: props.url,
            'onOpensign:loaded': (e) => emit('loaded', e.detail),
            'onOpensign:signed': (e) => emit('signed', e.detail),
            'onOpensign:declined': (e) => emit('declined', e.detail),
            'onOpensign:error': (e) => emit('error', e.detail),
          })
        : null;
  },
});
