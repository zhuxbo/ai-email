import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';

import { MessageList } from './message-list';
import { useMailStore } from '../lib/store/mail';

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
