import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';

import { MessageDetail } from './message-detail';
import { useMailStore } from '../lib/store/mail';

describe('MessageDetail', () => {
  beforeEach(() => {
    useMailStore.setState({
      messages: [
        {
          id: 'm1',
          accountId: 'a1',
          subject: 'S',
          toAddrs: [],
          ccAddrs: [],
          fromAddr: 'x',
          sentAt: null,
          hasAttachment: false,
          flags: [],
        },
      ] as never,
      selectedMessageId: 'm1',
      body: { messageId: 'm1', textPlain: 'B', html: null, fetchedAt: '' },
      loadingBody: false,
    } as never);
  });
  it('渲染主题与正文', () => {
    render(<MessageDetail />);
    expect(screen.getByText('S')).toBeInTheDocument();
    expect(screen.getByText('B')).toBeInTheDocument();
  });
});
