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
});
