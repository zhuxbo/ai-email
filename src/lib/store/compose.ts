import { create } from 'zustand';

import * as tauri from '../tauri';
import type { MessageHeader } from '../types';
import { errMsg } from '../utils';
import { useMailStore } from './mail';
import { useUiStore } from './ui';

/** 发送成功后抽屉停留展示回执的时长；到点无条件 reset + 关抽屉（两者皆幂等）。 */
const SEND_SUCCESS_LINGER_MS = 1200;

function parseAddrs(s: string): string[] {
  return s
    .split(',')
    .map((x) => x.trim())
    .filter((x) => x !== '');
}

/** CJK 占比启发式：非空且占比低 → 判为外文。 */
function detectForeign(text: string): boolean {
  const t = text.trim();
  if (t === '') return false;
  const cjk = (t.match(/[一-鿿぀-ヿ]/g) ?? []).length;
  return cjk / t.length < 0.15;
}

function defaultSubject(original: string | null): string {
  if (original === null || original.trim() === '') return 'Re:';
  const s = original.trim();
  return /^re:\s/i.test(s) ? s : `Re: ${s}`;
}

interface ComposeState {
  replyContext: { messageId: string; accountId: string } | null;
  fromAccountId: string | null;
  to: string;
  cc: string;
  subject: string;
  intentZh: string;
  bodyForeign: string;
  bodyZhBack: string | null;
  bilingual: boolean;
  aiAssisted: boolean;
  drafting: boolean;
  backTranslating: boolean;
  sending: boolean;
  /** 当前正在起草的邮件 messageId；用于丢弃迟到响应（陈旧闭包竞态守卫）。*/
  draftingFor: string | null;
  /** #52 最近一次 runDraft 的缓存来源（fresh / cached），供 UI 显示缓存命中。*/
  draftSource: 'fresh' | 'cached' | null;
  error: string | null;
  receiptInfo: string | null;

  openReply: (
    m: Pick<MessageHeader, 'id' | 'accountId' | 'fromAddr' | 'subject' | 'snippet'>,
  ) => void;
  openBlank: () => void;
  setField: (
    patch: Partial<
      Pick<ComposeState, 'to' | 'cc' | 'subject' | 'intentZh' | 'bodyForeign' | 'fromAccountId'>
    >,
  ) => void;
  runDraft: () => Promise<void>;
  refreshBackTranslation: () => Promise<void>;
  runSend: () => Promise<void>;
  reset: () => void;
}

const BLANK = {
  replyContext: null,
  fromAccountId: null,
  to: '',
  cc: '',
  subject: '',
  intentZh: '',
  bodyForeign: '',
  bodyZhBack: null,
  bilingual: false,
  aiAssisted: false,
  drafting: false,
  backTranslating: false,
  sending: false,
  draftingFor: null,
  draftSource: null,
  error: null,
  receiptInfo: null,
} as const;

export const useComposeStore = create<ComposeState>((set, get) => ({
  ...BLANK,

  openReply: (m) => {
    set({
      ...BLANK,
      replyContext: { messageId: m.id, accountId: m.accountId },
      fromAccountId: m.accountId,
      to: m.fromAddr ?? '',
      subject: defaultSubject(m.subject),
      bilingual: detectForeign((m.subject ?? '') + ' ' + (m.snippet ?? '')),
    });
  },

  openBlank: () => {
    const { selectedAccountId, accounts } = useMailStore.getState();
    set({ ...BLANK, fromAccountId: selectedAccountId ?? accounts[0]?.id ?? null });
  },

  setField: (patch) => {
    if ('bodyForeign' in patch) {
      set({ ...patch, aiAssisted: false });
    } else {
      set(patch);
    }
  },

  runDraft: async () => {
    const ctx = get().replyContext;
    if (ctx === null) return;
    // #68 每次起草前用递增 token 标记本次请求身份，取代单纯比对 messageId。
    // 同一邮件连续起草时，旧 token 与新 token 不同，旧响应返回后守卫不匹配即丢弃。
    const token = (get().draftingFor ?? '') + ':' + Date.now().toString();
    set({ drafting: true, draftingFor: token, error: null });
    try {
      const draft = await tauri.aiDraftReply(ctx.messageId, get().intentZh.trim() || null);
      // 守卫：draftingFor 已被更新（新起草）时丢弃本次响应
      if (get().draftingFor !== token) return;

      // #18 bilingual 基于实际草稿语言重判，而非仅依赖 openReply 时的 subject+snippet 判断
      const draftIsForeign = detectForeign(draft.body);
      const newBilingual = draftIsForeign;

      if (get().subject.trim() === '') {
        set({
          bodyForeign: draft.body,
          aiAssisted: true,
          subject: draft.subject,
          bilingual: newBilingual,
          // #52 保留 source 供 UI 显示缓存命中
          draftSource: draft.source,
        });
      } else {
        set({
          bodyForeign: draft.body,
          aiAssisted: true,
          bilingual: newBilingual,
          draftSource: draft.source,
        });
      }

      if (draftIsForeign) {
        const back = await tauri.aiTranslateText(draft.body, 'zh-CN');
        if (get().draftingFor === token) set({ bodyZhBack: back.text });
      } else {
        set({ bodyZhBack: null });
      }
    } catch (e) {
      if (get().draftingFor === token) set({ error: errMsg(e) });
    } finally {
      // #68 起草完成/失败后重置 draftingFor 为 null，避免下次同邮件起草复用旧 token
      if (get().draftingFor === token) set({ drafting: false, draftingFor: null });
    }
  },

  refreshBackTranslation: async () => {
    const ctx = get().replyContext;
    if (ctx === null || !get().bilingual) return;
    set({ backTranslating: true, error: null });
    try {
      const back = await tauri.aiTranslateText(get().bodyForeign, 'zh-CN');
      if (get().replyContext?.messageId === ctx.messageId) set({ bodyZhBack: back.text });
    } catch (e) {
      if (get().replyContext?.messageId === ctx.messageId) set({ error: errMsg(e) });
    } finally {
      if (get().replyContext?.messageId === ctx.messageId) set({ backTranslating: false });
    }
  },

  runSend: async () => {
    const from = get().fromAccountId;
    if (from === null) return;
    if (!window.confirm('确认发送？此操作不可撤销，并会写入 send_log 审计表。')) return;
    // #13 发送前记录当前草稿会话身份（replyContext.messageId 或 null 表示空白草稿）；
    // linger 定时器触发时比对身份，仅当用户尚未开启新草稿会话时才 reset+关抽屉。
    const sentReplyContext = get().replyContext;
    set({ sending: true, error: null });
    try {
      const receipt = await tauri.smtpSend({
        accountId: from,
        to: parseAddrs(get().to),
        cc: parseAddrs(get().cc),
        subject: get().subject.trim(),
        body: get().bodyForeign,
        inReplyTo: get().replyContext?.messageId ?? null,
        aiAssisted: get().aiAssisted,
      });
      set({
        receiptInfo: `已发送，send_log ${receipt.sendLog.id.slice(0, 8)} · ${receipt.sendLog.smtpResponse ?? ''}`,
      });
      setTimeout(() => {
        // 仅当当前仍是本次发送对应的草稿会话时才清理；
        // 若用户已开新草稿（replyContext 已变），不动它、不强制关抽屉。
        const cur = get().replyContext;
        const isSameSession =
          cur?.messageId === sentReplyContext?.messageId &&
          cur?.accountId === sentReplyContext?.accountId;
        if (isSameSession) {
          get().reset();
          useUiStore.getState().closeDrawer();
        }
      }, SEND_SUCCESS_LINGER_MS);
    } catch (e) {
      set({ error: errMsg(e) });
    } finally {
      set({ sending: false });
    }
  },

  reset: () => {
    set({ ...BLANK });
  },
}));
