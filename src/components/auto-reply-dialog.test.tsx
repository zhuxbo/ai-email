import { describe, it, expect, beforeEach, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';

import { AutoReplyDialog } from './auto-reply-dialog';
import { useAutoReplyStore } from '../lib/store/auto-reply';
import { useMailStore } from '../lib/store/mail';

beforeEach(() => {
  useMailStore.setState({ accounts: [], selectedAccountId: null } as never);
  useAutoReplyStore.setState({ queue: [], rules: [], rulesAccountId: null, error: null });
  vi.restoreAllMocks();
});

describe('AutoReplyDialog', () => {
  it('open=false 不渲染', () => {
    render(<AutoReplyDialog open={false} onClose={vi.fn()} />);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('open 时 loadQueue 被调用并渲染两区标题', () => {
    const loadQueue = vi.fn().mockResolvedValue(undefined);
    useAutoReplyStore.setState({ loadQueue } as never);
    render(<AutoReplyDialog open onClose={vi.fn()} />);
    expect(loadQueue).toHaveBeenCalled();
    expect(screen.getByText('建议回复队列')).toBeInTheDocument();
    expect(screen.getByText('规则管理')).toBeInTheDocument();
  });

  it('未手动选择时跟随当前账户，手动选择后保持规则账户', async () => {
    const loadRules = vi.fn().mockResolvedValue(undefined);
    useMailStore.setState({
      accounts: [
        { id: 'a1', email: 'first@example.com' },
        { id: 'a2', email: 'second@example.com' },
      ],
      selectedAccountId: 'a1',
    } as never);
    useAutoReplyStore.setState({ loadRules } as never);

    render(<AutoReplyDialog open onClose={vi.fn()} />);
    const select = screen.getByRole('combobox', { name: '规则所属账户' });
    expect(select).toHaveValue('a1');

    act(() => {
      useMailStore.setState({ selectedAccountId: 'a2' } as never);
    });
    await waitFor(() => {
      expect(select).toHaveValue('a2');
    });

    fireEvent.change(select, { target: { value: 'a1' } });
    act(() => {
      useMailStore.setState({ selectedAccountId: 'a2' } as never);
    });
    expect(select).toHaveValue('a1');
    expect(loadRules).toHaveBeenLastCalledWith('a1');
  });
});
