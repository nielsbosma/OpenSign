import { fileURLToPath, URL } from 'node:url';
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

const api = process.env.API_PROXY_TARGET ?? 'http://localhost:3000';
const proxied = [
  '/api',
  '/mcp',
  '/.well-known',
  '/oauth/register',
  '/oauth/authorize',
  '/oauth/token',
  '/oauth/revoke',
  '/restapi',
  '/scrive/api',
  '/docusign/restapi',
];

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) } },
  server: {
    port: 5173,
    strictPort: true,
    host: process.env.VITE_HOST ?? 'localhost',
    proxy: Object.fromEntries(proxied.map((path) => [path, { target: api, xfwd: true }])),
  },
  // Nothing inlined as data: URLs, so the Content-Security-Policy can stay 'self' only.
  build: {
    sourcemap: true,
    chunkSizeWarningLimit: 1500,
    assetsInlineLimit: 0,
    rollupOptions: {
      output: {
        // One file per docs language, loaded only by people reading in that language.
        manualChunks: (id) => {
          const language = /\/docs\/([a-z]{2})\/[^/]+\.md/.exec(id)?.[1];
          return language ? `docs-${language}` : undefined;
        },
      },
    },
  },
});
