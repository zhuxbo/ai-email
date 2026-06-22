import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

import { SuggestedReplyList } from './suggested-reply-list';
import { useAutoReplyStore } from '../lib/store/auto-reply';
import { useComposeStore } from '../lib/store/compose';
import { useUiStore } from '../lib/store/ui';
import type { SuggestedReply } from '../lib/types';

const sug = (id: string): SuggestedReply => ({
  id,
  messageId: `m-${id}`,
  accountId: 'acc-1',
  ruleNameSnapshot: '工作紧急',
  intentSnapshot: '礼貌确认今天内回复',
  subject: '合同问题',
  fromAddr: 'boss@client.com',
  snippet: '合同草案请查收',
  sentAt: null,
  category: 'work',
  priority: 1,
  createdAt: '2026-06-22T00:00:00Z',
});

beforeEach(() => {
  useAutoReplyStore.setState({ queue: [sug('a')], error: null });
  vi.restoreAllMocks();
});

describe('SuggestedReplyList', () => {
  it('渲染队列项 + 命中规则名', () => {
    render(<SuggestedReplyList />);
    expect(screen.getByText('合同问题')).toBeInTheDocument();
    expect(screen.getByText(/工作紧急/)).toBeInTheDocument();
  });

  it('「去回复」接线 compose.openReply + 预填意图 + runDraft + 开抽屉 compose', () => {
    const openReply = vi.fn();
    const setField = vi.fn();
    const runDraft = vi.fn().mockResolvedValue(undefined);
    const openDrawer = vi.fn();
    useComposeStore.setState({ openReply, setField, runDraft } as never);
    useUiStore.setState({ openDrawer } as never);
    render(<SuggestedReplyList />);
    fireEvent.click(screen.getByRole('button', { name: '去回复' }));
    expect(openReply).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'm-a', accountId: 'acc-1', fromAddr: 'boss@client.com' }),
    );
    expect(setField).toHaveBeenCalledWith({ intentZh: '礼貌确认今天内回复' });
    expect(runDraft).toHaveBeenCalled();
    expect(openDrawer).toHaveBeenCalledWith('compose');
  });

  it('「忽略」调 dismiss', () => {
    const dismiss = vi.fn().mockResolvedValue(undefined);
    useAutoReplyStore.setState({ dismiss } as never);
    render(<SuggestedReplyList />);
    fireEvent.click(screen.getByRole('button', { name: '忽略' }));
    expect(dismiss).toHaveBeenCalledWith('a');
  });

  it('空队列显示友好提示', () => {
    useAutoReplyStore.setState({ queue: [] });
    render(<SuggestedReplyList />);
    expect(screen.getByText(/暂无建议回复/)).toBeInTheDocument();
  });
});
