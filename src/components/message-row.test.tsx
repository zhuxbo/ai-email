import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';

import { MessageRow } from './message-row';
import { useMailStore } from '../lib/store/mail';
import type { FoldedItem } from '../lib/types';

const m: FoldedItem = {
  id: 'm1',
  accountId: 'a1',
  mailboxId: 'mb1',
  imapUid: 1,
  rfcMessageId: null,
  threadId: null,
  subject: '主题',
  fromAddr: 'x@y.z',
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
  hasUnread: true,
};

describe('MessageRow', () => {
  it('渲染主题，带未读点与账户来源色边条', () => {
    useMailStore.setState({ accounts: [{ id: 'a1', email: 'acc@x.com' }] as never } as never);
    const { container } = render(
      <MessageRow m={m} count={1} hasUnread={true} active={false} onClick={vi.fn()} />,
    );
    expect(screen.getByText('主题')).toBeInTheDocument();
    expect(container.querySelector('[data-testid="unread-dot"]')).toBeTruthy();
    // 账户来源色现在体现在行左侧边框，而非独立圆点
    expect(container.querySelector('button')?.style.borderLeftColor).toBeTruthy();
  });

  it('hasUnread=true 显示蓝点，hasUnread=false 不渲染占位（发件人左对齐）', () => {
    useMailStore.setState({ accounts: [{ id: 'a1', email: 'acc@x.com' }] as never } as never);
    const unread = render(
      <MessageRow m={m} count={1} hasUnread={true} active={false} onClick={vi.fn()} />,
    );
    expect(unread.container.querySelector('[data-testid="unread-dot"]')?.className).toContain(
      'bg-blue-500',
    );
    unread.unmount();
    const read = render(
      <MessageRow m={m} count={1} hasUnread={false} active={false} onClick={vi.fn()} />,
    );
    // hasUnread=false 时未读点整体不渲染（不再保留透明占位），发件人因此左对齐。
    expect(read.container.querySelector('[data-testid="unread-dot"]')).toBeNull();
  });

  it('count>1 显示数量角标', () => {
    useMailStore.setState({ accounts: [{ id: 'a1', email: 'acc@x.com' }] as never } as never);
    render(<MessageRow m={m} count={3} hasUnread={true} active={false} onClick={vi.fn()} />);
    expect(screen.getByText('3')).toBeInTheDocument();
  });

  it('count=1 不显示数量角标', () => {
    useMailStore.setState({ accounts: [{ id: 'a1', email: 'acc@x.com' }] as never } as never);
    render(<MessageRow m={m} count={1} hasUnread={false} active={false} onClick={vi.fn()} />);
    expect(screen.queryByTestId('count-badge')).toBeNull();
  });

  it('hasUnread 驱动未读样式（不依赖 flags）', () => {
    useMailStore.setState({ accounts: [{ id: 'a1', email: 'acc@x.com' }] as never } as never);
    // flags 含 \\Seen 但 hasUnread=true → 仍显示未读点
    const mRead = { ...m, flags: ['\\Seen'] };
    render(<MessageRow m={mRead} count={1} hasUnread={true} active={false} onClick={vi.fn()} />);
    expect(screen.getByTestId('unread-dot')).toBeInTheDocument();
  });
});

describe('MessageRow 星标指示', () => {
  const base: FoldedItem = {
    id: 'm1',
    accountId: 'a1',
    mailboxId: 'mb',
    imapUid: 1,
    rfcMessageId: null,
    threadId: null,
    subject: 'Hi',
    fromAddr: 'bob@x.com',
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
  };
  it('\\Flagged 时渲染星标', () => {
    render(
      <MessageRow
        m={{ ...base, flags: ['\\Flagged'] }}
        count={1}
        hasUnread={false}
        active={false}
        onClick={vi.fn()}
      />,
    );
    expect(screen.getByLabelText('已加星')).toBeInTheDocument();
  });
  it('无 \\Flagged 不渲染星标', () => {
    render(
      <MessageRow
        m={{ ...base, flags: [] }}
        count={1}
        hasUnread={false}
        active={false}
        onClick={vi.fn()}
      />,
    );
    expect(screen.queryByLabelText('已加星')).toBeNull();
  });
});

describe('MessageRow 优先级标签', () => {
  beforeEach(() => {
    useMailStore.setState({ accounts: [{ id: 'a1', email: 'acc@x.com' }] as never } as never);
  });

  it('priority=3 的「低」与分类/AI 标签同处底部标签行（非主题行）', () => {
    render(
      <MessageRow
        m={{ ...m, priority: 3, category: 'notification', tags: ['订单验证'] }}
        count={1}
        hasUnread={false}
        active={false}
        onClick={vi.fn()}
      />,
    );
    const low = screen.getByText('低');
    // 优先级标签的父容器即底部标签行——必须与分类/AI 标签同处一行（而非主题行）。
    // 若 badge 仍在主题行，其父容器只含主题、不含「通知」，断言即失败。
    const tagRow = low.parentElement;
    expect(tagRow?.textContent).toContain('通知');
    expect(tagRow?.textContent).toContain('订单验证');
  });

  it('priority=1 渲染「高」', () => {
    render(
      <MessageRow
        m={{ ...m, priority: 1 }}
        count={1}
        hasUnread={false}
        active={false}
        onClick={vi.fn()}
      />,
    );
    expect(screen.getByText('高')).toBeInTheDocument();
  });

  it('priority=2 不渲染优先级标签', () => {
    render(
      <MessageRow
        m={{ ...m, priority: 2, category: null, tags: [] }}
        count={1}
        hasUnread={false}
        active={false}
        onClick={vi.fn()}
      />,
    );
    expect(screen.queryByText('高')).toBeNull();
    expect(screen.queryByText('低')).toBeNull();
  });

  it('仅有优先级标签（无分类、无 tag）也渲染标签行', () => {
    render(
      <MessageRow
        m={{ ...m, priority: 3, category: null, tags: [] }}
        count={1}
        hasUnread={false}
        active={false}
        onClick={vi.fn()}
      />,
    );
    expect(screen.getByText('低')).toBeInTheDocument();
  });
});
