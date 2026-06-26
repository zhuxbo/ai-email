import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';

import { MessageRow } from './message-row';
import { useMailStore } from '../lib/store/mail';
import type { MessageHeader } from '../lib/types';

const m: MessageHeader = {
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
};

describe('MessageRow', () => {
  it('渲染主题，带未读点与账户来源色边条', () => {
    useMailStore.setState({ accounts: [{ id: 'a1', email: 'acc@x.com' }] as never } as never);
    const { container } = render(<MessageRow m={m} active={false} onClick={vi.fn()} />);
    expect(screen.getByText('主题')).toBeInTheDocument();
    expect(container.querySelector('[data-testid="unread-dot"]')).toBeTruthy();
    // 账户来源色现在体现在行左侧边框，而非独立圆点
    expect(container.querySelector('button')?.style.borderLeftColor).toBeTruthy();
  });

  it('未读时显示蓝点，已读时不渲染占位（发件人左对齐）', () => {
    useMailStore.setState({ accounts: [{ id: 'a1', email: 'acc@x.com' }] as never } as never);
    const unread = render(<MessageRow m={{ ...m, flags: [] }} active={false} onClick={vi.fn()} />);
    expect(unread.container.querySelector('[data-testid="unread-dot"]')?.className).toContain(
      'bg-blue-500',
    );
    unread.unmount();
    const read = render(
      <MessageRow m={{ ...m, flags: ['\\Seen'] }} active={false} onClick={vi.fn()} />,
    );
    // 已读后未读点整体不渲染（不再保留透明占位），发件人因此左对齐。
    expect(read.container.querySelector('[data-testid="unread-dot"]')).toBeNull();
  });
});

describe('MessageRow 星标指示', () => {
  const base: MessageHeader = {
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
  };
  it('\\Flagged 时渲染星标', () => {
    render(<MessageRow m={{ ...base, flags: ['\\Flagged'] }} active={false} onClick={vi.fn()} />);
    expect(screen.getByLabelText('已加星')).toBeInTheDocument();
  });
  it('无 \\Flagged 不渲染星标', () => {
    render(<MessageRow m={{ ...base, flags: [] }} active={false} onClick={vi.fn()} />);
    expect(screen.queryByLabelText('已加星')).toBeNull();
  });
});
