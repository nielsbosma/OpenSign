import { describe, expect, it } from 'vitest';
import { checkText } from './ReviewPanel';

describe('checkText', () => {
  it('fills the text for a code', () => {
    const check = { severity: 'error', code: 'no_phone', params: { name: 'Bo' }, message: 'server' };
    expect(checkText(check)).toBe('Bo gets the document by SMS but has no phone number');
  });

  it('shows the English from the server without a known code', () => {
    expect(checkText({ severity: 'error', message: 'Saved before codes' })).toBe('Saved before codes');
    expect(checkText({ severity: 'error', code: 'from_a_newer_server', message: 'Newer' })).toBe('Newer');
    expect(checkText({ severity: 'error', code: 'constructor', message: 'Odd' })).toBe('Odd');
  });
});
