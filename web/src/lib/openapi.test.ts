import { describe, expect, it } from 'vitest';
import { endpoints, example, type OpenApi, snippets, typeLabel } from './openapi';

const spec: OpenApi = {
  info: { title: 'Test', version: '1' },
  paths: {
    '/documents/{id}': {
      get: { operationId: 'documents_get', tags: ['documents'] },
      patch: { operationId: 'documents_update', tags: ['documents'] },
    },
  },
  components: {
    schemas: {
      Party: {
        type: 'object',
        required: ['email', 'role'],
        properties: {
          email: { type: 'string', format: 'email' },
          role: { type: 'string', enum: ['signer', 'viewer'] },
          name: { type: ['string', 'null'] },
        },
      },
    },
  },
};

describe('openapi', () => {
  it('lists every operation with its method', () => {
    expect(endpoints(spec).map((e) => `${e.method} ${e.path}`)).toEqual(['get /documents/{id}', 'patch /documents/{id}']);
  });

  it('builds an example from the required fields only', () => {
    expect(example(spec, { type: 'array', items: { $ref: '#/components/schemas/Party' } })).toEqual([]);
    expect(example(spec, { $ref: '#/components/schemas/Party' })).toEqual({ email: 'anna@example.com', role: 'signer' });
  });

  it('names types for people', () => {
    expect(typeLabel(spec, { type: 'array', items: { $ref: '#/components/schemas/Party' } })).toBe('Party[]');
    expect(typeLabel(spec, { anyOf: [{ type: 'string', format: 'uuid' }, { type: 'null' }] })).toBe('string (uuid)');
  });

  it('quotes shell arguments safely', () => {
    const { curl, python } = snippets({
      method: 'post',
      url: 'https://x.test/api/v1/documents',
      body: `{"title":"Anna's lease","x":true}`,
    });
    expect(curl).toContain(`-d '{"title":"Anna'\\''s lease","x":true}'`);
    expect(python).toContain('"x": True');
  });
});
