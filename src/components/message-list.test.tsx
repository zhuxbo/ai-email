import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

import { MessageList } from './message-list';
import { useMailStore } from '../lib/store/mail';
import type { FoldedItem, SearchHit } from '../lib/types';

const mkRow = (over: Partial<FoldedItem>): FoldedItem => ({
  id: 'm1',
  accountId: 'a1',
  mailboxId: 'mb1',
  imapUid: 1,
  rfcMessageId: null,
  threadId: null,
  subject: null,
  fromAddr: null,
  toAddrs: [],
  ccAddrs: [],
  sentAt: null,
  internalDate: null,
  flags: [],
  sizeBytes: null,
  hasAttachment: false,
  snippet: null,
  priority: null,
  category: null,
  tags: [],
  bodyFetchedAt: null,
  referencesHeader: null,
  filterDisabled: false,
  categoryLocked: false,
  foldKind: 'single',
  foldKey: 'm1',
  count: 1,
  hasUnread: false,
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
    expect(screen.getByText('收件箱为空，点顶部同步。')).toBeInTheDocument();
  });
  it('空状态文案不含「点左侧」', () => {
    useMailStore.setState({ accounts: [{ id: 'a1' }] as never } as never);
    render(<MessageList />);
    expect(screen.queryByText(/点左侧/)).toBeNull();
  });
  it('列表头不再渲染同步指示器（已上移顶栏）', () => {
    useMailStore.setState({ accounts: [{ id: 'a1' }] as never } as never);
    render(<MessageList />);
    expect(screen.queryByRole('button', { name: '自动收信状态，点击立即同步' })).toBeNull();
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
    } as never);
  });

  it('有未读时「全部已读」按钮可用并带计数', () => {
    useMailStore.setState({
      messages: [
        mkRow({ id: 'm1', hasUnread: true }),
        mkRow({ id: 'm2', hasUnread: false }),
      ] as never,
    } as never);
    render(<MessageList />);
    const btn = screen.getByRole('button', { name: /全部已读/ });
    expect(btn).toBeEnabled();
    expect(btn).toHaveTextContent('全部已读（1）');
  });

  it('无未读时按钮常驻但禁用', () => {
    useMailStore.setState({
      messages: [mkRow({ id: 'm1', hasUnread: false })] as never,
    } as never);
    render(<MessageList />);
    expect(screen.getByRole('button', { name: /全部已读/ })).toBeDisabled();
  });

  it('点击「全部已读」调用 markAllSeen', () => {
    const markAllSeen = vi.fn().mockResolvedValue(undefined);
    useMailStore.setState({
      messages: [mkRow({ id: 'm1', hasUnread: true })] as never,
      markAllSeen,
    } as never);
    render(<MessageList />);
    fireEvent.click(screen.getByRole('button', { name: /全部已读/ }));
    expect(markAllSeen).toHaveBeenCalled();
  });
});

describe('MessageList 未读筛选', () => {
  beforeEach(() => {
    useMailStore.setState({
      accounts: [{ id: 'a1' }],
      selectedAccountId: null,
      selectedMailboxId: null,
      categoryFilter: [],
      sortByPriority: false,
      unreadOnly: false,
      query: '',
      accountErrors: {},
    } as never);
  });

  it('开启未读只显示未读邮件', () => {
    useMailStore.setState({
      messages: [
        mkRow({ id: 'm1', subject: '未读邮件', hasUnread: true }),
        mkRow({ id: 'm2', subject: '已读邮件', hasUnread: false }),
      ] as never,
      unreadOnly: true,
    } as never);
    render(<MessageList />);
    expect(screen.getByText('未读邮件')).toBeInTheDocument();
    expect(screen.queryByText('已读邮件')).toBeNull();
  });

  it('点「未读」按钮切换 unreadOnly', () => {
    const setUnreadOnly = vi.fn();
    useMailStore.setState({
      messages: [mkRow({ id: 'm1', hasUnread: true })] as never,
      setUnreadOnly,
    } as never);
    render(<MessageList />);
    fireEvent.click(screen.getByRole('button', { name: '未读' }));
    expect(setUnreadOnly).toHaveBeenCalledWith(true);
  });
});

describe('MessageList 搜索态', () => {
  const mkHit = (over: Partial<SearchHit>): SearchHit => ({
    ...mkRow({ id: 'h1' }),
    mailboxName: 'INBOX',
    accountEmail: 'me@qq.com',
    bodyMatched: false,
    ...over,
  });

  beforeEach(() => {
    useMailStore.setState({
      accounts: [{ id: 'a1' }],
      messages: [mkRow({ id: 'm1' })],
      selectedAccountId: null,
      selectedMailboxId: null,
      categoryFilter: [],
      sortByPriority: false,
      query: '发票',
      searchResults: null,
      searching: false,
      accountErrors: {},
    } as never);
  });

  it('搜索结果非空时接管列表：显示搜索标题与归属徽标，隐藏筛选控件', () => {
    useMailStore.setState({
      searchResults: [
        mkHit({ id: 'h1', subject: '季度发票', bodyMatched: true }),
        mkHit({ id: 'h2', subject: 'hello', mailboxName: '已发送' }),
      ],
    } as never);
    render(<MessageList />);
    expect(screen.getByText(/搜索“发票”/)).toBeInTheDocument();
    expect(screen.getByText('季度发票')).toBeInTheDocument();
    expect(screen.getByText('正文')).toBeInTheDocument(); // bodyMatched 徽标
    expect(screen.getByText('已发送')).toBeInTheDocument(); // 信箱归属徽标
    // 筛选控件在搜索态不渲染
    expect(screen.queryByText('私人')).toBeNull();
    expect(screen.queryByRole('button', { name: '未读' })).toBeNull();
  });

  it('无结果时显示空态与正文覆盖范围提示', () => {
    useMailStore.setState({ searchResults: [], searching: false } as never);
    render(<MessageList />);
    expect(screen.getByText('无匹配结果。')).toBeInTheDocument();
    expect(screen.getByText(/正文仅覆盖已打开过/)).toBeInTheDocument();
  });

  it('点击结果行调用 openSearchHit', () => {
    const openSearchHit = vi.fn().mockResolvedValue(undefined);
    useMailStore.setState({
      searchResults: [mkHit({ id: 'h1', subject: '季度发票' })],
      openSearchHit,
    } as never);
    render(<MessageList />);
    fireEvent.click(screen.getByText('季度发票'));
    expect(openSearchHit).toHaveBeenCalledTimes(1);
  });
});

describe('MessageList 标签筛选', () => {
  beforeEach(() => {
    useMailStore.setState({
      accounts: [{ id: 'a1' }],
      messages: [
        mkRow({ id: 'm1', subject: 'm1', tags: ['报销', '重要'] }),
        mkRow({ id: 'm2', subject: 'm2', tags: [] }),
        mkRow({ id: 'm3', subject: 'm3', tags: ['报销'] }),
      ],
      selectedAccountId: null,
      selectedMailboxId: null,
      categoryFilter: [],
      sortByPriority: false,
      unreadOnly: false,
      tagFilter: null,
      query: '',
      searchResults: null,
      accountErrors: {},
    } as never);
  });

  it('未选标签时显示下拉入口（列出窗口内标签）', () => {
    render(<MessageList />);
    fireEvent.click(screen.getByText('标签 ▾'));
    expect(screen.getByText('#报销')).toBeInTheDocument();
    expect(screen.getByText('#重要')).toBeInTheDocument();
  });

  it('选中标签后仅显示含该标签的行，活动筛选可一键清除', () => {
    useMailStore.setState({ tagFilter: '报销' } as never);
    render(<MessageList />);
    expect(screen.getByText('m1')).toBeInTheDocument();
    expect(screen.getByText('m3')).toBeInTheDocument();
    expect(screen.queryByText('m2')).toBeNull();
    // 清除后全部可见
    fireEvent.click(screen.getByTitle('点击取消标签筛选'));
    expect(screen.getByText('m2')).toBeInTheDocument();
  });
});
