// src/components/message-actions.test.tsx
import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MessageActions } from './message-actions';
import { useMailStore } from '../lib/store/mail';
import { useComposeStore } from '../lib/store/compose';
import { useUiStore } from '../lib/store/ui';

describe('MessageActions', () => {
  beforeEach(() => {
    useMailStore.setState({
      selectedMessageId: 'm1',
      body: { messageId: 'm1', textPlain: 'x', html: null, fetchedAt: '' },
      messages: [
        {
          id: 'm1',
          accountId: 'a1',
          subject: 's',
          fromAddr: 'x',
          toAddrs: [],
          ccAddrs: [],
          sentAt: null,
          snippet: null,
        } as never,
      ],
    } as never);
    useUiStore.setState({ drawerOpen: false, drawerTab: 'summary' } as never);
  });

  it('回复打开 compose tab', async () => {
    render(<MessageActions />);
    await userEvent.click(screen.getByRole('button', { name: '回复' }));
    expect(useUiStore.getState().drawerTab).toBe('compose');
    expect(useUiStore.getState().drawerOpen).toBe(true);
    expect(useComposeStore.getState().replyContext?.messageId).toBe('m1');
  });

  it('翻译打开 translate tab', async () => {
    render(<MessageActions />);
    await userEvent.click(screen.getByRole('button', { name: '翻译' }));
    expect(useUiStore.getState().drawerTab).toBe('translate');
    expect(useUiStore.getState().drawerOpen).toBe(true);
  });

  it('摘要打开 summary tab', async () => {
    render(<MessageActions />);
    await userEvent.click(screen.getByRole('button', { name: '摘要' }));
    expect(useUiStore.getState().drawerTab).toBe('summary');
    expect(useUiStore.getState().drawerOpen).toBe(true);
  });
});
