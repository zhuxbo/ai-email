import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

import { MessageList } from './message-list';
import { useMailStore } from '../lib/store/mail';

const mkRow = (over: Record<string, unknown>) => ({
  id: 'm1',
  accountId: 'a1',
  subject: null,
  fromAddr: null,
  sentAt: null,
  snippet: null,
  priority: null,
  category: null,
  flags: [],
  tags: [],
  ...over,
});

describe('MessageList 空态', () => {
  beforeEach(() => {
    useMailStore.setState({
      accounts: [],
      messages: [],
      selectedAccountId: null,
      categoryFilter: [],
      sortByPriority: false,
      query: '',
      accountErrors: {},
    } as never);
  });
  it('0 账户引导添加', () => {
    render(<MessageList />);
    expect(screen.getByText('还没有账户，点左下角 ＋ 添加。')).toBeInTheDocument();
  });
  it('有账户空列表提示同步', () => {
    useMailStore.setState({ accounts: [{ id: 'a1' }] as never } as never);
    render(<MessageList />);
    expect(screen.getByText('收件箱为空，点左侧 🔄 同步。')).toBeInTheDocument();
  });
  it('部分账户加载失败时显示提示并映射邮箱', () => {
    useMailStore.setState({
      accounts: [{ id: 'a1', email: 'amy@qq.com' }] as never,
      accountErrors: { a1: 'boom' },
    } as never);
    render(<MessageList />);
    expect(screen.getByText(/1 个账户加载失败：amy@qq.com/)).toBeInTheDocument();
    // 失败原因（boom）也应显示出来，而非仅邮箱地址
    expect(screen.getByText(/boom/)).toBeInTheDocument();
  });
});

describe('MessageList 全部已读按钮', () => {
  beforeEach(() => {
    useMailStore.setState({
      accounts: [{ id: 'a1' }],
      messages: [],
      selectedAccountId: null,
      selectedMailboxId: null,
      categoryFilter: [],
      sortByPriority: false,
      query: '',
      accountErrors: {},
      error: null,
    } as never);
  });

  it('有未读时显示「全部已读」按钮', () => {
    useMailStore.setState({
      messages: [mkRow({ id: 'm1', flags: [] }), mkRow({ id: 'm2', flags: ['\\Seen'] })] as never,
    } as never);
    render(<MessageList />);
    expect(screen.getByRole('button', { name: '全部已读' })).toBeInTheDocument();
  });

  it('无未读时不显示该按钮', () => {
    useMailStore.setState({
      messages: [mkRow({ id: 'm1', flags: ['\\Seen'] })] as never,
    } as never);
    render(<MessageList />);
    expect(screen.queryByRole('button', { name: '全部已读' })).toBeNull();
  });

  it('点击「全部已读」调用 markAllSeen', () => {
    const markAllSeen = vi.fn().mockResolvedValue(undefined);
    useMailStore.setState({
      messages: [mkRow({ id: 'm1', flags: [] })] as never,
      markAllSeen,
    } as never);
    render(<MessageList />);
    fireEvent.click(screen.getByRole('button', { name: '全部已读' }));
    expect(markAllSeen).toHaveBeenCalled();
  });
});
