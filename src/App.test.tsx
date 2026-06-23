// Tests for App.tsx event-driven subscription wiring (Phase-14 / audit #21-23).
// Verifies:
//   1. onMailClassified and onAutoReplyUpdated are subscribed on mount
//   2. callbacks trigger the correct store refreshes
//   3. unlisten is called on unmount (no subscription leak)
//   4. (Phase-15) classified 仅在 INBOX 视图触发 reloadMessages，非 INBOX 跳过

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
  // db init events — resolved immediately with a no-op unlisten in tests
  onDbReady: vi.fn((): Promise<UnlistenFn> => Promise.resolve(vi.fn())),
  onDbError: vi.fn((): Promise<UnlistenFn> => Promise.resolve(vi.fn())),
  // 兜底查询：默认返回 initializing（不触发状态推进，让 db store mock 保持 ready）
  getDbStatus: vi.fn().mockResolvedValue({ status: 'initializing' as const, message: null }),
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
// classifiedAffectsCurrentView 默认返回 true（聚合 INBOX），测试可按需覆盖。
const classifiedAffectsCurrentViewSpy = vi.fn(() => true);

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
  classifiedAffectsCurrentView: classifiedAffectsCurrentViewSpy,
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

// DB 状态在测试中固定为 ready，跳过 loading/error 界面，让主逻辑可测。
vi.mock('./lib/store/db', () => ({
  useDbStore: Object.assign(
    (selector: (s: { status: string; errorMessage: null }) => unknown) =>
      selector({ status: 'ready', errorMessage: null }),
    {
      getState: () => ({ setReady: vi.fn(), setError: vi.fn() }),
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
import { onMailClassified, onAutoReplyUpdated, onDbReady, onDbError } from './lib/tauri';

const onMailClassifiedMock = vi.mocked(onMailClassified);
const onAutoReplyUpdatedMock = vi.mocked(onAutoReplyUpdated);
const onDbReadyMock = vi.mocked(onDbReady);
const onDbErrorMock = vi.mocked(onDbError);

describe('App — event-driven subscription wiring', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    classifiedHandler = null;
    autoReplyHandler = null;
    // 每个测试前把 classifiedAffectsCurrentView 恢复为默认（聚合 INBOX → true）
    classifiedAffectsCurrentViewSpy.mockReturnValue(true);
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

  it('calls unlisten even when unmount races ahead of Promise resolve (mounted guard)', async () => {
    // 复现 I1 真泄漏路径：listen Promise 尚未 resolve，组件已卸载。
    // 旧代码：unlisten 变量仍是 null，cleanup 是 no-op，最终 resolve 的 fn 进死闭包永不调用。
    // 新代码（mounted 守卫）：resolve 时发现 mounted=false，立即调用 fn() 清理。

    let resolveClassified!: (fn: () => void) => void;
    let resolveAutoReply!: (fn: () => void) => void;

    const delayedClassifiedPromise = new Promise<() => void>((res) => {
      resolveClassified = res;
    });
    const delayedAutoReplyPromise = new Promise<() => void>((res) => {
      resolveAutoReply = res;
    });

    onMailClassifiedMock.mockReturnValueOnce(delayedClassifiedPromise);
    onAutoReplyUpdatedMock.mockReturnValueOnce(delayedAutoReplyPromise);

    let unmount!: () => void;
    act(() => {
      const result = render(<App />);
      unmount = result.unmount;
    });

    // 先 unmount，此时 listen Promise 还未 resolve
    act(() => {
      unmount();
    });

    // 再 resolve listen Promise（模拟网络/IPC 延迟后返回 unlisten fn）
    await act(async () => {
      resolveClassified(mockUnlistenClassified);
      resolveAutoReply(mockUnlistenAutoReply);
      await Promise.resolve();
    });

    // mounted 守卫应在 resolve 时发现组件已卸载，立即调用 unlisten fn
    expect(mockUnlistenClassified).toHaveBeenCalledTimes(1);
    expect(mockUnlistenAutoReply).toHaveBeenCalledTimes(1);
  });

  it('db effect: unmount 早于 Promise resolve 时 guard 清理孤儿 listener（StrictMode 对抗）', async () => {
    // 复现 db effect 竞态路径：onDbReady/onDbError 的 Promise 尚未 resolve，组件已卸载。
    // mounted 守卫（guard.mounted=false）应在 resolve 时立即调用返回的 unlisten fn 清理孤儿监听器。

    let resolveReady!: (fn: () => void) => void;
    let resolveError!: (fn: () => void) => void;

    const mockUnlistenDbReady = vi.fn<() => void>();
    const mockUnlistenDbError = vi.fn<() => void>();

    const delayedReadyPromise = new Promise<() => void>((res) => {
      resolveReady = res;
    });
    const delayedErrorPromise = new Promise<() => void>((res) => {
      resolveError = res;
    });

    onDbReadyMock.mockReturnValueOnce(delayedReadyPromise);
    onDbErrorMock.mockReturnValueOnce(delayedErrorPromise);

    let unmount!: () => void;
    act(() => {
      const result = render(<App />);
      unmount = result.unmount;
    });

    // 先 unmount，此时 db listener Promise 还未 resolve
    act(() => {
      unmount();
    });

    // 再 resolve（模拟 Tauri IPC 在卸载后才返回 unlisten fn）
    await act(async () => {
      resolveReady(mockUnlistenDbReady);
      resolveError(mockUnlistenDbError);
      await Promise.resolve();
    });

    // guard.mounted=false 分支应立即调用两个 unlisten fn，不留孤儿监听器
    expect(mockUnlistenDbReady).toHaveBeenCalledTimes(1);
    expect(mockUnlistenDbError).toHaveBeenCalledTimes(1);
  });
});

// ─── Phase-15：classified 视图过滤 ────────────────────────────────────────────
// 验证 App.tsx 的 onMailClassified 回调在非 INBOX 视图时跳过 reloadMessages。
// classifiedAffectsCurrentView 的单元逻辑在 mail store 测试中验证；
// 此处仅验证 App.tsx 正确地调用它并据此决策。
describe('App — classified 视图过滤（Phase-15）', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    classifiedHandler = null;
    autoReplyHandler = null;
    classifiedAffectsCurrentViewSpy.mockReturnValue(true);
  });

  it('聚合 INBOX 视图（classifiedAffectsCurrentView=true）收到 classified → 触发 reloadMessages', () => {
    classifiedAffectsCurrentViewSpy.mockReturnValue(true);
    act(() => {
      render(<App />);
    });

    act(() => {
      classifiedHandler?.({ accountId: 'acc-1', count: 3 });
    });

    expect(reloadMessagesSpy).toHaveBeenCalled();
  });

  it('非 INBOX 信箱（classifiedAffectsCurrentView=false）收到 classified → 不触发 reloadMessages', () => {
    classifiedAffectsCurrentViewSpy.mockReturnValue(false);
    act(() => {
      render(<App />);
    });

    act(() => {
      classifiedHandler?.({ accountId: 'acc-1', count: 3 });
    });

    expect(reloadMessagesSpy).not.toHaveBeenCalled();
  });

  it('INBOX 信箱（classifiedAffectsCurrentView=true）收到 classified → 触发 reloadMessages', () => {
    classifiedAffectsCurrentViewSpy.mockReturnValue(true);
    act(() => {
      render(<App />);
    });

    act(() => {
      classifiedHandler?.({ accountId: 'acc-1', count: 1 });
    });

    expect(reloadMessagesSpy).toHaveBeenCalled();
  });

  it('同一非 INBOX 信箱连续多次 classified 事件 → 始终跳过（不误触发）', () => {
    classifiedAffectsCurrentViewSpy.mockReturnValue(false);
    act(() => {
      render(<App />);
    });

    act(() => {
      classifiedHandler?.({ accountId: 'acc-1', count: 1 });
      classifiedHandler?.({ accountId: 'acc-1', count: 2 });
      classifiedHandler?.({ accountId: 'acc-1', count: 3 });
    });

    expect(reloadMessagesSpy).not.toHaveBeenCalled();
  });

  it('视图切换：非 INBOX 时事件跳过，切换后视图变 INBOX 时再来事件则触发', () => {
    // 第一阶段：非 INBOX
    classifiedAffectsCurrentViewSpy.mockReturnValue(false);
    act(() => {
      render(<App />);
    });

    act(() => {
      classifiedHandler?.({ accountId: 'acc-1', count: 1 });
    });
    expect(reloadMessagesSpy).not.toHaveBeenCalled();

    // 模拟用户切换到 INBOX（classifiedAffectsCurrentView 现在返回 true）
    classifiedAffectsCurrentViewSpy.mockReturnValue(true);

    act(() => {
      classifiedHandler?.({ accountId: 'acc-1', count: 2 });
    });
    // 此次应触发
    expect(reloadMessagesSpy).toHaveBeenCalledTimes(1);
  });
});
