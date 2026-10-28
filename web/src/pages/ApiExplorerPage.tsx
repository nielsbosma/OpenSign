import { useQuery } from '@tanstack/react-query';
import { ChevronRight, Download, KeyRound, Play, RotateCcw, Search } from 'lucide-react';
import { type KeyboardEvent, useEffect, useMemo, useState } from 'react';
import { Link, useSearchParams } from 'react-router';
import { CopyButton, EmptyState, PageHeader, Spinner } from '@/components/common';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Textarea } from '@/components/ui/textarea';
import { t } from '@/lib/i18n';
import {
  type Endpoint,
  endpoints,
  example,
  nullable,
  type OpenApi,
  type Parameter,
  refName,
  resolve,
  type Schema,
  snippets,
  typeLabel,
  variants,
} from '@/lib/openapi';
import { useSession } from '@/lib/session';
import { cn } from '@/lib/utils';

const BASE = '/api/v1';

const METHOD_COLORS: Record<string, string> = {
  get: 'bg-sky-500/12 text-sky-700 dark:text-sky-300',
  post: 'bg-emerald-500/12 text-emerald-700 dark:text-emerald-300',
  put: 'bg-amber-500/12 text-amber-700 dark:text-amber-300',
  patch: 'bg-violet-500/12 text-violet-700 dark:text-violet-300',
  delete: 'bg-red-500/12 text-red-700 dark:text-red-300',
};

function MethodBadge({ method, className }: { method: string; className?: string }) {
  return (
    <span
      className={cn(
        'inline-flex w-12 shrink-0 justify-center rounded px-1.5 py-0.5 font-mono font-semibold text-[10px] uppercase',
        METHOD_COLORS[method],
        className,
      )}
    >
      {method}
    </span>
  );
}

function SchemaRows({
  spec,
  schema,
  depth = 0,
  seen = [],
}: {
  spec: OpenApi;
  schema: Schema | undefined;
  depth?: number;
  seen?: string[];
}) {
  const resolved = resolve(spec, schema);
  const [target] = variants(spec, resolved);
  const object = target?.type === 'array' ? resolve(spec, target.items) : target;
  const properties = Object.entries(object?.properties ?? {});
  if (properties.length === 0) {
    return depth === 0 ? <p className="text-muted-foreground text-sm">{typeLabel(spec, schema)}</p> : null;
  }
  return (
    <ul className={cn('grid', depth > 0 && 'mt-1 ms-3 border-s ps-3')}>
      {properties.map(([name, prop]) => {
        const name_ = refName(prop) ?? refName(variants(spec, resolve(spec, prop))[0]);
        const inner = resolve(spec, variants(spec, resolve(spec, prop))[0]);
        const nested = (inner?.type === 'array' ? resolve(spec, inner.items) : inner)?.properties;
        const description = resolve(spec, prop).description ?? prop.description;
        const row = (
          <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 py-1.5">
            <code className="font-medium font-mono text-[13px]">{name}</code>
            <span className="font-mono text-muted-foreground text-xs">
              {typeLabel(spec, prop)}
              {nullable(spec, prop) && ' | null'}
            </span>
            {object?.required?.includes(name) && <span className="text-[11px] text-destructive">required</span>}
            {description && <p className="w-full text-muted-foreground text-xs leading-relaxed">{description}</p>}
          </div>
        );
        const recursive = !!name_ && seen.includes(name_);
        return (
          <li key={name} className="border-b border-dashed last:border-b-0">
            {nested && depth < 4 && !recursive ? (
              <details className="group">
                <summary className="flex cursor-pointer list-none items-start gap-1 [&::-webkit-details-marker]:hidden">
                  <ChevronRight className="mt-2.5 size-3.5 shrink-0 text-muted-foreground transition-transform rtl:rotate-180 group-open:rotate-90" />
                  <div className="min-w-0 flex-1">{row}</div>
                </summary>
                <SchemaRows spec={spec} schema={prop} depth={depth + 1} seen={name_ ? [...seen, name_] : seen} />
              </details>
            ) : (
              <div className="ps-4.5">{row}</div>
            )}
          </li>
        );
      })}
    </ul>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="grid gap-2">
      <h3 className="font-semibold text-muted-foreground text-xs uppercase tracking-wide">{title}</h3>
      {children}
    </section>
  );
}

type Result = { status: number; ms: number; contentType: string; body: string; blobUrl?: string; replayed: boolean };

function bodyKind(endpoint: Endpoint) {
  const types = Object.keys(endpoint.requestBody?.content ?? {});
  if (types.includes('multipart/form-data')) return 'multipart/form-data';
  if (types.includes('application/pdf')) return 'application/pdf';
  if (types.includes('application/json')) return 'application/json';
  return undefined;
}

function initialValues(spec: OpenApi, endpoint: Endpoint) {
  const params: Record<string, string> = {};
  for (const p of endpoint.parameters ?? []) {
    if (p.in === 'path') params[p.name] = '';
  }
  const json = endpoint.requestBody?.content['application/json']?.schema;
  return { params, body: json ? JSON.stringify(example(spec, json), null, 2) : '' };
}

function EndpointView({ spec, endpoint }: { spec: OpenApi; endpoint: Endpoint }) {
  const { user } = useSession();
  const [values, setValues] = useState(() => initialValues(spec, endpoint));
  const [file, setFile] = useState<File | null>(null);
  const [auth, setAuth] = useState<'session' | 'key'>('session');
  const [apiKey, setApiKey] = useState('');
  const [result, setResult] = useState<Result | null>(null);
  const [sending, setSending] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => () => void (result?.blobUrl && URL.revokeObjectURL(result.blobUrl)), [result]);

  const kind = bodyKind(endpoint);
  const parameters = endpoint.parameters ?? [];
  const pathParams = parameters.filter((p) => p.in === 'path');
  const queryParams = parameters.filter((p) => p.in === 'query');
  const responseSchema = Object.entries(endpoint.responses ?? {}).find(([code]) => code.startsWith('2'))?.[1]?.content?.['application/json']
    ?.schema;

  const path = endpoint.path.replace(/\{(\w+)\}/g, (_, name: string) =>
    values.params[name] ? encodeURIComponent(values.params[name]) : `{${name}}`,
  );
  const query = new URLSearchParams(
    queryParams.flatMap((p) => (values.params[p.name] ? [[p.name, values.params[p.name]] as [string, string]] : [])),
  ).toString();
  const relative = `${BASE}${path}${query ? `?${query}` : ''}`;
  const server = spec.servers?.[0]?.url ?? `${location.origin}${BASE}`;
  const code = snippets(
    {
      method: endpoint.method,
      url: `${server}${path}${query ? `?${query}` : ''}`,
      body: kind === 'application/json' && values.body.trim() ? compact(values.body) : undefined,
      contentType: kind,
    },
    file?.name,
  );

  const [lang, setLang] = useState<keyof typeof code>('curl');
  const setParam = (name: string, value: string) => setValues({ ...values, params: { ...values.params, [name]: value } });

  const send = async () => {
    setProblem(null);
    const missing = pathParams.find((p) => !values.params[p.name]);
    if (missing) return setProblem(t('Fill in {name}.', { name: missing.name }));
    if (auth === 'key' && !apiKey.trim()) return setProblem(t('Paste an API key, or use your session.'));
    if (kind && kind !== 'application/json' && !file) return setProblem(t('Choose a file to send.'));
    const headers: Record<string, string> = {};
    if (auth === 'key') headers.Authorization = `Bearer ${apiKey.trim()}`;
    let body: BodyInit | undefined;
    if (kind === 'application/json' && values.body.trim()) {
      try {
        body = JSON.stringify(JSON.parse(values.body));
      } catch {
        return setProblem(t('The body is not valid JSON.'));
      }
      headers['Content-Type'] = 'application/json';
    } else if (kind === 'multipart/form-data' && file) {
      const form = new FormData();
      form.append('file', file);
      body = form;
    } else if (kind === 'application/pdf' && file) {
      body = file;
      headers['Content-Type'] = 'application/pdf';
    }
    setSending(true);
    const started = performance.now();
    try {
      const response = await fetch(relative, {
        method: endpoint.method.toUpperCase(),
        headers,
        body,
        credentials: auth === 'session' ? 'same-origin' : 'omit',
      });
      const contentType = response.headers.get('content-type') ?? '';
      const ms = Math.round(performance.now() - started);
      const replayed = response.headers.get('idempotent-replayed') === 'true';
      if (contentType.includes('application/pdf')) {
        const blob = await response.blob();
        setResult({
          status: response.status,
          ms,
          contentType,
          replayed,
          body: `${blob.size.toLocaleString()} bytes`,
          blobUrl: URL.createObjectURL(blob),
        });
      } else {
        const text = await response.text();
        let pretty = text;
        try {
          pretty = JSON.stringify(JSON.parse(text), null, 2);
        } catch {}
        setResult({ status: response.status, ms, contentType, replayed, body: pretty });
      }
    } catch (e) {
      setProblem(e instanceof Error ? e.message : t('The request failed.'));
    } finally {
      setSending(false);
    }
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void send();
    }
  };

  return (
    <div className="grid min-w-0 gap-8 2xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
      <div className="grid min-w-0 content-start gap-6">
        <div className="grid gap-2">
          <div className="flex items-center gap-2">
            <MethodBadge method={endpoint.method} />
            <code className="break-all font-mono text-sm">{endpoint.path}</code>
          </div>
          {endpoint.summary && <p className="text-[15px] leading-relaxed">{endpoint.summary}</p>}
          <p className="text-muted-foreground text-xs">
            {t('Operation')} <code className="font-mono">{endpoint.operationId}</code>
            {endpoint.security?.length === 0 && ` · ${t('No API key needed')}`}
          </p>
        </div>
        {parameters.length > 0 && (
          <Section title={t('Parameters')}>
            <ParameterRows spec={spec} parameters={parameters} />
          </Section>
        )}
        {kind && (
          <Section title={`${t('Request Body')} · ${kind}`}>
            {kind === 'application/json' ? (
              <SchemaRows spec={spec} schema={endpoint.requestBody?.content[kind]?.schema} />
            ) : (
              <p className="text-muted-foreground text-sm">
                {kind === 'application/pdf' ? t('The PDF itself.') : t('A form with the file in `file`.')}
              </p>
            )}
          </Section>
        )}
        <Section title={t('Response')}>
          {responseSchema ? (
            <SchemaRows spec={spec} schema={responseSchema} />
          ) : (
            <p className="text-muted-foreground text-sm">
              {Object.values(endpoint.responses ?? {})[0]?.description ?? t('See the status code.')}
            </p>
          )}
          <p className="text-muted-foreground text-xs">
            Errors return <code className="font-mono">{'{ "error": "…", "kind": "…" }'}</code> with a 4xx or 5xx status.
          </p>
        </Section>
      </div>

      <div className="grid min-w-0 content-start gap-4 2xl:sticky 2xl:top-0" onKeyDown={onKeyDown}>
        <div className="grid gap-4 rounded-lg border bg-card p-4">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="me-auto font-semibold">{t('Try It')}</h3>
            <Select value={auth} onValueChange={(v) => setAuth(v as 'session' | 'key')}>
              <SelectTrigger size="sm" className="w-44">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="session">{t('As {name}', { name: user?.name ?? t('You') })}</SelectItem>
                <SelectItem value="key">{t('With an API Key')}</SelectItem>
              </SelectContent>
            </Select>
          </div>
          {auth === 'key' && (
            <Input
              type="password"
              autoComplete="off"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder={`osk_… (${t('kept in this tab only')})`}
              className="font-mono"
            />
          )}
          {parameters.map((p) => (
            <label key={p.name} className="grid gap-1">
              <span className="flex items-baseline gap-2 text-sm">
                <code className="font-mono">{p.name}</code>
                <span className="text-muted-foreground text-xs">{p.in}</span>
                {p.required && <span className="text-[11px] text-destructive">required</span>}
              </span>
              <ParameterInput spec={spec} parameter={p} value={values.params[p.name] ?? ''} onChange={(v) => setParam(p.name, v)} />
            </label>
          ))}
          {kind === 'application/json' && (
            <div className="grid gap-1">
              <div className="flex items-center justify-between">
                <span className="text-sm">{t('Body')}</span>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setValues({ ...values, body: initialValues(spec, endpoint).body })}
                  className="h-7 text-muted-foreground"
                >
                  <RotateCcw /> {t('Reset')}
                </Button>
              </div>
              <Textarea
                value={values.body}
                onChange={(e) => setValues({ ...values, body: e.target.value })}
                spellCheck={false}
                className="min-h-40 font-mono text-xs"
              />
            </div>
          )}
          {kind && kind !== 'application/json' && (
            <Input type="file" accept=".pdf,.docx,.doc,.odt,.rtf" onChange={(e) => setFile(e.target.files?.[0] ?? null)} />
          )}
          <div className="flex flex-wrap items-center gap-3">
            <Button onClick={send} disabled={sending}>
              {sending ? <Spinner className="text-primary-foreground" /> : <Play />} {t('Send Request')}
            </Button>
            <span className="text-muted-foreground text-xs">
              {endpoint.method === 'get' ? t('Ctrl+Enter sends.') : t('This changes real data. Ctrl+Enter sends.')}
            </span>
          </div>
          {problem && <p className="text-destructive text-sm">{problem}</p>}
        </div>

        {result && (
          <div className="grid gap-2 rounded-lg border bg-card p-4">
            <div className="flex flex-wrap items-center gap-2 text-sm">
              <Badge
                variant="outline"
                className={cn(
                  'font-mono',
                  result.status < 300 ? 'text-success' : result.status < 500 ? 'text-warning' : 'text-destructive',
                )}
              >
                {result.status}
              </Badge>
              <span className="text-muted-foreground">{result.ms} ms</span>
              {result.replayed && <Badge variant="secondary">{t('Idempotent Replay')}</Badge>}
              <span className="ms-auto">{!result.blobUrl && result.body && <CopyButton value={result.body} />}</span>
            </div>
            {result.blobUrl ? (
              <a href={result.blobUrl} download="document.pdf" className="inline-flex items-center gap-1.5 text-primary text-sm">
                <Download className="size-4" /> {t('Download the PDF ({size})', { size: result.body })}
              </a>
            ) : (
              <pre className="max-h-[28rem] overflow-auto rounded-md bg-muted/50 p-3 font-mono text-xs leading-relaxed">
                {result.body || t('(empty body)')}
              </pre>
            )}
          </div>
        )}

        <Tabs value={lang} onValueChange={(v) => setLang(v as keyof typeof code)} className="min-w-0 rounded-lg border bg-card p-4">
          <div className="flex items-center justify-between gap-2">
            <TabsList>
              <TabsTrigger value="curl">curl</TabsTrigger>
              <TabsTrigger value="javascript">JavaScript</TabsTrigger>
              <TabsTrigger value="python">Python</TabsTrigger>
            </TabsList>
            <CopyButton value={code[lang]} />
          </div>
          {(Object.keys(code) as (keyof typeof code)[]).map((lang) => (
            <TabsContent key={lang} value={lang} className="min-w-0">
              <pre className="overflow-x-auto rounded-md bg-muted/50 p-3 font-mono text-xs leading-relaxed">{code[lang]}</pre>
            </TabsContent>
          ))}
        </Tabs>
      </div>
    </div>
  );
}

function compact(json: string) {
  try {
    return JSON.stringify(JSON.parse(json));
  } catch {
    return json;
  }
}

function ParameterRows({ spec, parameters }: { spec: OpenApi; parameters: Parameter[] }) {
  return (
    <ul className="grid">
      {parameters.map((p) => (
        <li
          key={`${p.in}-${p.name}`}
          className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 border-b border-dashed py-1.5 last:border-b-0"
        >
          <code className="font-medium font-mono text-[13px]">{p.name}</code>
          <span className="font-mono text-muted-foreground text-xs">{typeLabel(spec, p.schema)}</span>
          <span className="text-muted-foreground text-xs">in {p.in}</span>
          {p.required && <span className="text-[11px] text-destructive">required</span>}
          {(p.description || resolve(spec, p.schema).description) && (
            <p className="w-full text-muted-foreground text-xs leading-relaxed">{p.description || resolve(spec, p.schema).description}</p>
          )}
        </li>
      ))}
    </ul>
  );
}

function ParameterInput({
  spec,
  parameter,
  value,
  onChange,
}: {
  spec: OpenApi;
  parameter: Parameter;
  value: string;
  onChange: (value: string) => void;
}) {
  const [schema] = variants(spec, resolve(spec, parameter.schema));
  const options = schema?.enum ?? (schema?.type === 'boolean' ? [true, false] : undefined);
  if (options) {
    return (
      <Select value={value || '__unset'} onValueChange={(v) => onChange(v === '__unset' ? '' : v)}>
        <SelectTrigger>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {!parameter.required && <SelectItem value="__unset">{t('Not set')}</SelectItem>}
          {options.map((o) => (
            <SelectItem key={String(o)} value={String(o)}>
              {String(o)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    );
  }
  return (
    <Input
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={typeLabel(spec, parameter.schema)}
      className="font-mono"
      inputMode={schema?.type === 'integer' || schema?.type === 'number' ? 'numeric' : undefined}
    />
  );
}

export function ApiExplorerPage() {
  const spec = useQuery({
    queryKey: ['openapi'],
    queryFn: async () => {
      const response = await fetch(`${BASE}/openapi.json`, { credentials: 'same-origin' });
      if (!response.ok) throw new Error(t('The API description could not be loaded'));
      return (await response.json()) as OpenApi;
    },
    staleTime: 10 * 60_000,
  });
  const [params, setParams] = useSearchParams();
  const [search, setSearch] = useState('');
  const all = useMemo(() => (spec.data ? endpoints(spec.data) : []), [spec.data]);
  const needle = search.trim().toLowerCase();
  const shown = all.filter((e) => [e.method, e.path, e.summary, e.operationId].some((s) => s?.toLowerCase().includes(needle)));
  const groups = [...new Set(shown.map((e) => e.tag))].map((tag) => [tag, shown.filter((e) => e.tag === tag)] as const);
  const selected = all.find((e) => e.operationId === params.get('op')) ?? all[0];

  return (
    <>
      <PageHeader
        title={t('API Explorer')}
        description={t('Every REST endpoint, with live calls and code to copy. Calls run as you, or with an API key you paste.')}
        actions={
          <div className="flex gap-2">
            <Button variant="outline" asChild>
              <a href={`${BASE}/openapi.json`} download="opensign-openapi.json">
                <Download /> {t('OpenAPI Document')}
              </a>
            </Button>
            <Button variant="outline" asChild>
              <Link to="/settings/api-keys">
                <KeyRound /> API Keys
              </Link>
            </Button>
          </div>
        }
      />
      {spec.isPending ? (
        <Spinner />
      ) : !spec.data ? (
        <EmptyState title={t('The API description could not be loaded')}>{t('Reload the page to try again.')}</EmptyState>
      ) : (
        <div className="grid gap-8 lg:grid-cols-[300px_minmax(0,1fr)]">
          <nav className="grid min-w-0 content-start gap-4 self-start lg:sticky lg:top-0" aria-label={t('Endpoints')}>
            <div className="relative">
              <Search className="absolute top-2.5 start-2.5 size-4 text-muted-foreground" />
              <Input value={search} onChange={(e) => setSearch(e.target.value)} placeholder={t('Search endpoints')} className="ps-8" />
            </div>
            {groups.length === 0 && <p className="text-muted-foreground text-sm">{t('No endpoint matches.')}</p>}
            {groups.map(([tag, list]) => (
              <div key={tag} className="grid min-w-0 gap-0.5">
                <h2 className="px-2 pb-1 font-semibold text-muted-foreground text-xs uppercase tracking-wide">{tag}</h2>
                {list.map((e) => (
                  <button
                    key={`${e.method} ${e.path}`}
                    type="button"
                    onClick={() => setParams({ op: e.operationId }, { replace: true })}
                    title={e.summary}
                    className={cn(
                      'flex min-w-0 items-center gap-2 rounded-md px-2 py-1.5 text-start hover:bg-muted',
                      e === selected && 'bg-muted',
                    )}
                  >
                    <MethodBadge method={e.method} />
                    <span className="truncate font-mono text-xs">{e.path}</span>
                  </button>
                ))}
              </div>
            ))}
            <p className="break-words px-2 text-muted-foreground text-xs leading-relaxed">
              {t(
                'Writes accept an {header} header, so a retry never acts twice. Scrive and DocuSign clients can use {scrive} and {docusign}.',
                {
                  header: 'Idempotency-Key',
                  scrive: '/scrive/api/v2',
                  docusign: '/docusign/restapi/v2.1',
                },
              )}
            </p>
          </nav>
          {selected && <EndpointView key={selected.operationId} spec={spec.data} endpoint={selected} />}
        </div>
      )}
    </>
  );
}
