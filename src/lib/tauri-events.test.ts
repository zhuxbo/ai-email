// Tests for the typed Tauri event subscription wrappers in tauri.ts.
// Verifies that onMailClassified / onAutoReplyUpdated:
//   1. register a listen() call with the correct event name
//   2. invoke the callback when an event fires
//   3. return an unlisten function that can be called without error

import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { UnlistenFn } from '@tauri-apps/api/event';

// Mock @tauri-apps/api/event before importing the module under test.
const mockUnlisten = vi.fn<() => void>();
let capturedHandler: ((event: { payload: unknown }) => void) | null = null;

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(
    (_eventName: string, handler: (event: { payload: unknown }) => void): Promise<UnlistenFn> => {
      capturedHandler = handler;
      return Promise.resolve(mockUnlisten);
    },
  ),
}));

import { listen } from '@tauri-apps/api/event';
import { onMailClassified, onAutoReplyUpdated } from './tauri';

const listenMock = vi.mocked(listen);

describe('onMailClassified', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    capturedHandler = null;
  });

  it('registers listen on mail://classified', async () => {
    await onMailClassified(vi.fn());
    expect(listenMock).toHaveBeenCalledWith('mail://classified', expect.any(Function));
  });

  it('invokes callback with typed payload when event fires', async () => {
    const cb = vi.fn();
    await onMailClassified(cb);

    const payload = { accountId: 'acc-1', count: 3 };
    capturedHandler?.({ payload });

    expect(cb).toHaveBeenCalledWith(payload);
  });

  it('returns the unlisten function from listen', async () => {
    const unlisten = await onMailClassified(vi.fn());
    expect(unlisten).toBe(mockUnlisten);
    // calling unlisten should not throw
    expect(() => {
      unlisten();
    }).not.toThrow();
  });
});

describe('onAutoReplyUpdated', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    capturedHandler = null;
  });

  it('registers listen on autoreply://updated', async () => {
    await onAutoReplyUpdated(vi.fn());
    expect(listenMock).toHaveBeenCalledWith('autoreply://updated', expect.any(Function));
  });

  it('invokes callback with typed payload when event fires', async () => {
    const cb = vi.fn();
    await onAutoReplyUpdated(cb);

    const payload = { accountId: 'acc-2' };
    capturedHandler?.({ payload });

    expect(cb).toHaveBeenCalledWith(payload);
  });

  it('returns the unlisten function from listen', async () => {
    const unlisten = await onAutoReplyUpdated(vi.fn());
    expect(unlisten).toBe(mockUnlisten);
    expect(() => {
      unlisten();
    }).not.toThrow();
  });
});
