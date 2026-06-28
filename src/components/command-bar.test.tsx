import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { CommandBar } from './command-bar';
import { useUiStore } from '../lib/store/ui';
import { useMailStore } from '../lib/store/mail';

beforeEach(() => {
  useUiStore.setState({ theme: 'light' });
  document.documentElement.classList.remove('dark');
});

describe('CommandBar', () => {
  it('reports query changes', async () => {
    const onQuery = vi.fn();
    render(<CommandBar onQueryChange={onQuery} onAiCommand={vi.fn()} />);
    await userEvent.type(screen.getByPlaceholderText(/搜索全部账户/), 'hi');
    expect(onQuery).toHaveBeenLastCalledWith('hi');
  });

  it('toggles theme via button', async () => {
    render(<CommandBar onQueryChange={vi.fn()} onAiCommand={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: '切换主题' }));
    expect(useUiStore.getState().theme).toBe('dark');
  });
});

describe('CommandBar 自动收信指示器', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    useMailStore.setState({
      accounts: [{ id: 'a', email: 'x@y.z' }],
      accountErrors: {},
      syncing: false,
      error: null,
      autoSyncIntervalMin: 5,
      lastSyncAt: null,
      syncAllInbox: vi.fn().mockResolvedValue(undefined),
    } as never);
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('同步指示器渲染在顶栏', () => {
    render(<CommandBar onQueryChange={vi.fn()} onAiCommand={vi.fn()} />);
    expect(screen.getByRole('button', { name: '自动收信状态，点击立即同步' })).toBeInTheDocument();
  });
});
