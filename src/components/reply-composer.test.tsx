import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { ReplyComposer } from './reply-composer';
import { useMailStore } from '../lib/store/mail';
import { useAiStore } from '../lib/store/ai';
import * as tauri from '../lib/tauri';

vi.mock('../lib/tauri', () => ({
  smtpSend: vi.fn(),
  aiDraftReply: vi.fn(),
}));

describe('ReplyComposer 聚合态发送账户', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal('confirm', () => true);
    useAiStore.setState({ models: [], roleDefaults: [] } as never);
    useMailStore.setState({
      composerOpen: true,
      messages: [
        {
          id: 'm1',
          accountId: 'acc-9',
          subject: 'Hello',
          fromAddr: 'sender@x.com',
          toAddrs: [],
          ccAddrs: [],
          sentAt: null,
          hasAttachment: false,
        },
      ] as never,
      selectedMessageId: 'm1',
      selectedAccountId: null,
    } as never);
  });

  it('全部视图下回复用 message.accountId 而非 selectedAccountId', async () => {
    vi.mocked(tauri.smtpSend).mockResolvedValue({
      sendLog: { id: 'log-1234abcd', smtpResponse: 'OK' },
    } as never);
    render(<ReplyComposer />);
    await userEvent.type(screen.getByLabelText(/正文/), '回复内容');
    await userEvent.click(screen.getByRole('button', { name: '发送' }));
    expect(tauri.smtpSend).toHaveBeenCalledWith(expect.objectContaining({ accountId: 'acc-9' }));
  });
});
