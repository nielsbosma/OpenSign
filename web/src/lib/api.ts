import { MutationCache, QueryCache, QueryClient, type UseQueryOptions, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import type { Services } from './api-types.gen';
import { t } from './i18n';

export type * from './api-types.gen';

export type ServiceName = keyof Services;
export type Input<N extends ServiceName> = Services[N]['input'];
export type Output<N extends ServiceName> = Services[N]['output'];

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly kind: string,
    readonly code?: string,
  ) {
    super(message);
  }
}

async function parse(response: Response) {
  const text = await response.text();
  let body = null;
  try {
    body = text ? JSON.parse(text) : null;
  } catch {
    if (response.ok) throw new ApiError(t('The server sent a response that could not be read'), response.status, 'internal');
  }
  if (!response.ok) {
    throw new ApiError(
      body?.error ?? response.statusText ?? t('Something went wrong'),
      response.status,
      body?.kind ?? 'internal',
      body?.code,
    );
  }
  return body;
}

/** `POST /api/rpc/<name>`. The session cookie rides along; the server checks Origin on every write. */
export async function call<N extends ServiceName>(name: N, input?: Input<N>): Promise<Output<N>> {
  const response = await fetch(`/api/rpc/${name}`, {
    method: 'POST',
    credentials: 'same-origin',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(input ?? {}),
  });
  return parse(response);
}

/** A PDF the app is allowed to read, as bytes. */
export async function fetchPdf(url: string, body?: unknown): Promise<ArrayBuffer> {
  const response = await fetch(url, {
    method: body === undefined ? 'GET' : 'POST',
    credentials: 'same-origin',
    headers: body === undefined ? undefined : { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!response.ok) await parse(response);
  return response.arrayBuffer();
}

export async function uploadFile(documentId: string, file: File): Promise<Output<'documents.upload'>> {
  const form = new FormData();
  form.append('file', file, file.name);
  const response = await fetch(`/api/documents/${documentId}/files`, { method: 'POST', credentials: 'same-origin', body: form });
  return parse(response);
}

/** Replaces one of a draft's or template's files, keeping its fields. */
export async function replaceFile(documentId: string, fileId: string, file: File): Promise<Output<'documents.replace_file'>> {
  const form = new FormData();
  form.append('file', file, file.name);
  const response = await fetch(`/api/documents/${documentId}/files/${fileId}/replace`, {
    method: 'POST',
    credentials: 'same-origin',
    body: form,
  });
  return parse(response);
}

/** A file the party was asked for, uploaded through their signing link. */
export async function uploadAttachment(
  token: string,
  session: string | null,
  request: string,
  file: Blob,
): Promise<import('./api-types.gen').AttachmentRequest> {
  const form = new FormData();
  form.append('token', token);
  if (session) form.append('session', session);
  form.append('request', request);
  form.append('file', file, file instanceof File ? file.name : 'webcam-photo.jpg');
  const response = await fetch('/api/sign/attachment', { method: 'POST', credentials: 'same-origin', body: form });
  return parse(response);
}

/** The image or file a party puts in one of their image or file fields. */
export async function uploadFieldFile(
  token: string,
  session: string | null,
  field: string,
  file: File,
): Promise<import('./api-types.gen').FieldRow> {
  const form = new FormData();
  form.append('token', token);
  if (session) form.append('session', session);
  form.append('field', field);
  form.append('file', file, file.name);
  const response = await fetch('/api/sign/field-file', { method: 'POST', credentials: 'same-origin', body: form });
  return parse(response);
}

export async function verifyFile(file: File): Promise<Output<'documents.verify'>> {
  const response = await fetch('/api/verify', { method: 'POST', credentials: 'same-origin', body: file });
  return parse(response);
}

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : t('Something went wrong');
}

export function createQueryClient() {
  const client: QueryClient = new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 15_000,
        retry: (count, error) => !(error instanceof ApiError && error.status < 500) && count < 2,
      },
    },
    queryCache: new QueryCache({
      // A session that ended elsewhere: ask who we are again, which sends the app to sign in.
      onError: (error, query) => {
        if (error instanceof ApiError && error.status === 401 && query.queryKey[0] !== 'account.me') {
          void client.invalidateQueries({ queryKey: ['account.me'] });
        }
      },
    }),
    mutationCache: new MutationCache({
      // Mutations that show their own errors pass `meta: { silent: true }`.
      onError: (error, _vars, _ctx, mutation) => {
        if (!mutation.meta?.silent) toast.error(errorMessage(error));
      },
    }),
  });
  return client;
}

export function useApi<N extends ServiceName>(
  name: N,
  input?: Input<N>,
  options?: Omit<UseQueryOptions<Output<N>, ApiError>, 'queryKey' | 'queryFn'>,
) {
  return useQuery<Output<N>, ApiError>({ queryKey: [name, input ?? {}], queryFn: () => call(name, input), ...options });
}

/** A write. On success it refreshes the queries named in `invalidates` (by service name prefix). */
export function useApiMutation<N extends ServiceName>(
  name: N,
  options?: { invalidates?: string[]; onSuccess?: (output: Output<N>, input: Input<N>) => void; silent?: boolean },
) {
  const client = useQueryClient();
  return useMutation<Output<N>, ApiError, Input<N>>({
    mutationFn: (input) => call(name, input),
    meta: options?.silent ? { silent: true } : undefined,
    onSuccess: async (output, input) => {
      if (options?.invalidates) {
        await Promise.all(
          options.invalidates.map((prefix) => client.invalidateQueries({ predicate: (q) => String(q.queryKey[0]).startsWith(prefix) })),
        );
      }
      options?.onSuccess?.(output, input);
    },
  });
}
