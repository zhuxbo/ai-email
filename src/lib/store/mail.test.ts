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

  it('#12 并发：setSeen 失败精准回滚单条，不覆盖另一个操作的乐观更新', async () => {
    // 初始两条消息，m1/m2 均未读未标星
    useMailStore.setState({
      messages: [
        { id: 'm1', accountId: 'a1', flags: [] },
        { id: 'm2', accountId: 'a1', flags: [] },
      ] as never,
      error: null,
    } as never);

    // m2 的 setSeen 正常成功
    vi.mocked(tauri.messageSetSeen).mockResolvedValue(undefined);

    // m1 的 setFlagged 会失败（挂起，便于控制竞态顺序）
    let rejectFlagged!: (e: Error) => void;
    vi.mocked(tauri.messageSetFlagged).mockImplementationOnce(
      () =>
        new Promise((_res, rej) => {
          rejectFlagged = rej;
        }),
    );

    // 并发：m1 打星（会失败）+ m2 标已读（会成功）
    const pendingFlagged = useMailStore.getState().setFlagged('m1', true);
    const pendingSeen = useMailStore.getState().setSeen('m2', true);

    // m2 先成功
    await pendingSeen;
    expect(useMailStore.getState().messages.find((m) => m.id === 'm2')?.flags).toContain('\\Seen');

    // m1 失败回滚
    rejectFlagged(new Error('flagged-boom'));
    await pendingFlagged;

    // m1 的 \\Flagged 回滚了（精准回滚）
    expect(useMailStore.getState().messages.find((m) => m.id === 'm1')?.flags).not.toContain(
      '\\Flagged',
    );
    // m2 的 \\Seen 不受影响（精准回滚不覆盖整列）
    expect(useMailStore.getState().messages.find((m) => m.id === 'm2')?.flags).toContain('\\Seen');
    expect(useMailStore.getState().error).toContain('flagged-boom');
  });
});

describe('mail store 打开自动标记已读', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('打开未读邮件触发 setSeen(id,true)', async () => {
    useMailStore.setState({
      messages: [{ id: 'm1', accountId: 'a1', flags: [] }] as never,
      selectedMessageId: null,
      accountErrors: {},
      error: null,
    } as never);
    await useMailStore.getState().selectMessage('m1');
    expect(tauri.messageSetSeen).toHaveBeenCalledWith('m1', true);
  });

  it('打开已读邮件不发请求', async () => {
    useMailStore.setState({
      messages: [{ id: 'm1', accountId: 'a1', flags: ['\\Seen'] }] as never,
      selectedMessageId: null,
      accountErrors: {},
      error: null,
    } as never);
    await useMailStore.getState().selectMessage('m1');
    expect(tauri.messageSetSeen).not.toHaveBeenCalled();
  });

  it('body 迟到（已切走）不标已读', async () => {
    useMailStore.setState({
      messages: [{ id: 'm1', accountId: 'a1', flags: [] }] as never,
      selectedMessageId: null,
      accountErrors: {},
      error: null,
    } as never);
    let resolveBody!: (b: {
      messageId: string;
      textPlain: string;
      html: null;
      fetchedAt: string;
    }) => void;
    vi.mocked(tauri.messageBody).mockImplementationOnce(
      () =>
        new Promise((res) => {
          resolveBody = res;
        }),
    );
    const pending = useMailStore.getState().selectMessage('m1');
    // body 加载期间用户切走到 m2 —— 守卫 get().selectedMessageId === 'm1' 将不成立
    useMailStore.setState({ selectedMessageId: 'm2' } as never);
    resolveBody({ messageId: 'm1', textPlain: 'x', html: null, fetchedAt: '' });
    await pending;
    // 迟到守卫不成立 → 不自动标记已读
    expect(tauri.messageSetSeen).not.toHaveBeenCalled();
  });
});

describe('mail store deleteMessage', () => {
  beforeEach(() => {
    useMailStore.setState({
      messages: [
        { id: 'm1', accountId: 'a1', flags: [] },
        { id: 'm2', accountId: 'a1', flags: [] },
      ] as never,
      selectedMessageId: 'm1',
      messageOpenSeq: 7,
      body: { messageId: 'm1', textPlain: 'x', html: null, fetchedAt: '' } as never,
      accountErrors: {},
      error: 'stale',
    } as never);
    vi.clearAllMocks();
  });

  it('乐观移除该行 + 清选中/body + 清 error 不 bump seq', async () => {
    await useMailStore.getState().deleteMessage('m1');
    const s = useMailStore.getState();
    expect(s.messages.find((m) => m.id === 'm1')).toBeUndefined();
    expect(s.selectedMessageId).toBeNull();
    expect(s.body).toBeNull();
    expect(s.messageOpenSeq).toBe(7);
    expect(s.error).toBeNull();
    expect(tauri.messageDelete).toHaveBeenCalledWith('m1');
  });

  it('失败时 reload 恢复 + 记 error', async () => {
    vi.mocked(tauri.messageDelete).mockRejectedValueOnce(new Error('boom'));
    await useMailStore.getState().deleteMessage('m1');
    expect(useMailStore.getState().error).toContain('boom');
    expect(tauri.unifiedInbox).toHaveBeenCalled(); // reloadMessages 触发
  });

  it('删非选中项不动 selectedMessageId/body', async () => {
    await useMailStore.getState().deleteMessage('m2');
    const s = useMailStore.getState();
    expect(s.messages.find((m) => m.id === 'm2')).toBeUndefined();
    expect(s.selectedMessageId).toBe('m1');
    expect(s.body).not.toBeNull();
    expect(s.messageOpenSeq).toBe(7);
  });
});
