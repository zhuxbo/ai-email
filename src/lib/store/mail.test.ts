// src/lib/store/mail.test.ts
import { describe, it, expect, beforeEach, vi } from 'vitest';

/**
 * 最小 FoldedItem mock（foldKind:'single' 代表单封）。
 * 用 vi.hoisted 提升，确保 vi.mock 工厂函数（被 hoist 到文件顶部）里可安全调用。
 */
const { mkFoldedMock } = vi.hoisted(() => {
  const mkFoldedMock = (id: string, accountId = 'a1') => ({
    id,
    accountId,
    mailboxId: 'm',
    imapUid: 1,
    rfcMessageId: null,
    threadId: null,
    subject: id,
    fromAddr: null,
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
    referencesHeader: null,
    filterDisabled: false,
    categoryLocked: false,
    foldKind: 'single' as const,
    foldKey: `msg:${id}`,
    count: 1,
    hasUnread: false,
  });
  return { mkFoldedMock };
});

import { useMailStore } from './mail';
import { useComposeStore } from './compose';
import type { ConversationView, FoldedItem } from '../types';

vi.mock('../tauri', () => ({
  accountsList: vi.fn().mockResolvedValue([{ id: 'a1' }, { id: 'a2' }]),
  unifiedInbox: vi.fn().mockResolvedValue({ messages: [mkFoldedMock('m1', 'a1')], errors: {} }),
  inboxSync: vi.fn().mockResolvedValue({ newMessageCount: 0, totalInMailbox: 0 }),
  mailboxSync: vi.fn().mockResolvedValue({ newMessageCount: 0, totalInMailbox: 5 }),
  messageBody: vi
    .fn()
    .mockResolvedValue({ messageId: 'm1', textPlain: 'x', html: null, fetchedAt: '' }),
  mailboxesList: vi.fn().mockResolvedValue([]),
  messagesList: vi.fn().mockResolvedValue([]),
  aiClassify: vi.fn().mockResolvedValue(undefined),
  accountAdd: vi.fn(),
  accountRemove: vi.fn().mockResolvedValue(undefined),
  accountUpdate: vi.fn(),
  messageSetSeen: vi.fn().mockResolvedValue(undefined),
  messageSetFlagged: vi.fn().mockResolvedValue(undefined),
  messageDelete: vi.fn().mockResolvedValue(undefined),
  messagesMarkSeenBulk: vi.fn().mockResolvedValue(undefined),
  conversationThread: vi.fn(),
  mailboxFolded: vi.fn().mockResolvedValue([]),
  mailboxMarkSeen: vi.fn().mockResolvedValue(undefined),
  accountInboxMarkSeen: vi.fn().mockResolvedValue(undefined),
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

  it('#12 对抗：同一条消息并发 setFlagged（失败）+ setSeen（成功），回滚不抹掉已成功的 \\Seen', async () => {
    // 这正是旧快照回滚的致命边界：旧实现捕获 originalFlags = []，
    // setFlagged 失败时恢复 [] → 把 setSeen 已成功写入的 \\Seen 一并抹掉。
    useMailStore.setState({
      messages: [{ id: 'm1', accountId: 'a1', flags: [] }] as never,
      error: null,
    } as never);

    // setSeen 立即成功
    vi.mocked(tauri.messageSetSeen).mockResolvedValue(undefined);

    // setFlagged 挂起，之后失败
    let rejectFlagged!: (e: Error) => void;
    vi.mocked(tauri.messageSetFlagged).mockImplementationOnce(
      () =>
        new Promise((_res, rej) => {
          rejectFlagged = rej;
        }),
    );

    // 先发起 setFlagged（捕获旧快照 flags=[]），再发起 setSeen
    const pendingFlagged = useMailStore.getState().setFlagged('m1', true);
    const pendingSeen = useMailStore.getState().setSeen('m1', true);

    // setSeen 先成功
    await pendingSeen;
    expect(useMailStore.getState().messages.find((m) => m.id === 'm1')?.flags).toContain('\\Seen');

    // setFlagged 后失败
    rejectFlagged(new Error('flagged-boom'));
    await pendingFlagged;

    // \\Flagged 应被回滚（本次操作失败）
    expect(useMailStore.getState().messages.find((m) => m.id === 'm1')?.flags).not.toContain(
      '\\Flagged',
    );
    // \\Seen 应仍保留（按粒度回滚，不恢复旧快照）
    expect(useMailStore.getState().messages.find((m) => m.id === 'm1')?.flags).toContain('\\Seen');
    expect(useMailStore.getState().error).toContain('flagged-boom');
  });
});

describe('mail store 打开自动标记已读', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('打开未读邮件即本地标已读并调 messageSetSeen', async () => {
    useMailStore.setState({
      messages: [{ id: 'm1', accountId: 'a1', flags: [] }] as never,
      selectedMessageId: null,
      accountErrors: {},
      error: null,
    } as never);
    await useMailStore.getState().selectMessage('m1');
    expect(useMailStore.getState().messages.find((m) => m.id === 'm1')?.flags).toContain('\\Seen');
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

  it('正文取失败仍标已读（已读不依赖正文取成功——修复漏标 bug）', async () => {
    // 复现 bug：QQ 代理不稳时 messageBody 常失败；旧逻辑把已读绑在正文成功之后导致漏标。
    vi.mocked(tauri.messageBody).mockRejectedValueOnce(new Error('IMAP 连接超时（30s）'));
    useMailStore.setState({
      messages: [{ id: 'm1', accountId: 'a1', flags: [] }] as never,
      selectedMessageId: null,
      accountErrors: {},
      error: null,
    } as never);
    await useMailStore.getState().selectMessage('m1');
    expect(tauri.messageSetSeen).toHaveBeenCalledWith('m1', true);
    expect(useMailStore.getState().messages.find((m) => m.id === 'm1')?.flags).toContain('\\Seen');
  });

  it('IMAP 标已读失败时保持本地已读、不报错（静默，区别于手动 setSeen 的回滚）', async () => {
    vi.mocked(tauri.messageSetSeen).mockRejectedValueOnce(new Error('STORE boom'));
    useMailStore.setState({
      messages: [{ id: 'm1', accountId: 'a1', flags: [] }] as never,
      selectedMessageId: null,
      accountErrors: {},
      error: null,
    } as never);
    await useMailStore.getState().selectMessage('m1');
    await Promise.resolve();
    const s = useMailStore.getState();
    expect(s.messages.find((m) => m.id === 'm1')?.flags).toContain('\\Seen');
    expect(s.error).toBeNull();
  });
});

describe('mail store 全部已读', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('聚合视图：调 accountInboxMarkSeen 覆盖所有账户并 reload', async () => {
    useMailStore.setState({
      accounts: [{ id: 'a1' }, { id: 'a2' }] as never,
      messages: [
        { ...mkFoldedMock('m1', 'a1'), hasUnread: true },
        { ...mkFoldedMock('m2', 'a2'), hasUnread: false },
      ] as never,
      selectedAccountId: null,
      selectedMailboxId: null,
      error: null,
    } as never);
    await useMailStore.getState().markAllSeen();
    expect(tauri.accountInboxMarkSeen).toHaveBeenCalledWith('a1');
    expect(tauri.accountInboxMarkSeen).toHaveBeenCalledWith('a2');
    // reload 走聚合路径
    expect(tauri.unifiedInbox).toHaveBeenCalled();
  });

  it('单信箱选中：调 mailboxMarkSeen(mailboxId) 并 reload', async () => {
    useMailStore.setState({
      accounts: [{ id: 'a1' }] as never,
      messages: [{ ...mkFoldedMock('m1', 'a1'), hasUnread: true }] as never,
      selectedAccountId: 'a1',
      selectedMailboxId: 'box-inbox',
      error: null,
    } as never);
    await useMailStore.getState().markAllSeen();
    expect(tauri.mailboxMarkSeen).toHaveBeenCalledWith('box-inbox');
    expect(tauri.mailboxFolded).toHaveBeenCalledWith('box-inbox', 100);
    expect(tauri.accountInboxMarkSeen).not.toHaveBeenCalled();
  });

  it('单信箱标记失败：记 error 并仍 reload', async () => {
    vi.mocked(tauri.mailboxMarkSeen).mockRejectedValueOnce(new Error('STORE boom'));
    useMailStore.setState({
      accounts: [{ id: 'a1' }] as never,
      messages: [{ ...mkFoldedMock('m1', 'a1'), hasUnread: true }] as never,
      selectedAccountId: 'a1',
      selectedMailboxId: 'box-1',
      error: null,
    } as never);
    await useMailStore.getState().markAllSeen();
    expect(useMailStore.getState().error).toContain('STORE boom');
    expect(tauri.mailboxFolded).toHaveBeenCalled();
  });
});

describe('mail store deleteMessage', () => {
  beforeEach(() => {
    useMailStore.setState({
      messages: [
        { id: 'm1', accountId: 'a1', flags: [] },
        { id: 'm2', accountId: 'a1', flags: [] },
      ] as never,
      selectedAccountId: null,
      selectedMailboxId: null,
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

  it('失败时 reload 恢复 + 记 error + 被移除的行重新出现在列表', async () => {
    // unifiedInbox reload 后返回包含 m1 的列表（模拟后端恢复）
    vi.mocked(tauri.unifiedInbox).mockResolvedValueOnce({
      messages: [
        { id: 'm1', accountId: 'a1', flags: [] },
        { id: 'm2', accountId: 'a1', flags: [] },
      ],
      errors: {},
    } as never);
    vi.mocked(tauri.messageDelete).mockRejectedValueOnce(new Error('boom'));

    // 乐观移除前 m1 存在
    expect(useMailStore.getState().messages.find((m) => m.id === 'm1')).toBeDefined();

    await useMailStore.getState().deleteMessage('m1');

    // 记 error
    expect(useMailStore.getState().error).toContain('boom');
    // reload 触发
    expect(tauri.unifiedInbox).toHaveBeenCalled();
    // 关键断言：被乐观移除的 m1 已通过 reload 恢复到列表
    expect(useMailStore.getState().messages.find((m) => m.id === 'm1')).toBeDefined();
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

describe('#16 selectMessage 切换邮件重置 compose 上下文', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useMailStore.setState({
      messages: [
        { id: 'm-a', accountId: 'acc-1', flags: [] },
        { id: 'm-b', accountId: 'acc-1', flags: [] },
      ] as never,
      selectedMessageId: null,
      messageOpenSeq: 0,
      body: null,
      accountErrors: {},
      error: null,
    } as never);
    // compose 已预填邮件 A 的 replyContext（模拟用户开始回复 A）
    useComposeStore.setState({
      replyContext: { messageId: 'm-a', accountId: 'acc-1' },
      to: 'sender-a@x.com',
      subject: 'Re: A',
      bodyForeign: '已编辑的回复内容',
    } as never);
  });

  it('切换到邮件 B 后 compose replyContext 被清除（不再停留在 A）', async () => {
    // 此时 compose 仍指向邮件 A
    expect(useComposeStore.getState().replyContext?.messageId).toBe('m-a');

    // 用户切换到邮件 B
    await useMailStore.getState().selectMessage('m-b');

    // compose 应被重置，replyContext 清空
    expect(useComposeStore.getState().replyContext).toBeNull();
  });

  it('切换到邮件 B 后 compose 正文也被清除（不把 A 的草稿误发给 B 的发件人）', async () => {
    expect(useComposeStore.getState().bodyForeign).toBe('已编辑的回复内容');

    await useMailStore.getState().selectMessage('m-b');

    // 草稿正文清空，不残留旧邮件的内容
    expect(useComposeStore.getState().bodyForeign).toBe('');
  });

  it('【竞态】body 加载期间切换邮件：compose 随 selectMessage 同步重置，不等 body 返回', async () => {
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

    // 开始加载邮件 B（body 挂起）
    const pendingB = useMailStore.getState().selectMessage('m-b');

    // body 还未返回时，compose 已应被同步重置
    expect(useComposeStore.getState().replyContext).toBeNull();

    // 最终 body 返回，不影响 compose 状态
    resolveBody({ messageId: 'm-b', textPlain: 'body-b', html: null, fetchedAt: '' });
    await pendingB;

    // compose 仍保持重置状态
    expect(useComposeStore.getState().replyContext).toBeNull();
  });

  it('#16 再次点击已选中邮件（同一 id）时草稿保留，不被清空', async () => {
    // 先选中邮件 A，然后在 compose 写入草稿
    useMailStore.setState({ selectedMessageId: 'm-a' } as never);
    useComposeStore.setState({
      replyContext: { messageId: 'm-a', accountId: 'acc-1' },
      bodyForeign: '用户正在输入的草稿',
    } as never);

    // 再次点击同一封邮件（e.g. 点邮件列表中已选中的行）
    await useMailStore.getState().selectMessage('m-a');

    // 草稿应保留，不被 reset 清空
    expect(useComposeStore.getState().bodyForeign).toBe('用户正在输入的草稿');
    expect(useComposeStore.getState().replyContext?.messageId).toBe('m-a');
  });

  it('#16 切换到不同邮件时草稿被 reset', async () => {
    // compose 有邮件 A 的草稿
    useMailStore.setState({ selectedMessageId: 'm-a' } as never);
    useComposeStore.setState({
      replyContext: { messageId: 'm-a', accountId: 'acc-1' },
      bodyForeign: '用户正在输入的草稿',
    } as never);

    // 切换到邮件 B
    await useMailStore.getState().selectMessage('m-b');

    // 草稿应被 reset
    expect(useComposeStore.getState().bodyForeign).toBe('');
    expect(useComposeStore.getState().replyContext).toBeNull();
  });
});

// ────────────────────────────────────────────────────────────────────────────
// Phase 15 多信箱路径单测
// 补全 selectedMailboxId 非 null 时走 messagesList 路径的分支覆盖，
// 并验证 selectMailbox 切换迟到守卫正确工作。
// ────────────────────────────────────────────────────────────────────────────

const INBOX_BOX = { id: 'box-inbox', name: 'INBOX', specialUse: 'inbox', accountId: 'a1' };
const SENT_BOX = { id: 'box-sent', name: 'Sent', specialUse: 'sent', accountId: 'a1' };
const DRAFTS_BOX = { id: 'box-drafts', name: 'Drafts', specialUse: 'drafts', accountId: 'a1' };
const TRASH_BOX = { id: 'box-trash', name: 'Trash', specialUse: 'trash', accountId: 'a1' };

// ────────────────────────────────────────────────────────────────────────────
// classifiedAffectsCurrentView 单元测试（Phase 15 扩展）
// ────────────────────────────────────────────────────────────────────────────
describe('classifiedAffectsCurrentView', () => {
  const allBoxes = [INBOX_BOX, SENT_BOX, DRAFTS_BOX, TRASH_BOX];

  it('聚合 INBOX（selectedMailboxId=null, selectedAccountId=null）→ true', () => {
    useMailStore.setState({
      selectedAccountId: null,
      selectedMailboxId: null,
      mailboxes: allBoxes,
    } as never);
    expect(useMailStore.getState().classifiedAffectsCurrentView()).toBe(true);
  });

  it('单账户默认 INBOX（selectedMailboxId=null）→ true', () => {
    useMailStore.setState({
      selectedAccountId: 'a1',
      selectedMailboxId: null,
      mailboxes: allBoxes,
    } as never);
    expect(useMailStore.getState().classifiedAffectsCurrentView()).toBe(true);
  });

  it('选中 INBOX 信箱（specialUse=inbox）→ true', () => {
    useMailStore.setState({
      selectedAccountId: 'a1',
      selectedMailboxId: INBOX_BOX.id,
      mailboxes: allBoxes,
    } as never);
    expect(useMailStore.getState().classifiedAffectsCurrentView()).toBe(true);
  });

  it('选中 Sent 信箱（specialUse=sent）→ false', () => {
    useMailStore.setState({
      selectedAccountId: 'a1',
      selectedMailboxId: SENT_BOX.id,
      mailboxes: allBoxes,
    } as never);
    expect(useMailStore.getState().classifiedAffectsCurrentView()).toBe(false);
  });

  it('选中 Drafts 信箱（specialUse=drafts）→ false', () => {
    useMailStore.setState({
      selectedAccountId: 'a1',
      selectedMailboxId: DRAFTS_BOX.id,
      mailboxes: allBoxes,
    } as never);
    expect(useMailStore.getState().classifiedAffectsCurrentView()).toBe(false);
  });

  it('选中 Trash 信箱（specialUse=trash）→ false', () => {
    useMailStore.setState({
      selectedAccountId: 'a1',
      selectedMailboxId: TRASH_BOX.id,
      mailboxes: allBoxes,
    } as never);
    expect(useMailStore.getState().classifiedAffectsCurrentView()).toBe(false);
  });

  it('选中未知 id（mailboxes 中找不到）→ false（box 不存在视为非 INBOX）', () => {
    useMailStore.setState({
      selectedAccountId: 'a1',
      selectedMailboxId: 'box-unknown',
      mailboxes: allBoxes,
    } as never);
    expect(useMailStore.getState().classifiedAffectsCurrentView()).toBe(false);
  });

  it('specialUse=null 的普通文件夹 → false', () => {
    const customBox = { id: 'box-custom', name: 'Archive', specialUse: null, accountId: 'a1' };
    useMailStore.setState({
      selectedAccountId: 'a1',
      selectedMailboxId: customBox.id,
      mailboxes: [...allBoxes, customBox],
    } as never);
    expect(useMailStore.getState().classifiedAffectsCurrentView()).toBe(false);
  });
});

describe('mail store 多信箱路径 (Phase 15)', () => {
  beforeEach(() => {
    useMailStore.setState({
      accounts: [{ id: 'a1' }] as never,
      selectedAccountId: 'a1',
      mailboxes: [INBOX_BOX, SENT_BOX] as never,
      selectedMailboxId: INBOX_BOX.id,
      messages: [],
      selectedMessageId: null,
      messageOpenSeq: 0,
      body: null,
      accountErrors: {},
      error: null,
    } as never);
    vi.clearAllMocks();
    vi.mocked(tauri.mailboxFolded).mockResolvedValue([mkFoldedMock('m-s1', 'a1')]);
    vi.mocked(tauri.mailboxSync).mockResolvedValue({ newMessageCount: 0, totalInMailbox: 1 });
  });

  it('selectMailbox 切非 INBOX 信箱：调用 mailboxSync + mailboxFolded，不走 unifiedInbox', async () => {
    await useMailStore.getState().selectMailbox(SENT_BOX.id);

    // 单信箱路径：mailboxFolded 按 mailboxId 拉取折叠列表
    expect(tauri.mailboxFolded).toHaveBeenCalledWith(SENT_BOX.id, 100);
    // 非 INBOX 触发按需同步
    expect(tauri.mailboxSync).toHaveBeenCalledWith('a1', 'Sent');
    // 聚合路径绝不应被调用
    expect(tauri.unifiedInbox).not.toHaveBeenCalled();
  });

  it('INBOX 选中走单信箱 mailboxFolded 路径，不走 unifiedInbox', async () => {
    // selectedMailboxId 已是 INBOX_BOX.id（beforeEach 默认），INBOX 不触发 mailboxSync
    vi.mocked(tauri.mailboxFolded).mockResolvedValue([mkFoldedMock('m-i1', 'a1')]);

    await useMailStore.getState().reloadMessages();

    expect(tauri.mailboxFolded).toHaveBeenCalledWith(INBOX_BOX.id, 100);
    expect(tauri.unifiedInbox).not.toHaveBeenCalled();
    expect(useMailStore.getState().messages[0]?.id).toBe('m-i1');
  });

  it('聚合视图（selectedAccountId=null）仍走 unifiedInbox 不变', async () => {
    useMailStore.setState({
      selectedAccountId: null,
      selectedMailboxId: null,
      mailboxes: [],
    } as never);
    vi.mocked(tauri.unifiedInbox).mockResolvedValue({
      messages: [mkFoldedMock('m-u1', 'a1')],
      errors: {},
    });

    await useMailStore.getState().reloadMessages();

    expect(tauri.unifiedInbox).toHaveBeenCalled();
    expect(tauri.mailboxFolded).not.toHaveBeenCalled();
    expect(useMailStore.getState().messages[0]?.id).toBe('m-u1');
  });

  it('切换迟到守卫：reloadMessages 迟到（mailboxId 已变）时不覆盖新信箱列表', async () => {
    // 直接测试 reloadMessages 里的 selectedMailboxId 守卫：
    // 1. store 设置 selectedMailboxId = SENT_BOX（A）
    // 2. 发起 reloadMessages()（内部调用 mailboxFolded(SENT_BOX)，请求挂起）
    // 3. 在请求返回前，将 selectedMailboxId 切到 INBOX_BOX（B）并拉到 B 的数据
    // 4. A 的响应迟到 resolve → 守卫 selectedMailboxId===SENT_BOX 不成立 → 不覆盖
    let resolveA!: (msgs: FoldedItem[] | PromiseLike<FoldedItem[]>) => void;
    vi.mocked(tauri.mailboxFolded).mockImplementationOnce(
      () =>
        new Promise((res) => {
          resolveA = res;
        }),
    );

    // A：切到 Sent，发起 reloadMessages（挂起）
    useMailStore.setState({ selectedMailboxId: SENT_BOX.id } as never);
    const pendingA = useMailStore.getState().reloadMessages();

    // 模拟用户已切到 B（INBOX）并完成加载
    useMailStore.setState({
      selectedMailboxId: INBOX_BOX.id,
      messages: [mkFoldedMock('m-inbox', 'a1')] as never,
    } as never);

    // A 的迟到响应 resolve — 守卫：get().selectedMailboxId（INBOX）!== SENT_BOX → 不写入
    resolveA([mkFoldedMock('m-sent-late', 'a1')]);
    await pendingA;

    // B 的列表不被覆盖
    expect(useMailStore.getState().messages[0]?.id).toBe('m-inbox');
    expect(useMailStore.getState().selectedMailboxId).toBe(INBOX_BOX.id);
  });
});

describe('mail store loadConversation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useMailStore.setState({
      selectedMessageId: null,
      conversation: null,
      loadingConversation: false,
      error: null,
    } as never);
  });

  it('loadConversation 填充 conversation', async () => {
    const view: ConversationView = { threadId: 't1', sentSyncOk: true, messages: [] };
    vi.mocked(tauri.conversationThread).mockResolvedValue(view);
    useMailStore.setState({ selectedMessageId: 'm1' } as never);
    await useMailStore.getState().loadConversation('m1');
    expect(useMailStore.getState().conversation?.threadId).toBe('t1');
    expect(useMailStore.getState().loadingConversation).toBe(false);
  });

  it('loadConversation 迟到响应不覆盖已切换的会话', async () => {
    let resolveM1: ((v: unknown) => void) | undefined;
    vi.mocked(tauri.conversationThread).mockImplementation(
      () =>
        new Promise((r) => {
          resolveM1 = r;
        }) as never,
    );
    useMailStore.setState({ selectedMessageId: 'm1', conversation: null } as never);
    const p = useMailStore.getState().loadConversation('m1');
    // user switches to m2 while m1 is in flight
    useMailStore.setState({ selectedMessageId: 'm2' } as never);
    resolveM1?.({ threadId: 't1-stale', sentSyncOk: true, messages: [] });
    await p;
    expect(useMailStore.getState().conversation).toBeNull(); // stale m1 response rejected
  });
});

// ────────────────────────────────────────────────────────────────────────────
// B4: FoldedItem state + markAllSeen 走后端范围标记 + 组级未读计数
// ────────────────────────────────────────────────────────────────────────────

describe('B4 markAllSeen 走后端范围标记', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('单信箱选中：调 mailboxMarkSeen(mailboxId) 并 reload', async () => {
    useMailStore.setState({
      accounts: [{ id: 'a1' }] as never,
      selectedAccountId: 'a1',
      selectedMailboxId: 'box-1',
      messages: [mkFoldedMock('m1', 'a1'), mkFoldedMock('m2', 'a1')] as never,
      accountErrors: {},
      error: null,
    } as never);
    vi.mocked(tauri.mailboxMarkSeen).mockResolvedValueOnce(undefined);
    // unifiedInbox/messagesList mock 由 beforeEach 中 vi.mock 提供

    await useMailStore.getState().markAllSeen();

    expect(tauri.mailboxMarkSeen).toHaveBeenCalledWith('box-1');
    // reload 后应调用 mailboxFolded（单信箱路径）
    expect(tauri.mailboxFolded).toHaveBeenCalledWith('box-1', 100);
  });

  it('聚合视图：对各账户 accountInboxMarkSeen 并 allSettled 容错 + reload', async () => {
    useMailStore.setState({
      accounts: [{ id: 'a1' }, { id: 'a2' }] as never,
      selectedAccountId: null,
      selectedMailboxId: null,
      messages: [mkFoldedMock('m1', 'a1'), mkFoldedMock('m2', 'a2')] as never,
      accountErrors: {},
      error: null,
    } as never);
    // a1 成功，a2 失败 — allSettled 不应整体抛
    vi.mocked(tauri.accountInboxMarkSeen).mockImplementation((id: string) =>
      id === 'a1' ? Promise.resolve() : Promise.reject(new Error('a2 boom')),
    );

    await expect(useMailStore.getState().markAllSeen()).resolves.toBeUndefined();

    expect(tauri.accountInboxMarkSeen).toHaveBeenCalledWith('a1');
    expect(tauri.accountInboxMarkSeen).toHaveBeenCalledWith('a2');
    // reload
    expect(tauri.unifiedInbox).toHaveBeenCalled();
  });
});

describe('B4 unreadCount 用组级 hasUnread 计数', () => {
  it('有 hasUnread 的行才计入未读数', () => {
    useMailStore.setState({
      messages: [
        { ...mkFoldedMock('m1'), hasUnread: true },
        { ...mkFoldedMock('m2'), hasUnread: true },
        { ...mkFoldedMock('m3'), hasUnread: false },
      ] as never,
    } as never);
    expect(useMailStore.getState().unreadCount()).toBe(2);
  });

  it('全部已读时 unreadCount 为 0', () => {
    useMailStore.setState({
      messages: [
        { ...mkFoldedMock('m1'), hasUnread: false },
        { ...mkFoldedMock('m2'), hasUnread: false },
      ] as never,
    } as never);
    expect(useMailStore.getState().unreadCount()).toBe(0);
  });
});

describe('mail store updateAccount', () => {
  beforeEach(() => {
    useMailStore.setState({ accounts: [], error: null } as never);
    vi.clearAllMocks();
  });
  it('成功更新后只替换 accounts 中对应项', async () => {
    const updated = {
      id: 'a1',
      email: 'a@qq.com',
      displayName: '新名',
      provider: 'qq',
      imapHost: 'imap.qq.com',
      imapPort: 993,
      smtpHost: 'smtp.qq.com',
      smtpPort: 465,
      createdAt: '',
      lastSyncedAt: null,
    };
    vi.mocked(tauri.accountUpdate).mockResolvedValueOnce(updated);
    useMailStore.setState({
      accounts: [
        { id: 'a1', displayName: '旧' },
        { id: 'a2', displayName: '别动' },
      ] as never,
    } as never);
    await useMailStore.getState().updateAccount('a1', {
      displayName: '新名',
      imapHost: 'imap.qq.com',
      imapPort: 993,
      smtpHost: 'smtp.qq.com',
      smtpPort: 465,
    });
    const accs = useMailStore.getState().accounts;
    expect(accs.find((a) => a.id === 'a1')?.displayName).toBe('新名');
    expect(accs.find((a) => a.id === 'a2')?.displayName).toBe('别动');
  });
  it('更新失败时设置 error 并抛出', async () => {
    vi.mocked(tauri.accountUpdate).mockRejectedValueOnce(new Error('boom'));
    useMailStore.setState({ accounts: [{ id: 'a1' }] as never } as never);
    await expect(
      useMailStore.getState().updateAccount('a1', {
        displayName: null,
        imapHost: 'h',
        imapPort: 993,
        smtpHost: 's',
        smtpPort: 465,
      }),
    ).rejects.toThrow('boom');
    expect(useMailStore.getState().error).toContain('boom');
  });
});
