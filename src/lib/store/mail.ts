// Mail store: accounts, aggregated message list, selected message + body, transient sync
// state. AI state (summary / translation / models / role defaults) lives in the ai store;
// this store only owns mail data + the front-end aggregation filter.

import { create } from 'zustand';

import * as tauri from '../tauri';
import type { Account, AddAccountForm, Category, MessageBody, MessageHeader } from '../types';
import { errMsg } from '../utils';
import { useAiStore } from './ai';

/**
 * 纯函数：在 flags 中添加或移除单个 flag，不改变其它 flag。
 * 用于乐观写与按粒度回滚，保证两处逻辑对称。
 */
function toggleFlag(flags: string[], flag: string, present: boolean): string[] {
  const has = flags.includes(flag);
  if (present && !has) return [...flags, flag];
  if (!present && has) return flags.filter((f) => f !== flag);
  return flags;
}

async function setFlagOptimistic(
  set: (partial: Partial<MailState>) => void,
  get: () => MailState,
  id: string,
  flag: string,
  value: boolean,
  call: (id: string, value: boolean) => Promise<void>,
): Promise<void> {
  // 开始新操作即清除上一次遗留的错误提示；失败时下方 catch 再写入新 error。
  set({
    messages: get().messages.map((m) =>
      m.id === id ? { ...m, flags: toggleFlag(m.flags, flag, value) } : m,
    ),
    error: null,
  });
  try {
    await call(id, value);
  } catch (e) {
    // #12 按 flag 粒度回滚：读取当前最新 flags，仅反转本次操作的那一个 flag，
    // 保留并发期间其它操作已成功写入的其它 flag，不覆盖整条旧快照。
    set({
      messages: get().messages.map((m) =>
        m.id === id ? { ...m, flags: toggleFlag(m.flags, flag, !value) } : m,
      ),
      error: errMsg(e),
    });
  }
}

interface MailState {
  accounts: Account[];
  selectedAccountId: string | null;

  messages: MessageHeader[];
  selectedMessageId: string | null;
  messageOpenSeq: number;
  body: MessageBody | null;

  /** Filter set — empty array means "show all". */
  categoryFilter: Category[];
  sortByPriority: boolean;

  query: string;
  /** 部分账户当前加载/同步失败：accountId → 错误信息。聚合层不再静默吞掉部分失败。 */
  accountErrors: Record<string, string>;

  syncing: boolean;
  loadingBody: boolean;
  error: string | null;

  loadAccounts: () => Promise<void>;
  addAccount: (form: AddAccountForm) => Promise<Account>;
  removeAccount: (id: string) => Promise<void>;

  syncInbox: (accountId?: string) => Promise<void>;
  selectMessage: (id: string) => Promise<void>;

  reloadMessages: () => Promise<void>;
  setFilter: (accountId: string | null) => Promise<void>;
  setQuery: (q: string) => void;
  classifyVisibleMessages: () => Promise<void>;
  toggleCategoryFilter: (cat: Category) => void;
  setSortByPriority: (on: boolean) => void;

  setSeen: (id: string, seen: boolean) => Promise<void>;
  setFlagged: (id: string, flagged: boolean) => Promise<void>;
  deleteMessage: (id: string) => Promise<void>;

  clearError: () => void;
}

export const useMailStore = create<MailState>((set, get) => ({
  accounts: [],
  selectedAccountId: null,

  messages: [],
  selectedMessageId: null,
  messageOpenSeq: 0,
  body: null,

  categoryFilter: [],
  sortByPriority: false,

  query: '',
  accountErrors: {},

  syncing: false,
  loadingBody: false,
  error: null,

  loadAccounts: async () => {
    try {
      const accounts = await tauri.accountsList();
      set({ accounts, error: null });
      // 默认聚合：不钉首账户，selectedAccountId=null → reloadMessages 拉全部账户 INBOX。
      await get().reloadMessages();
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  addAccount: async (form) => {
    set({ error: null });
    try {
      const account = await tauri.accountAdd(form);
      set((s) => ({ accounts: [...s.accounts, account] }));
      // First sync runs in the background — surface any error via the store but don't block
      // the dialog from closing. syncInbox 完成后 reloadMessages 会聚合刷新新账户的邮件。
      void get()
        .syncInbox(account.id)
        .catch((e: unknown) => {
          set({ error: errMsg(e) });
        });
      return account;
    } catch (e) {
      set({ error: errMsg(e) });
      throw e;
    }
  },

  removeAccount: async (id) => {
    try {
      await tauri.accountRemove(id);
      // 清掉选中（被删账户的邮件可能正选中），过滤回退聚合，再重载聚合列表。
      set((s) => ({
        accounts: s.accounts.filter((a) => a.id !== id),
        selectedAccountId: s.selectedAccountId === id ? null : s.selectedAccountId,
        selectedMessageId: null,
        body: null,
      }));
      await get().reloadMessages();
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  syncInbox: async (accountId?: string) => {
    const filter = accountId ?? get().selectedAccountId;
    const targets = filter == null ? get().accounts.map((a) => a.id) : [filter];
    set({ syncing: true, error: null });
    const results = await Promise.allSettled(targets.map((id) => tauri.inboxSync(id)));
    const syncErrs: Record<string, string> = {};
    results.forEach((r, i) => {
      if (r.status === 'rejected') syncErrs[targets[i] ?? ''] = errMsg(r.reason);
    });
    const anyNew = results.some((r) => r.status === 'fulfilled' && r.value.newMessageCount > 0);
    set({ syncing: false });
    await get().reloadMessages();
    // 同步阶段失败叠加在加载失败之上 —— 两类失败汇入同一个 accountErrors 通道。
    if (Object.keys(syncErrs).length > 0) {
      set((s) => ({ accountErrors: { ...s.accountErrors, ...syncErrs } }));
    }
    if (anyNew)
      setTimeout(() => {
        void get().reloadMessages();
      }, 3500); // 有新邮件才延迟 reload（复用带 filter 守卫的 reloadMessages）
  },

  selectMessage: async (id) => {
    // Bump messageOpenSeq so the mobile shell enters detail even on re-select.
    // AI summary/translation live in the ai store and reset per message id below.
    set({
      selectedMessageId: id,
      body: null,
      loadingBody: true,
      messageOpenSeq: get().messageOpenSeq + 1,
    });
    useAiStore.getState().resetForMessage(id);
    try {
      const body = await tauri.messageBody(id);
      if (get().selectedMessageId === id) {
        set({ body });
        // 打开即标记已读（仅未读时发请求，避免每次打开都打 IMAP）。
        const m = get().messages.find((x) => x.id === id);
        if (m !== undefined && !m.flags.includes('\\Seen')) {
          void get().setSeen(id, true);
        }
      }
    } catch (e) {
      set({ error: errMsg(e) });
    } finally {
      set({ loadingBody: false });
    }
  },

  reloadMessages: async () => {
    const filter = get().selectedAccountId;
    try {
      const { messages, errors } = await tauri.unifiedInbox({ accountId: filter });
      // filter 守卫：迟到 reload 不覆盖已切换的筛选。
      if (get().selectedAccountId === filter) set({ messages, accountErrors: errors });
    } catch (e) {
      // 整体聚合失败（如 accountsList 抛错）：清掉过时的 per-account 错误，由全局 error 接管。
      set({ error: errMsg(e), accountErrors: {} });
    }
  },

  setFilter: async (accountId: string | null) => {
    set({ selectedAccountId: accountId, selectedMessageId: null, body: null }); // 不 bump messageOpenSeq
    useAiStore.getState().resetForMessage('');
    await get().reloadMessages();
  },

  setQuery: (q: string) => {
    set({ query: q });
  },

  classifyVisibleMessages: async () => {
    const ids = get().messages.map((m) => m.id);
    if (ids.length === 0) return;
    try {
      await tauri.aiClassify(ids);
      await get().reloadMessages();
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  toggleCategoryFilter: (cat) => {
    set((s) => {
      const exists = s.categoryFilter.includes(cat);
      return {
        categoryFilter: exists
          ? s.categoryFilter.filter((c) => c !== cat)
          : [...s.categoryFilter, cat],
      };
    });
  },

  setSortByPriority: (on) => {
    set({ sortByPriority: on });
  },

  setSeen: async (id, seen) => {
    await setFlagOptimistic(set, get, id, '\\Seen', seen, tauri.messageSetSeen);
  },

  setFlagged: async (id, flagged) => {
    await setFlagOptimistic(set, get, id, '\\Flagged', flagged, tauri.messageSetFlagged);
  },

  deleteMessage: async (id) => {
    const { messages: prev, selectedMessageId } = get();
    const wasSelected = selectedMessageId === id;
    // 开始新操作即清旧错误（与 setFlagOptimistic 对齐）；乐观移除目标行；
    // 不 bump messageOpenSeq（同切筛选约定）。
    const next: Partial<MailState> = {
      messages: prev.filter((m) => m.id !== id),
      error: null,
    };
    if (wasSelected) {
      next.selectedMessageId = null;
      // 清 body 避免详情区残留已删邮件（同 removeAccount 的既有惯例）
      next.body = null;
    }
    set(next);
    try {
      await tauri.messageDelete(id);
    } catch (e) {
      // 删除失败：记错误，reload 从后端重拉恢复（多状态比手动回滚更可靠）
      set({ error: errMsg(e) });
      await get().reloadMessages();
    }
  },

  clearError: () => {
    set({ error: null });
  },
}));
