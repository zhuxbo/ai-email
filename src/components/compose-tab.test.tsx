import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ComposeTab } from './compose-tab';
import { useComposeStore } from '../lib/store/compose';

vi.mock('../lib/tauri', () => ({
  aiDraftReply: vi.fn(),
  aiTranslateText: vi.fn(),
  smtpSend: vi.fn(),
}));
import * as tauri from '../lib/tauri';

describe('ComposeTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal('confirm', () => true);
    useComposeStore.setState({
      replyContext: { messageId: 'm1', accountId: 'acc-9' },
      fromAccountId: 'acc-9',
      to: 'bob@x.com',
      cc: '',
      subject: 'Re: Hi',
      intentZh: '',
      bodyForeign: 'hello',
      bodyZhBack: null,
      bilingual: false,
      aiAssisted: false,
      drafting: false,
      backTranslating: false,
      sending: false,
      draftingFor: null,
      error: null,
      receiptInfo: null,
    } as never);
  });
  it('发送走 message.accountId', async () => {
    vi.mocked(tauri.smtpSend).mockResolvedValue({
      sendLog: { id: 'log-1234', smtpResponse: 'OK' },
    } as never);
    render(<ComposeTab />);
    await userEvent.click(screen.getByRole('button', { name: '发送' }));
    expect(tauri.smtpSend).toHaveBeenCalledWith(expect.objectContaining({ accountId: 'acc-9' }));
  });

  it('新邮件模式不显示中文意图/AI 起草', () => {
    useComposeStore.setState({ replyContext: null, fromAccountId: 'acc-9' } as never);
    render(<ComposeTab />);
    expect(screen.queryByRole('button', { name: 'AI 起草' })).toBeNull();
  });

  it('双语且有回译时显示中文对照刷新', () => {
    useComposeStore.setState({ bilingual: true, bodyZhBack: '可以，周一可行。' } as never);
    render(<ComposeTab />);
    expect(screen.getByRole('button', { name: '刷新对照' })).toBeInTheDocument();
  });
});
