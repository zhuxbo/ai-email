import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';

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
});
