import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { serverText } from './i18n';
import { SERVER_TEXTS } from './server-texts';

const CRATES = join(__dirname, '../../../crates');

function rust() {
  const files = readdirSync(CRATES, { recursive: true, encoding: 'utf8' }).filter((f) => f.endsWith('.rs'));
  return files
    .map((f) => readFileSync(join(CRATES, f), 'utf8'))
    .join('\n')
    .replace(/\\\r?\n\s*/g, '')
    .replaceAll('\\u{201c}', '“')
    .replaceAll('\\u{201d}', '”')
    .replaceAll('\\"', '"');
}

describe('server texts', () => {
  const source = rust();

  it('fill their placeholders and show unknown wording as it came', () => {
    expect(serverText('Upload “{label}” first', { label: 'ID' })).toBe('Upload “ID” first');
    expect(serverText('From a newer server')).toBe('From a newer server');
    expect(serverText('{value} is bad', { value: '{value}' })).toBe('{value} is bad');
  });

  it('are all still sent by the server', () => {
    const gone = SERVER_TEXTS.filter((text) => {
      const thing = text.match(/^(.+) not found$/)?.[1];
      if (thing && source.includes(`not_found("${thing}")`)) return false;
      return !source.includes(`"${text.replace(' (reference {id})', '')}`);
    });
    expect(gone).toEqual([]);
  });
});
