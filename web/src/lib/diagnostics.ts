/**
 * The last few console errors and failed calls, kept in memory so a bug report can say what went
 * wrong just before. Nothing leaves the browser unless the user sends a report and keeps the lines.
 */

const MAX_LINE = 500;
const consoleLines: string[] = [];
const failedCalls: string[] = [];
let installed = false;

function push(list: string[], line: string, max: number) {
  list.push(line.slice(0, MAX_LINE));
  if (list.length > max) list.shift();
}

function stringify(value: unknown): string {
  if (value instanceof Error) return `${value.name}: ${value.message}`;
  if (typeof value === 'string') return value;
  try {
    return JSON.stringify(value) ?? String(value);
  } catch {
    return String(value);
  }
}

/** `/api/rpc/documents.get` becomes `documents.get`; other paths lose their ids. Never a query. */
function callName(raw: string): string {
  const path = new URL(raw, location.href).pathname;
  const rpc = /^\/api\/rpc\/([\w.]+)$/.exec(path);
  if (rpc) return rpc[1]!;
  return path
    .split('/')
    .map((part) => (/\d/.test(part) || part.length > 24 ? ':id' : part))
    .join('/');
}

/** Starts recording. Called once the signed-in app shell is up, never on public pages. */
export function installDiagnostics() {
  if (installed || typeof window === 'undefined') return;
  installed = true;
  for (const level of ['error', 'warn'] as const) {
    const original = console[level].bind(console);
    console[level] = (...args: unknown[]) => {
      push(consoleLines, `${level}: ${args.map(stringify).join(' ')}`, 30);
      original(...args);
    };
  }
  window.addEventListener('error', (e) => push(consoleLines, `uncaught: ${stringify(e.error ?? e.message)}`, 30));
  window.addEventListener('unhandledrejection', (e) => push(consoleLines, `unhandled rejection: ${stringify(e.reason)}`, 30));
  const originalFetch = window.fetch.bind(window);
  window.fetch = async (input, init) => {
    const raw = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
    try {
      const response = await originalFetch(input, init);
      if (response.status >= 400) push(failedCalls, `${callName(raw)} → ${response.status}`, 20);
      return response;
    } catch (error) {
      push(failedCalls, `${callName(raw)} → no answer`, 20);
      throw error;
    }
  };
}

export function diagnostics() {
  return { console: [...consoleLines], requests: [...failedCalls] };
}
