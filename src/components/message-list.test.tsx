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
});
