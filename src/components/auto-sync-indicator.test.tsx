import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

import { AutoSyncIndicator } from './auto-sync-indicator';
import { useMailStore } from '../lib/store/mail';

function setStore(patch: Record<string, unknown>) {
  useMailStore.setState({
    syncing: false,
    lastSyncAt: null,
    autoSyncIntervalMin: 5,
    accountErrors: {},
    error: null,
    accounts: [{ id: 'a', email: 'x@y.z' }],
    syncAllInbox: vi.fn().mockResolvedValue(undefined),
    ...patch,
  } as never);
}

describe('AutoSyncIndicator', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    setStore({});
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('syncing 态显示同步中', () => {
    setStore({ syncing: true });
    render(<AutoSyncIndicator />);
    expect(screen.getByText(/同步中/)).toBeInTheDocument();
  });

  it('accountErrors 非空 → 失败态', () => {
    setStore({ accountErrors: { a: '认证失败' } });
    render(<AutoSyncIndicator />);
    expect(screen.getByText(/失败/)).toBeInTheDocument();
  });

  it('全局 error 也进失败态（accountErrors 空）', () => {
    setStore({ error: '网络错误' });
    render(<AutoSyncIndicator />);
    expect(screen.getByText(/失败/)).toBeInTheDocument(); // 对抗：漏全局 error 会落空闲
  });

  it('interval=0 → 已关闭', () => {
    setStore({ autoSyncIntervalMin: 0 });
    render(<AutoSyncIndicator />);
    expect(screen.getByText(/已关/)).toBeInTheDocument();
  });

  it('lastSyncAt=null → 尚未同步、不显倒计时', () => {
    setStore({ lastSyncAt: null });
    render(<AutoSyncIndicator />);
    expect(screen.getByText(/尚未同步/)).toBeInTheDocument();
    expect(screen.queryByText(/分钟后/)).toBeNull();
  });

  it('空闲态显示倒计时', () => {
    setStore({ lastSyncAt: Date.now() });
    render(<AutoSyncIndicator />);
    expect(screen.getByText(/分钟后/)).toBeInTheDocument();
  });

  it('点击调 syncAllInbox', () => {
    const spy = vi.fn().mockResolvedValue(undefined);
    setStore({ lastSyncAt: Date.now(), syncAllInbox: spy });
    render(<AutoSyncIndicator />);
    fireEvent.click(screen.getByRole('button'));
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it('syncing 或无账户时 disabled', () => {
    setStore({ syncing: true });
    render(<AutoSyncIndicator />);
    expect(screen.getByRole('button')).toBeDisabled();
  });
});
