import { describe, it, expect, vi, afterEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useBreakpoint } from './use-breakpoint';

function mockWidth(width: number): void {
  vi.stubGlobal(
    'matchMedia',
    (query: string) =>
      ({
        matches:
          (query.includes('min-width: 1024px') && width >= 1024) ||
          (query.includes('min-width: 768px') && width >= 768),
        media: query,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        addListener: vi.fn(),
        removeListener: vi.fn(),
        dispatchEvent: vi.fn(),
        onchange: null,
      }) as unknown as MediaQueryList,
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('useBreakpoint', () => {
  it('returns desktop at >=1024', () => {
    mockWidth(1280);
    const { result } = renderHook(() => useBreakpoint());
    expect(result.current).toBe('desktop');
  });
  it('returns tablet between 768 and 1024', () => {
    mockWidth(800);
    const { result } = renderHook(() => useBreakpoint());
    expect(result.current).toBe('tablet');
  });
  it('returns mobile below 768', () => {
    mockWidth(400);
    const { result } = renderHook(() => useBreakpoint());
    expect(result.current).toBe('mobile');
  });
  it('returns desktop at exactly 1024', () => {
    mockWidth(1024);
    const { result } = renderHook(() => useBreakpoint());
    expect(result.current).toBe('desktop');
  });
  it('returns tablet at exactly 768', () => {
    mockWidth(768);
    const { result } = renderHook(() => useBreakpoint());
    expect(result.current).toBe('tablet');
  });
});
