// #24: runSend 成功后主动刷新邮件列表与建议回复队列。
// 验证 reloadMessages 与 loadQueue 在发送成功后被调用。

import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { useComposeStore } from './compose';
import type { MessageHeader } from '../types';

vi.mock('../tauri', () => ({
  aiDraftReply: vi.fn(),
  aiTranslateText: vi.fn(),
  smtpSend: vi.fn(),
}));
import * as tauri from '../tauri';

const reloadMessagesSpy = vi.fn().mockResolvedValue(undefined);
const loadQueueSpy = vi.fn().mockResolvedValue(undefined);

vi.mock('./mail', () => ({
  useMailStore: {
    getState: () => ({
      reloadMessages: reloadMessagesSpy,
      selectedAccountId: null,
      accounts: [],
    }),
  },
}));

vi.mock('./auto-reply', () => ({
  useAutoReplyStore: {
    getState: () => ({
      loadQueue: loadQueueSpy,
    }),
  },
}));

vi.mock('./ui', () => ({
  useUiStore: {
    getState: () => ({ closeDrawer: vi.fn() }),
  },
}));

const enMsg: MessageHeader = {
  id: 'm-en',
  accountId: 'acc-9',
  mailboxId: 'mb',
  imapUid: 1,
  rfcMessageId: null,
  threadId: null,
  subject: 'Meeting next week',
  fromAddr: 'bob@x.com',
  toAddrs: [],
  ccAddrs: [],
  sentAt: null,
  internalDate: null,
  flags: [],
  sizeBytes: null,
  hasAttachment: false,
  snippet: 'Can we reschedule?',
  priority: null,
  category: null,
  tags: [],
  bodyFetchedAt: null,
};

describe('#24 runSend 成功后刷新列表与队列', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    vi.stubGlobal('confirm', () => true);
    useComposeStore.getState().reset();
    vi.mocked(tauri.smtpSend).mockResolvedValue({
      sendLog: { id: 'log-aaaa', smtpResponse: 'OK' },
    } as never);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('发送成功后立即调用 reloadMessages', async () => {
    useComposeStore.getState().openReply(enMsg);
    useComposeStore.getState().setField({ bodyForeign: 'hello' });
    await useComposeStore.getState().runSend();

    expect(reloadMessagesSpy).toHaveBeenCalledTimes(1);
  });

  it('发送成功后立即调用 loadQueue', async () => {
    useComposeStore.getState().openReply(enMsg);
    useComposeStore.getState().setField({ bodyForeign: 'hello' });
    await useComposeStore.getState().runSend();

    expect(loadQueueSpy).toHaveBeenCalledTimes(1);
  });

  it('发送失败时不调用 reloadMessages 和 loadQueue', async () => {
    vi.mocked(tauri.smtpSend).mockRejectedValue(new Error('SMTP error'));
    useComposeStore.getState().openReply(enMsg);
    useComposeStore.getState().setField({ bodyForeign: 'hello' });
    await useComposeStore.getState().runSend();

    expect(reloadMessagesSpy).not.toHaveBeenCalled();
    expect(loadQueueSpy).not.toHaveBeenCalled();
  });
});
