// src/lib/store/mail.test.ts
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useMailStore } from './mail';
vi.mock('../tauri', () => ({
  accountsList: vi.fn().mockResolvedValue([{ id: 'a1' }, { id: 'a2' }]),
  unifiedInbox: vi
    .fn()
    .mockResolvedValue({ messages: [{ id: 'm1', accountId: 'a1' }], errors: {} }),
  inboxSync: vi.fn().mockResolvedValue({ newMessageCount: 0, totalInMailbox: 0 }),
  messageBody: vi
    .fn()
    .mockResolvedValue({ messageId: 'm1', textPlain: 'x', html: null, fetchedAt: '' }),
  mailboxesList: vi.fn().mockResolvedValue([]),
  messagesList: vi.fn().mockResolvedValue([]),
  aiClassify: vi.fn().mockResolvedValue(undefined),
  accountAdd: vi.fn(),
  accountRemove: vi.fn().mockResolvedValue(undefined),
  messageSetSeen: vi.fn().mockResolvedValue(undefined),
  messageSetFlagged: vi.fn().mockResolvedValue(undefined),
  messageDelete: vi.fn().mockResolvedValue(undefined),
}));
import * as tauri from '../tauri';
describe('mail store 聚合新成员', () => {
  beforeEach(() => {
    useMailStore.setState({
      accounts: [{ id: 'a1' }, { id: 'a2' }] as never,
      selectedAccountId: null,
      messages: [],
      selectedMessageId: 'm1',
      messageOpenSeq: 5,
      accountErrors: {},
    } as never);
    vi.clearAllMocks();
  });
  it('setFilter 切筛选 + 清选中 + 不 bump seq + reload', async () => {
    await useMailStore.getState().setFilter('a1');
    const s = useMailStore.getState();
    expect(s.selectedAccountId).toBe('a1');
    expect(s.selectedMessageId).toBeNull();
    expect(s.messageOpenSeq).toBe(5);
    expect(tauri.unifiedInbox).toHaveBeenCalledWith({ accountId: 'a1' });
  });
  it('syncInbox 聚合并行：a1 失败不阻塞 a2，记 accountErrors', async () => {
    vi.mocked(tauri.inboxSync).mockImplementation((id: string) =>
      id === 'a1'
        ? Promise.reject(new Error('boom'))
        : Promise.resolve({ newMessageCount: 1, totalInMailbox: 1 }),
    );
    await useMailStore.getState().syncInbox();
    expect(useMailStore.getState().accountErrors.a1).toContain('boom');
    expect(useMailStore.getState().accountErrors.a2).toBeUndefined();
  });
  it('reloadMessages 整体失败时清空 accountErrors，不残留过时 per-account 错误', async () => {
    useMailStore.setState({ accountErrors: { a1: 'old' } } as never);
    vi.mocked(tauri.unifiedInbox).mockRejectedValueOnce(new Error('accountsList boom'));
    await useMailStore.getState().reloadMessages();
    expect(useMailStore.getState().accountErrors).toEqual({});
    expect(useMailStore.getState().error).toContain('boom');
  });
});

describe('mail store flag 乐观更新', () => {
  beforeEach(() => {
    useMailStore.setState({
      messages: [{ id: 'm1', accountId: 'a1', flags: [] }] as never,
      selectedMessageId: 'm1',
      accountErrors: {},
      error: null,
    } as never);
    vi.clearAllMocks();
  });

  it('setSeen 乐观加 \\Seen 并调命令', async () => {
    await useMailStore.getState().setSeen('m1', true);
    const [msg] = useMailStore.getState().messages;
    expect(msg?.flags).toContain('\\Seen');
    expect(tauri.messageSetSeen).toHaveBeenCalledWith('m1', true);
  });

  it('setFlagged 失败回滚 + 记 error', async () => {
    let rejectCall!: (e: Error) => void;
    vi.mocked(tauri.messageSetFlagged).mockImplementationOnce(
      () =>
        new Promise((_resolve, reject) => {
          rejectCall = reject;
        }),
    );
    const pending = useMailStore.getState().setFlagged('m1', true);
    // 命令仍 in-flight：乐观写已发生
    expect(useMailStore.getState().messages[0]?.flags).toContain('\\Flagged');
    rejectCall(new Error('boom'));
    await pending;
    // 失败后回滚 + 记 error
    expect(useMailStore.getState().messages[0]?.flags).not.toContain('\\Flagged');
    expect(useMailStore.getState().error).toContain('boom');
  });

  it('setSeen 成功清除遗留 error', async () => {
    useMailStore.setState({ error: 'old error' } as never);
    await useMailStore.getState().setSeen('m1', true);
    expect(useMailStore.getState().error).toBeNull();
  });
});
