import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
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
  referencesHeader: null,
  filterDisabled: false,
};

// M2：提为模块级稳定 spy，使"未关抽屉"可断言（而非每次 getState() 返回不同实例）
const closeDrawerSpy = vi.fn();
vi.mock('./ui', () => ({
  useUiStore: {
    getState: () => ({ closeDrawer: closeDrawerSpy }),
  },
}));

describe('compose store', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    closeDrawerSpy.mockClear();
    vi.useFakeTimers();
    useComposeStore.getState().reset();
  });

  afterEach(() => {
    vi.useRealTimers();
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
    // 必须用 replyContext 中锁定的账户（acc-9），而非当前选中账户（acc-OTHER）
    expect(tauri.smtpSend).toHaveBeenCalledWith(
      expect.objectContaining({ accountId: 'acc-9', inReplyTo: 'm-en' }),
    );
    // 反证：不应使用 selectedAccountId 的值
    expect(tauri.smtpSend).not.toHaveBeenCalledWith(
      expect.objectContaining({ accountId: 'acc-OTHER' }),
    );
  });

  it('#48 切换发件账户（setField fromAccountId）后 runSend 用新账户', async () => {
    vi.mocked(tauri.smtpSend).mockResolvedValue({
      sendLog: { id: 'log-48xx', smtpResponse: 'OK' },
    } as never);
    vi.stubGlobal('confirm', () => true);
    // openBlank 场景：用户手动选择发件账户
    useMailStore.setState({
      selectedAccountId: 'acc-default',
      accounts: [{ id: 'acc-default' }, { id: 'acc-switched' }] as never,
    } as never);
    useComposeStore.getState().openBlank();
    expect(useComposeStore.getState().fromAccountId).toBe('acc-default');
    // 用户切换发件账户
    useComposeStore.getState().setField({ fromAccountId: 'acc-switched' });
    expect(useComposeStore.getState().fromAccountId).toBe('acc-switched');
    useComposeStore.getState().setField({ bodyForeign: 'hello switched' });
    await useComposeStore.getState().runSend();
    // 应当使用用户切换后的 acc-switched，而非初始的 acc-default
    expect(tauri.smtpSend).toHaveBeenCalledWith(
      expect.objectContaining({ accountId: 'acc-switched' }),
    );
    expect(tauri.smtpSend).not.toHaveBeenCalledWith(
      expect.objectContaining({ accountId: 'acc-default' }),
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

  it('#68 同一邮件连续起草：第二次请求响应覆盖第一次，旧响应被丢弃', async () => {
    // 第一次 runDraft 挂起
    let resolveFirst!: (v: unknown) => void;
    vi.mocked(tauri.aiDraftReply)
      .mockReturnValueOnce(
        new Promise((res) => {
          resolveFirst = res;
        }) as never,
      )
      .mockResolvedValueOnce({
        subject: 'Re: s',
        body: 'SECOND BODY',
        tone: 'friendly',
        source: 'fresh',
        model: 'm',
        inputTokens: null,
        outputTokens: null,
        cacheReadTokens: null,
      } as never);
    vi.mocked(tauri.aiTranslateText).mockResolvedValue({ text: '第二次回译' });

    useComposeStore.getState().openReply(enMsg);
    const firstPending = useComposeStore.getState().runDraft();

    // 第二次 runDraft 在第一次还未返回时发起
    const secondPending = useComposeStore.getState().runDraft();
    await secondPending;

    // 第一次响应姗姗来迟，应被丢弃
    resolveFirst({ subject: 'Re: s', body: 'FIRST BODY', tone: 'friendly' });
    await firstPending;

    // 只有第二次的结果生效
    expect(useComposeStore.getState().bodyForeign).toBe('SECOND BODY');
  });

  it('#13 runSend 成功后 linger 计时器：期间开新草稿时不 reset 新草稿', async () => {
    vi.mocked(tauri.smtpSend).mockResolvedValue({
      sendLog: { id: 'log-bbbb', smtpResponse: 'OK' },
    } as never);
    vi.stubGlobal('confirm', () => true);

    useComposeStore.getState().openReply(enMsg);
    useComposeStore.getState().setField({ bodyForeign: 'hello' });
    await useComposeStore.getState().runSend();

    // 发送成功后、linger 计时器触发前，用户打开另一封邮件的回复
    const newMsg: MessageHeader = { ...enMsg, id: 'm-new', subject: 'New thread' };
    useComposeStore.getState().openReply(newMsg);
    useComposeStore.getState().setField({ bodyForeign: 'new draft content' });

    // 推进 linger 计时器
    vi.runAllTimers();

    // 新草稿不应被 reset
    expect(useComposeStore.getState().bodyForeign).toBe('new draft content');
    expect(useComposeStore.getState().replyContext?.messageId).toBe('m-new');
  });

  it('#18 runDraft 后 bilingual 随 draft 真实语言重判（snippet 为中文但草稿为英文）', async () => {
    const zhMsg: MessageHeader = {
      ...enMsg,
      id: 'm-zh',
      subject: '下周会议',
      snippet: '我们能把会议改到下周一吗？这件事很重要，请尽快确认。',
    };
    // snippet 全中文 → openReply 时 bilingual=false
    useComposeStore.getState().openReply(zhMsg);
    expect(useComposeStore.getState().bilingual).toBe(false);

    // 但 AI 起草了英文回复
    vi.mocked(tauri.aiDraftReply).mockResolvedValue({
      subject: 'Re: 下周会议',
      body: 'Sure, Monday works for me.',
      tone: 'friendly',
      source: 'fresh',
      model: 'm',
      inputTokens: null,
      outputTokens: null,
      cacheReadTokens: null,
    } as never);
    vi.mocked(tauri.aiTranslateText).mockResolvedValue({ text: '可以，周一适合我。' });

    await useComposeStore.getState().runDraft();

    // draft 为英文 → bilingual 应被更新为 true，并触发回译
    expect(useComposeStore.getState().bilingual).toBe(true);
    expect(useComposeStore.getState().bodyZhBack).toBe('可以，周一适合我。');
  });

  it('#52 runDraft 结果的 source 字段保留在 store', async () => {
    vi.mocked(tauri.aiDraftReply).mockResolvedValue({
      subject: 'Re: Meeting next week',
      body: 'Sure, Monday works.',
      tone: 'friendly',
      source: 'cached',
      model: 'haiku',
      inputTokens: 100,
      outputTokens: 50,
      cacheReadTokens: 80,
    } as never);
    vi.mocked(tauri.aiTranslateText).mockResolvedValue({ text: '可以。' });

    useComposeStore.getState().openReply(enMsg);
    await useComposeStore.getState().runDraft();

    expect(useComposeStore.getState().draftSource).toBe('cached');
  });

  it('#13 对抗：发送邮件 A 后对同一封 A 重开 reply，linger 不清新草稿', async () => {
    // 同封邮件（同 messageId + accountId）重开是 bug 的核心边界：
    // 旧守卫仅比对 messageId+accountId，两者相同 → 误判为同一会话 → 清掉新草稿。
    vi.mocked(tauri.smtpSend).mockResolvedValue({
      sendLog: { id: 'log-cc', smtpResponse: 'OK' },
    } as never);
    vi.stubGlobal('confirm', () => true);

    // 发送邮件 A
    useComposeStore.getState().openReply(enMsg); // enMsg.id = 'm-en', accountId = 'acc-9'
    useComposeStore.getState().setField({ bodyForeign: 'first reply' });
    await useComposeStore.getState().runSend();

    // linger 触发前，对同一封 A（相同 messageId+accountId）重开并键入新内容
    useComposeStore.getState().openReply(enMsg); // 同 id='m-en', accountId='acc-9'
    useComposeStore.getState().setField({ bodyForeign: 'second reply to same mail' });

    // 推进 linger 计时器
    vi.runAllTimers();

    // 新草稿不应被清除
    expect(useComposeStore.getState().bodyForeign).toBe('second reply to same mail');
    expect(useComposeStore.getState().replyContext?.messageId).toBe('m-en');
    // closeDrawer 不应被调用（抽屉未关）
    expect(closeDrawerSpy).not.toHaveBeenCalled();
  });
});

describe('#71 runDraft force 参数透传', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    useComposeStore.getState().reset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('默认调用（无 force）→ 后端收到 force=false', async () => {
    vi.mocked(tauri.aiDraftReply).mockResolvedValue({
      subject: 'Re: s',
      body: 'body',
      tone: 'friendly',
      source: 'cached',
      model: 'm',
      inputTokens: null,
      outputTokens: null,
      cacheReadTokens: null,
    } as never);
    vi.mocked(tauri.aiTranslateText).mockResolvedValue({ text: '翻译' });

    useComposeStore.getState().openReply(enMsg);
    await useComposeStore.getState().runDraft();

    expect(tauri.aiDraftReply).toHaveBeenCalledWith('m-en', null, false);
  });

  it('force=true 时传给后端（重新生成）', async () => {
    vi.mocked(tauri.aiDraftReply).mockResolvedValue({
      subject: 'Re: s',
      body: 'regenerated body',
      tone: 'friendly',
      source: 'fresh',
      model: 'm',
      inputTokens: null,
      outputTokens: null,
      cacheReadTokens: null,
    } as never);
    vi.mocked(tauri.aiTranslateText).mockResolvedValue({ text: '重新生成回译' });

    useComposeStore.getState().openReply(enMsg);
    await useComposeStore.getState().runDraft(true);

    expect(tauri.aiDraftReply).toHaveBeenCalledWith('m-en', null, true);
    expect(useComposeStore.getState().bodyForeign).toBe('regenerated body');
  });
});

describe('#17 runDraft 不覆盖起草期间用户编辑的正文', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    useComposeStore.getState().reset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('【竞态核心】起草 in-flight 期间用户修改正文 → 起草结果不覆盖用户编辑', async () => {
    let resolveDraft!: (v: unknown) => void;
    vi.mocked(tauri.aiDraftReply).mockReturnValue(
      new Promise((res) => {
        resolveDraft = res;
      }) as never,
    );

    useComposeStore.getState().openReply(enMsg);
    const draftPending = useComposeStore.getState().runDraft();

    // 起草 in-flight 期间，用户手动编辑了正文（setField 设 aiAssisted=false）
    useComposeStore.getState().setField({ bodyForeign: '用户自己写的内容' });
    expect(useComposeStore.getState().aiAssisted).toBe(false);

    // 起草结果返回
    resolveDraft({
      subject: 'Re: Meeting next week',
      body: 'AI 起草的内容',
      tone: 'friendly',
      source: 'fresh',
      model: 'm',
      inputTokens: null,
      outputTokens: null,
      cacheReadTokens: null,
    });
    await draftPending;

    // 用户编辑的内容不应被 AI 草稿覆盖
    expect(useComposeStore.getState().bodyForeign).toBe('用户自己写的内容');
    expect(useComposeStore.getState().aiAssisted).toBe(false);
  });

  it('起草期间未编辑正文 → 起草结果正常写入', async () => {
    vi.mocked(tauri.aiDraftReply).mockResolvedValue({
      subject: 'Re: Meeting next week',
      body: 'AI 起草的内容',
      tone: 'friendly',
      source: 'fresh',
      model: 'm',
      inputTokens: null,
      outputTokens: null,
      cacheReadTokens: null,
    } as never);
    vi.mocked(tauri.aiTranslateText).mockResolvedValue({ text: '可以。' });

    useComposeStore.getState().openReply(enMsg);
    await useComposeStore.getState().runDraft();

    // 用户没有编辑过（aiAssisted 仍为 true），草稿写入
    expect(useComposeStore.getState().bodyForeign).toBe('AI 起草的内容');
    expect(useComposeStore.getState().aiAssisted).toBe(true);
  });
});
