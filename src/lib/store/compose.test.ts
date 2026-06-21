import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useComposeStore } from './compose';
import { useMailStore } from './mail';
import type { MessageHeader } from '../types';

vi.mock('../tauri', () => ({
  aiDraftReply: vi.fn(),
  aiTranslateText: vi.fn(),
  smtpSend: vi.fn(),
}));
import * as tauri from '../tauri';

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
  snippet: 'Can we reschedule the meeting to next Monday?',
  priority: null,
  category: null,
  tags: [],
  bodyFetchedAt: null,
};

describe('compose store', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useComposeStore.getState().reset();
  });

  it('openReply 预填 + 锁定账户 + 外文判双语', () => {
    useComposeStore.getState().openReply(enMsg);
    const s = useComposeStore.getState();
    expect(s.replyContext).toEqual({ messageId: 'm-en', accountId: 'acc-9' });
    expect(s.fromAccountId).toBe('acc-9');
    expect(s.to).toBe('bob@x.com');
    expect(s.subject).toBe('Re: Meeting next week');
    expect(s.bilingual).toBe(true);
  });

  it('runDraft：英文草稿→回译填对照', async () => {
    vi.mocked(tauri.aiDraftReply).mockResolvedValue({
      subject: 'Re: Meeting next week',
      body: 'Sure, Monday works.',
      tone: 'friendly',
    } as never);
    vi.mocked(tauri.aiTranslateText).mockResolvedValue({ text: '可以，周一可行。' });
    useComposeStore.getState().openReply(enMsg);
    await useComposeStore.getState().runDraft();
    const s = useComposeStore.getState();
    expect(s.bodyForeign).toBe('Sure, Monday works.');
    expect(s.bodyZhBack).toBe('可以，周一可行。');
    expect(s.aiAssisted).toBe(true);
  });

  it('runSend 用 replyContext.accountId（破 C1）', async () => {
    vi.mocked(tauri.smtpSend).mockResolvedValue({
      sendLog: { id: 'log-aaaa', smtpResponse: 'OK' },
    } as never);
    vi.stubGlobal('confirm', () => true);
    useMailStore.setState({ selectedAccountId: 'acc-OTHER', accounts: [] } as never);
    useComposeStore.getState().openReply(enMsg);
    useComposeStore.getState().setField({ bodyForeign: 'hi' });
    await useComposeStore.getState().runSend();
    expect(tauri.smtpSend).toHaveBeenCalledWith(
      expect.objectContaining({ accountId: 'acc-9', inReplyTo: 'm-en' }),
    );
  });

  it('写新邮件 runDraft 早退（replyContext null 不崩）', async () => {
    useComposeStore.getState().openBlank();
    await useComposeStore.getState().runDraft();
    expect(tauri.aiDraftReply).not.toHaveBeenCalled();
  });

  it('runDraft 迟到响应被守卫丢弃（draftingFor 不匹配不写入）', async () => {
    let resolveDraft!: (v: unknown) => void;
    vi.mocked(tauri.aiDraftReply).mockReturnValue(
      new Promise((res) => {
        resolveDraft = res;
      }) as never,
    );
    useComposeStore.getState().openReply(enMsg); // draftingFor = 'm-en'
    const pending = useComposeStore.getState().runDraft(); // awaits the pending draft
    // a newer reply starts → openReply resets state, draftingFor becomes null
    useComposeStore.getState().openReply({ ...enMsg, id: 'm-other' });
    resolveDraft({ subject: 'Re: stale', body: 'STALE BODY', tone: 'friendly' });
    await pending;
    // the stale draft must NOT have written into the (now-reset) state
    expect(useComposeStore.getState().bodyForeign).toBe('');
  });

  it('refreshBackTranslation 迟到回译被守卫丢弃（切邮件不串台）', async () => {
    let resolveBack!: (v: unknown) => void;
    vi.mocked(tauri.aiTranslateText).mockReturnValue(
      new Promise((res) => {
        resolveBack = res;
      }) as never,
    );
    useComposeStore.getState().openReply(enMsg); // replyContext = m-en, bilingual=true
    useComposeStore.setState({ bodyForeign: 'hello' } as never);
    const pending = useComposeStore.getState().refreshBackTranslation();
    // switch to a different reply before the back-translation resolves
    useComposeStore.getState().openReply({ ...enMsg, id: 'm-other' });
    resolveBack({ text: '迟到回译' });
    await pending;
    expect(useComposeStore.getState().bodyZhBack).toBeNull(); // not polluted into m-other
  });
});
