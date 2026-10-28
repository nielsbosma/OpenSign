/** Just enough of OpenAPI 3.1 to browse and call the OpenSign REST API from the explorer. */

export type Schema = {
  $ref?: string;
  type?: string | string[];
  format?: string;
  description?: string;
  title?: string;
  enum?: unknown[];
  const?: unknown;
  default?: unknown;
  properties?: Record<string, Schema>;
  required?: string[];
  items?: Schema;
  anyOf?: Schema[];
  oneOf?: Schema[];
  allOf?: Schema[];
  additionalProperties?: boolean | Schema;
  minimum?: number;
  maximum?: number;
  maxLength?: number;
};

export type Parameter = { name: string; in: 'path' | 'query'; required?: boolean; schema?: Schema; description?: string | null };

type Content = Record<string, { schema?: Schema }>;

export type Operation = {
  operationId: string;
  summary?: string;
  tags?: string[];
  parameters?: Parameter[];
  requestBody?: { required?: boolean; content: Content };
  responses?: Record<string, { description?: string; content?: Content }>;
  security?: unknown[];
};

export type OpenApi = {
  info: { title: string; version: string; description?: string };
  servers?: { url: string }[];
  paths: Record<string, Record<string, Operation>>;
  components?: { schemas?: Record<string, Schema> };
};

export type Endpoint = Operation & { method: string; path: string; tag: string };

const METHODS = ['get', 'post', 'put', 'patch', 'delete'];

export function endpoints(spec: OpenApi): Endpoint[] {
  return Object.entries(spec.paths).flatMap(([path, ops]) =>
    METHODS.flatMap((method) => {
      const op = ops[method];
      return op ? [{ ...op, method, path, tag: op.tags?.[0] ?? 'general' }] : [];
    }),
  );
}

export function resolve(spec: OpenApi, schema: Schema | undefined): Schema {
  let s = schema ?? {};
  for (let depth = 0; s.$ref && depth < 20; depth++) {
    s = spec.components?.schemas?.[s.$ref.split('/').pop() ?? ''] ?? {};
  }
  return s;
}

export function refName(schema: Schema | undefined) {
  return schema?.$ref?.split('/').pop();
}

/** The non-null variants of a schema, so `Option<T>` shows as `T`. */
export function variants(spec: OpenApi, schema: Schema): Schema[] {
  const list = schema.anyOf ?? schema.oneOf;
  if (list) return list.map((v) => resolve(spec, v)).filter((v) => v.type !== 'null');
  if (schema.allOf?.length === 1) return [resolve(spec, schema.allOf[0])];
  return [schema];
}

export function typeLabel(spec: OpenApi, raw: Schema | undefined): string {
  if (!raw) return 'any';
  const name = refName(raw);
  const schema = resolve(spec, raw);
  const vs = variants(spec, schema);
  if (vs.length > 1 || vs[0] !== schema) {
    const labels = (schema.anyOf ?? schema.oneOf ?? schema.allOf ?? [])
      .filter((v) => resolve(spec, v).type !== 'null')
      .map((v) => typeLabel(spec, v));
    return [...new Set(labels)].join(' | ') || 'any';
  }
  if (schema.enum) return schema.enum.map((e) => JSON.stringify(e)).join(' | ');
  if (schema.const !== undefined) return JSON.stringify(schema.const);
  const types = (Array.isArray(schema.type) ? schema.type : [schema.type]).filter((t) => t && t !== 'null');
  const type = types[0];
  if (type === 'array') return `${typeLabel(spec, schema.items)}[]`;
  if (type === 'object' || schema.properties) return name ?? 'object';
  return [type ?? name ?? 'any', schema.format && `(${schema.format})`].filter(Boolean).join(' ');
}

export function nullable(spec: OpenApi, raw: Schema | undefined) {
  const schema = resolve(spec, raw);
  const types = Array.isArray(schema.type) ? schema.type : [schema.type];
  return types.includes('null') || !!(schema.anyOf ?? schema.oneOf)?.some((v) => resolve(spec, v).type === 'null');
}

const SAMPLE_STRINGS: Record<string, string> = {
  uuid: '00000000-0000-0000-0000-000000000000',
  'date-time': '2026-01-31T12:00:00Z',
  date: '2026-01-31',
  email: 'anna@example.com',
  uri: 'https://example.com',
};

/** A plausible value for a schema: its default, first enum value, or a typed placeholder. Only required fields of objects are filled. */
export function example(spec: OpenApi, raw: Schema | undefined, depth = 0): unknown {
  const schema = resolve(spec, raw);
  if (schema.default !== undefined) return schema.default;
  if (schema.const !== undefined) return schema.const;
  if (schema.enum?.length) return schema.enum[0];
  const [first] = variants(spec, schema);
  if (first !== schema) return first ? example(spec, first, depth) : null;
  if (depth > 6) return null;
  const types = (Array.isArray(schema.type) ? schema.type : [schema.type]).filter((t) => t !== 'null');
  switch (types[0] ?? (schema.properties ? 'object' : undefined)) {
    case 'string':
      return SAMPLE_STRINGS[schema.format ?? ''] ?? '';
    case 'integer':
    case 'number':
      return schema.minimum ?? 0;
    case 'boolean':
      return false;
    case 'array':
      return [];
    case 'object': {
      const out: Record<string, unknown> = {};
      for (const key of schema.required ?? []) out[key] = example(spec, schema.properties?.[key], depth + 1);
      return out;
    }
    default:
      return null;
  }
}

export type Call = {
  method: string;
  url: string;
  body?: string;
  contentType?: string;
};

const shellQuote = (s: string) => `'${s.replaceAll("'", `'\\''`)}'`;

export function snippets(call: Call, fileName?: string): Record<'curl' | 'javascript' | 'python', string> {
  const method = call.method.toUpperCase();
  const multipart = call.contentType === 'multipart/form-data';
  const pdf = call.contentType === 'application/pdf';
  const file = fileName ?? 'contract.pdf';

  const curl = [`curl -X ${method} ${shellQuote(call.url)}`, `  -H "Authorization: Bearer $OPENSIGN_API_KEY"`];
  if (multipart) curl.push(`  -F ${shellQuote(`file=@${file}`)}`);
  else if (pdf) curl.push('  -H "Content-Type: application/pdf"', `  --data-binary ${shellQuote(`@${file}`)}`);
  else if (call.body) curl.push('  -H "Content-Type: application/json"', `  -d ${shellQuote(call.body)}`);

  const jsInit = [`  method: '${method}',`, '  headers: {', `    Authorization: ${'`'}Bearer ${'$'}{process.env.OPENSIGN_API_KEY}${'`'},`];
  let jsPre = '';
  if (multipart) {
    jsPre = `const form = new FormData();\nform.append('file', new Blob([await fs.readFile('${file}')]), '${file}');\n\n`;
    jsInit.push('  },', '  body: form,');
  } else if (pdf) {
    jsInit.push("    'Content-Type': 'application/pdf',", '  },', `  body: await fs.readFile('${file}'),`);
  } else if (call.body) {
    jsInit.push("    'Content-Type': 'application/json',", '  },', `  body: JSON.stringify(${indent(call.body, 2).trimStart()}),`);
  } else {
    jsInit.push('  },');
  }
  const javascript = `${jsPre}const response = await fetch('${call.url}', {\n${jsInit.join('\n')}\n});\nconst result = await response.json();`;

  const py = ['import os', 'import requests', '', 'response = requests.request(', `    "${method}",`, `    "${call.url}",`];
  py.push(
    '    headers={"Authorization": f"Bearer {os.environ[\'OPENSIGN_API_KEY\']}"' + (pdf ? ', "Content-Type": "application/pdf"},' : '},'),
  );
  if (multipart) py.push(`    files={"file": open("${file}", "rb")},`);
  else if (pdf) py.push(`    data=open("${file}", "rb"),`);
  else if (call.body) py.push(`    json=${pythonLiteral(call.body)},`);
  py.push(')', 'response.raise_for_status()', 'print(response.json())');

  return { curl: curl.join(' \\\n'), javascript, python: py.join('\n') };
}

function indent(json: string, spaces: number) {
  return json
    .split('\n')
    .map((line, i) => (i === 0 ? line : ' '.repeat(spaces) + line))
    .join('\n');
}

function pythonLiteral(json: string) {
  try {
    return indent(
      JSON.stringify(JSON.parse(json), null, 4)
        .replace(/\btrue\b(?=(?:[^"]*"[^"]*")*[^"]*$)/g, 'True')
        .replace(/\bfalse\b(?=(?:[^"]*"[^"]*")*[^"]*$)/g, 'False')
        .replace(/\bnull\b(?=(?:[^"]*"[^"]*")*[^"]*$)/g, 'None'),
      4,
    );
  } catch {
    return json;
  }
}
