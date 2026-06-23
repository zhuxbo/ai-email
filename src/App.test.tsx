// Tests for App.tsx event-driven subscription wiring (Phase-14 / audit #21-23).
// Verifies:
//   1. onMailClassified and onAutoReplyUpdated are subscribed on mount
//   2. callbacks trigger the correct store refreshes
//   3. unlisten is called on unmount (no subscription leak)

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, act } from '@testing-library/react';
import type { UnlistenFn } from '@tauri-apps/api/event';

// Captured event handlers so tests can fire events manually.
type Handler = (payload: unknown) => void;
let classifiedHandler: Handler | null = null;
let autoReplyHandler: Handler | null = null;
const mockUnlistenClassified = vi.fn<() => void>();
const mockUnlistenAutoReply = vi.fn<() => void>();

vi.mock('./lib/tauri', () => ({
  // event subscriptions under test
  onMailClassified: vi.fn((cb: Handler): Promise<UnlistenFn> => {
    classifiedHandler = cb;
    return Promise.resolve(mockUnlistenClassified);
  }),
  onAutoReplyUpdated: vi.fn((cb: Handler): Promise<UnlistenFn> => {
    autoReplyHandler = cb;
    return Promise.resolve(mockUnlistenAutoReply);
  }),
  // commands used during app init / rendering
  accountsList: vi.fn().mockResolvedValue([]),
  unifiedInbox: vi.fn().mockResolvedValue({ messages: [], errors: {} }),
  suggestedRepliesList: vi.fn().mockResolvedValue([]),
  mailboxesList: vi.fn().mockResolvedValue([]),
  messagesList: vi.fn().mockResolvedValue([]),
  roleDefaultsList: vi.fn().mockResolvedValue([]),
  modelsList: vi.fn().mockResolvedValue([]),
}));

// Spy on store methods called inside event callbacks.
const reloadMessagesSpy = vi.fn().mockResolvedValue(undefined);
const loadQueueSpy = vi.fn().mockResolvedValue(undefined);

const mailState = {
  reloadMessages: reloadMessagesSpy,
  loadAccounts: vi.fn().mockResolvedValue(undefined),
  accounts: [] as unknown[],
  selectedAccountId: null as string | null,
  messageOpenSeq: 0,
  syncing: false,
  error: null as string | null,
  syncInbox: vi.fn().mockResolvedValue(undefined),
  removeAccount: vi.fn().mockResolvedValue(undefined),
  setFilter: vi.fn().mockResolvedValue(undefined),
  setQuery: vi.fn(),
  clearError: vi.fn(),
};

vi.mock('./lib/store/mail', () => ({
  useMailStore: Object.assign(
    // selector hook — return the value from mailState
    (selector: (s: typeof mailState) => unknown) => selector(mailState),
    {
      getState: () => mailState,
    },
  ),
}));

vi.mock('./lib/store/auto-reply', () => ({
  useAutoReplyStore: Object.assign(
    (selector: (s: { queue: unknown[]; error: null }) => unknown) =>
      selector({ queue: [], error: null }),
    {
      getState: () => ({
        loadQueue: loadQueueSpy,
        queue: [],
        error: null,
        clearError: vi.fn(),
      }),
    },
  ),
}));

vi.mock('./lib/store/ai', () => ({
  useAiStore: Object.assign(
    (selector: (s: { error: null }) => unknown) => selector({ error: null }),
    {
      getState: () => ({ loadAiConfig: vi.fn().mockResolvedValue(undefined), error: null }),
    },
  ),
}));

vi.mock('./lib/store/compose', () => ({
  useComposeStore: Object.assign(
    (selector: (s: { error: null }) => unknown) => selector({ error: null }),
    { setState: vi.fn() },
  ),
}));

vi.mock('./lib/store/ui', () => ({
  applyTheme: vi.fn(),
  useUiStore: Object.assign(
    (selector: (s: { theme: string }) => unknown) => selector({ theme: 'light' }),
    {
      getState: () => ({ theme: 'light' }),
    },
  ),
}));

// Stub out heavy child components — we only care about App's useEffect wiring.
vi.mock('./components/app-shell', () => ({
  AppShell: () => <div data-testid="shell" />,
}));
vi.mock('./components/add-account-dialog', () => ({
  AddAccountDialog: () => null,
}));
vi.mock('./components/ai-drawer', () => ({
  AiDrawer: () => null,
}));
vi.mock('./components/ai-settings-dialog', () => ({
  AiSettingsDialog: () => null,
}));
vi.mock('./components/auto-reply-dialog', () => ({
  AutoReplyDialog: () => null,
}));
vi.mock('./components/message-detail', () => ({
  MessageDetail: () => null,
}));
vi.mock('./components/message-list', () => ({
  MessageList: () => null,
}));

import App from './App';
import { onMailClassified, onAutoReplyUpdated } from './lib/tauri';

const onMailClassifiedMock = vi.mocked(onMailClassified);
const onAutoReplyUpdatedMock = vi.mocked(onAutoReplyUpdated);

describe('App — event-driven subscription wiring', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    classifiedHandler = null;
    autoReplyHandler = null;
  });

  it('subscribes to mail://classified and autoreply://updated on mount', () => {
    act(() => {
      render(<App />);
    });

    expect(onMailClassifiedMock).toHaveBeenCalledTimes(1);
    expect(onAutoReplyUpdatedMock).toHaveBeenCalledTimes(1);
  });

  it('mail://classified callback triggers reloadMessages', () => {
    act(() => {
      render(<App />);
    });

    // Simulate backend emitting the event
    act(() => {
      classifiedHandler?.({ accountId: 'acc-1', count: 2 });
    });

    expect(reloadMessagesSpy).toHaveBeenCalled();
  });

  it('autoreply://updated callback triggers loadQueue', () => {
    act(() => {
      render(<App />);
    });

    act(() => {
      autoReplyHandler?.({ accountId: 'acc-1' });
    });

    expect(loadQueueSpy).toHaveBeenCalled();
  });

  it('calls unlisten for both subscriptions on unmount', async () => {
    let unmount!: () => void;
    act(() => {
      const result = render(<App />);
      unmount = result.unmount;
    });

    // flush the Promise microtasks so the .then(fn => unlisten = fn) callbacks
    // in the useEffect have a chance to run before we unmount
    await act(async () => {
      await Promise.resolve();
    });

    act(() => {
      unmount();
    });

    expect(mockUnlistenClassified).toHaveBeenCalledTimes(1);
    expect(mockUnlistenAutoReply).toHaveBeenCalledTimes(1);
  });
});
