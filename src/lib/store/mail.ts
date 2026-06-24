// Mail store: accounts, aggregated message list, selected message + body, transient sync
// state. AI state (summary / translation / models / role defaults) lives in the ai store;
// this store only owns mail data + the front-end aggregation filter.

import { create } from 'zustand';

import * as tauri from '../tauri';
import type {
  Account,
  AddAccountForm,
  Category,
  Mailbox,
  MessageBody,
  MessageHeader,
} from '../types';
import { errMsg } from '../utils';
import { useAiStore } from './ai';
import { useComposeStore } from './compose';

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

  /** 当前账户下已加载的信箱列表；切换账户时重新拉取。 */
  mailboxes: Mailbox[];
  /**
   * 当前选中的信箱 id（仅当 selectedAccountId 非 null 时有意义）。
   * null = 统一收件箱视图（跨账户聚合 INBOX）。
   * 切换到单账户时默认设为该账户 INBOX 的 id；再切走/切回聚合恢复 null。
   */
  selectedMailboxId: string | null;

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
  /** 切换到同一账户下的某个信箱（只在 selectedAccountId 非 null 时调用）。 */
  selectMailbox: (mailboxId: string) => Promise<void>;
  setQuery: (q: string) => void;
  classifyVisibleMessages: () => Promise<void>;
  toggleCategoryFilter: (cat: Category) => void;
  setSortByPriority: (on: boolean) => void;

  setSeen: (id: string, seen: boolean) => Promise<void>;
  /** 自动已读：打开邮件时本地乐观标 \Seen，IMAP 尽力同步、失败静默不回滚（区别于 setSeen）。 */
  markSeenSilent: (id: string) => Promise<void>;
  setFlagged: (id: string, flagged: boolean) => Promise<void>;
  deleteMessage: (id: string) => Promise<void>;

  /**
   * 当前视图是否受 mail://classified 事件影响。
   * classified 只更新 INBOX 邮件的 category/priority，因此仅当视图在看 INBOX 时才需要重拉：
   * - selectedMailboxId 为 null → 聚合 INBOX（跨账户），受影响。
   * - selectedMailboxId 非 null 且对应信箱 specialUse 为 'inbox' → 受影响。
   * - 其它信箱（sent/drafts/trash/junk/普通文件夹）→ 不受影响，跳过重拉。
   */
  classifiedAffectsCurrentView: () => boolean;

  clearError: () => void;
}

export const useMailStore = create<MailState>((set, get) => ({
  accounts: [],
  selectedAccountId: null,

  mailboxes: [],
  selectedMailboxId: null,

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
    set({ syncing: false });
    await get().reloadMessages();
    // 同步阶段失败叠加在加载失败之上 —— 两类失败汇入同一个 accountErrors 通道。
    if (Object.keys(syncErrs).length > 0) {
      set((s) => ({ accountErrors: { ...s.accountErrors, ...syncErrs } }));
    }
    // 后台 classify 写回 category/priority 后会 emit mail://classified，
    // App.tsx 订阅该事件后刷新列表，不再需要固定延迟计时器。
  },

  selectMessage: async (id) => {
    // Bump messageOpenSeq so the mobile shell enters detail even on re-select.
    // AI summary/translation live in the ai store and reset per message id below.
    // #16 compose 草稿仅在切换到**不同**邮件时才 reset，避免点击已选中行时清空正在编辑的草稿。
    const prevId = get().selectedMessageId;
    set({
      selectedMessageId: id,
      body: null,
      loadingBody: true,
      messageOpenSeq: get().messageOpenSeq + 1,
    });
    useAiStore.getState().resetForMessage(id);
    if (prevId !== id) {
      useComposeStore.getState().reset();
    }
    // 打开即标已读：不依赖正文取成功（正文走 IMAP 常失败，旧逻辑把已读绑在正文成功后导致漏标）。
    // 静默——本地乐观标 + IMAP 尽力，失败不回滚不报错（用户已查看；下次 sync 按服务端纠正）。
    const m = get().messages.find((x) => x.id === id);
    if (m !== undefined && !m.flags.includes('\\Seen')) {
      void get().markSeenSilent(id);
    }

    try {
      const body = await tauri.messageBody(id);
      if (get().selectedMessageId === id) {
        set({ body });
      }
    } catch (e) {
      set({ error: errMsg(e) });
    } finally {
      set({ loadingBody: false });
    }
  },

  reloadMessages: async () => {
    const { selectedAccountId: filter, selectedMailboxId: mailboxId } = get();

    // 单账户且已选中特定信箱（非 INBOX 聚合）：直接按 mailboxId 取消息。
    if (filter !== null && mailboxId !== null) {
      try {
        const messages = await tauri.messagesList(mailboxId, 50, 0);
        // 守卫：迟到 reload 不覆盖已切换的筛选。
        if (get().selectedAccountId === filter && get().selectedMailboxId === mailboxId) {
          set({ messages, accountErrors: {} });
        }
      } catch (e) {
        set({ error: errMsg(e) });
      }
      return;
    }

    // 默认：跨账户聚合 INBOX（selectedAccountId=null 或 selectedMailboxId=null 时）。
    try {
      const { messages, errors } = await tauri.unifiedInbox({ accountId: filter });
      // filter 守卫：迟到 reload 不覆盖已切换的筛选。
      if (get().selectedAccountId === filter && get().selectedMailboxId === mailboxId) {
        set({ messages, accountErrors: errors });
      }
    } catch (e) {
      // 整体聚合失败（如 accountsList 抛错）：清掉过时的 per-account 错误，由全局 error 接管。
      set({ error: errMsg(e), accountErrors: {} });
    }
  },

  setFilter: async (accountId: string | null) => {
    if (accountId === null) {
      // 切回聚合视图：清信箱列表和选中信箱，回统一 INBOX。
      set({
        selectedAccountId: null,
        selectedMailboxId: null,
        mailboxes: [],
        selectedMessageId: null,
        body: null,
      });
      useAiStore.getState().resetForMessage('');
      await get().reloadMessages();
      return;
    }

    // 切换到单账户：拉取该账户信箱列表，默认选中 INBOX。
    set({ selectedAccountId: accountId, selectedMessageId: null, body: null });
    useAiStore.getState().resetForMessage('');

    try {
      const boxes = await tauri.mailboxesList(accountId);
      // 守卫：防止拉取期间用户已再次切换。
      if (get().selectedAccountId !== accountId) return;

      const inbox = boxes.find((m) => m.specialUse === 'inbox' || m.name.toUpperCase() === 'INBOX');
      set({
        mailboxes: boxes,
        // 找到 INBOX 就选中它并走 mailboxId 路径；找不到（空账户）则回聚合路径。
        selectedMailboxId: inbox?.id ?? null,
      });
    } catch (e) {
      set({ error: errMsg(e) });
    }

    await get().reloadMessages();
  },

  selectMailbox: async (mailboxId: string) => {
    const { selectedAccountId, mailboxes } = get();
    if (selectedAccountId === null) return;

    set({ selectedMailboxId: mailboxId, selectedMessageId: null, body: null });
    useAiStore.getState().resetForMessage('');

    // 触发按需同步（非 INBOX 信箱首次访问时拉取新邮件），不阻塞 UI。
    const box = mailboxes.find((m) => m.id === mailboxId);
    if (box !== undefined && box.specialUse !== 'inbox' && box.name.toUpperCase() !== 'INBOX') {
      void tauri
        .mailboxSync(selectedAccountId, box.name)
        .then(() => get().reloadMessages())
        .catch((e: unknown) => {
          set({ error: errMsg(e) });
        });
    }

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

  markSeenSilent: async (id) => {
    // 自动已读：本地乐观标 \Seen（不清 error，避免干扰其它操作的错误显示）。
    set({
      messages: get().messages.map((m) =>
        m.id === id ? { ...m, flags: toggleFlag(m.flags, '\\Seen', true) } : m,
      ),
    });
    // IMAP 尽力同步；失败保持本地已读、不回滚不报错（用户已查看；下次 sync 按服务端纠正）。
    // 有意吞掉 rejection：无数据丢失（邮件与本地状态都在），区别于手动 setSeen 的回滚语义。
    await tauri.messageSetSeen(id, true).catch(() => undefined);
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

  classifiedAffectsCurrentView: () => {
    const { selectedMailboxId, mailboxes } = get();
    // selectedMailboxId 为 null → 聚合 INBOX 或单账户默认 INBOX，受影响。
    if (selectedMailboxId === null) return true;
    // 选中具体信箱：按 specialUse 判断是否为 inbox。
    const box = mailboxes.find((m) => m.id === selectedMailboxId);
    return box?.specialUse === 'inbox';
  },

  clearError: () => {
    set({ error: null });
  },
}));
