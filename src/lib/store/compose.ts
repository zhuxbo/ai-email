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
  draftingFor: string | null;
  error: string | null;
  receiptInfo: string | null;

  openReply: (m: MessageHeader) => void;
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
    set({ drafting: true, draftingFor: ctx.messageId, error: null });
    try {
      const draft = await tauri.aiDraftReply(ctx.messageId, get().intentZh.trim() || null);
      if (get().draftingFor !== ctx.messageId) return;
      if (get().subject.trim() === '') {
        set({ bodyForeign: draft.body, aiAssisted: true, subject: draft.subject });
      } else {
        set({ bodyForeign: draft.body, aiAssisted: true });
      }
      if (get().bilingual && detectForeign(draft.body)) {
        const back = await tauri.aiTranslateText(draft.body, 'zh-CN');
        if (get().draftingFor === ctx.messageId) set({ bodyZhBack: back.text });
      } else {
        set({ bodyZhBack: null });
      }
    } catch (e) {
      if (get().draftingFor === ctx.messageId) set({ error: errMsg(e) });
    } finally {
      if (get().draftingFor === ctx.messageId) set({ drafting: false });
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
        get().reset();
        useUiStore.getState().closeDrawer();
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
