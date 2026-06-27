import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { useAutoSync } from './use-auto-sync';
import { useMailStore } from '../store/mail';

describe('useAutoSync', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    useMailStore.setState({
      autoSyncIntervalMin: 5,
      syncing: false,
      accounts: [{ id: 'a' }] as never,
      syncAllInbox: vi.fn().mockResolvedValue(undefined),
    } as never);
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('每隔间隔调一次 syncAllInbox', () => {
    const spy = useMailStore.getState().syncAllInbox as ReturnType<typeof vi.fn>;
    renderHook(() => {
      useAutoSync();
    });
    vi.advanceTimersByTime(5 * 60_000);
    expect(spy).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(5 * 60_000);
    expect(spy).toHaveBeenCalledTimes(2);
  });

  it('syncing 时跳过', () => {
    useMailStore.setState({ syncing: true } as never);
    const spy = useMailStore.getState().syncAllInbox as ReturnType<typeof vi.fn>;
    renderHook(() => {
      useAutoSync();
    });
    vi.advanceTimersByTime(5 * 60_000);
    expect(spy).not.toHaveBeenCalled();
  });

  it('无账户时跳过', () => {
    useMailStore.setState({ accounts: [] } as never);
    const spy = useMailStore.getState().syncAllInbox as ReturnType<typeof vi.fn>;
    renderHook(() => {
      useAutoSync();
    });
    vi.advanceTimersByTime(5 * 60_000);
    expect(spy).not.toHaveBeenCalled();
  });

  it('interval=0 不设定时器', () => {
    useMailStore.setState({ autoSyncIntervalMin: 0 } as never);
    const spy = useMailStore.getState().syncAllInbox as ReturnType<typeof vi.fn>;
    renderHook(() => {
      useAutoSync();
    });
    vi.advanceTimersByTime(60 * 60_000);
    expect(spy).not.toHaveBeenCalled();
  });

  it('改间隔后只按新间隔触发，旧间隔不再触发', () => {
    const spy = useMailStore.getState().syncAllInbox as ReturnType<typeof vi.fn>;
    const { rerender } = renderHook(() => {
      useAutoSync();
    });
    act(() => {
      useMailStore.setState({ autoSyncIntervalMin: 1 } as never);
    });
    rerender();
    vi.advanceTimersByTime(60_000); // 1 分钟
    expect(spy).toHaveBeenCalledTimes(1);
    // 对抗：若旧 5 分钟定时器没清，5 分钟时会多一次
    vi.advanceTimersByTime(4 * 60_000);
    expect(spy).toHaveBeenCalledTimes(5); // 1 分钟间隔下又过了 4 次，共 5；旧定时器不应额外加
  });

  it('卸载清定时器', () => {
    const spy = useMailStore.getState().syncAllInbox as ReturnType<typeof vi.fn>;
    const { unmount } = renderHook(() => {
      useAutoSync();
    });
    unmount();
    vi.advanceTimersByTime(10 * 60_000);
    expect(spy).not.toHaveBeenCalled();
  });

  it('StrictMode 双挂载只有单个定时器', () => {
    const spy = useMailStore.getState().syncAllInbox as ReturnType<typeof vi.fn>;
    const { unmount } = renderHook(() => {
      useAutoSync();
    });
    unmount(); // 模拟 StrictMode mount→unmount
    renderHook(() => {
      useAutoSync();
    }); // →mount
    vi.advanceTimersByTime(5 * 60_000);
    expect(spy).toHaveBeenCalledTimes(1); // 非 2
  });
});
