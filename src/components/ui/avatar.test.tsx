import { describe, it, expect } from 'vitest';
import { colorForSeed } from './avatar';
describe('colorForSeed', () => {
  it('同 seed 稳定同色', () => {
    expect(colorForSeed('x@y.z')).toBe(colorForSeed('x@y.z'));
  });
  it('返回调色板内的十六进制', () => {
    expect(colorForSeed('a')).toMatch(/^#[0-9a-f]{6}$/i);
  });
});
