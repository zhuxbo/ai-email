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
};

describe('MessageRow', () => {
  it('渲染主题且带来源色点', () => {
    useMailStore.setState({ accounts: [{ id: 'a1', email: 'acc@x.com' }] as never } as never);
    const { container } = render(<MessageRow m={m} active={false} onClick={vi.fn()} />);
    expect(screen.getByText('主题')).toBeInTheDocument();
    expect(container.querySelector('[data-testid="source-dot"]')).toBeTruthy();
  });
});
