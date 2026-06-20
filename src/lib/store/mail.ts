// One store for everything Sprint 1 needs: accounts, mailboxes, message list, selected
// message + body, transient sync state. Split into multiple stores once the surface grows
// (e.g. when AI panel data lands in Sprint 2+).

import { create } from 'zustand';

import * as tauri from '../tauri';
import type {
  Account,
  AddAccountForm,
  AddModelForm,
  AiModel,
  AiRole,
  Category,
  Mailbox,
  MessageBody,
  MessageHeader,
  RoleDefault,
  SummaryResult,
  TranslateResult,
} from '../types';
import { useAiStore } from './ai';

interface MailState {
  accounts: Account[];
  selectedAccountId: string | null;

  mailboxes: Mailbox[];
  selectedMailboxId: string | null;

  messages: MessageHeader[];
  selectedMessageId: string | null;
  messageOpenSeq: number;
  body: MessageBody | null;
  summary: SummaryResult | null;
  translation: TranslateResult | null;

  models: AiModel[];
  roleDefaults: RoleDefault[];

  /** Filter set — empty array means "show all". */
  categoryFilter: Category[];
  sortByPriority: boolean;

  composerOpen: boolean;

  query: string;
  syncErrors: Record<string, string>;

  syncing: boolean;
  loadingBody: boolean;
  summarizing: boolean;
  translating: boolean;
  error: string | null;

  loadAccounts: () => Promise<void>;
  addAccount: (form: AddAccountForm) => Promise<Account>;
  removeAccount: (id: string) => Promise<void>;

  selectAccount: (id: string) => Promise<void>;
  syncInbox: (accountId?: string) => Promise<void>;

  selectMailbox: (id: string) => Promise<void>;
  selectMessage: (id: string) => Promise<void>;
  summarizeSelectedMessage: () => Promise<void>;
  translateSelectedMessage: (target: string) => Promise<void>;
  clearTranslation: () => void;

  openComposer: () => void;
  closeComposer: () => void;

  loadAiConfig: () => Promise<void>;
  addModel: (form: AddModelForm) => Promise<AiModel>;
  removeModel: (id: string) => Promise<void>;
  setRoleDefault: (role: AiRole, modelId: string) => Promise<void>;
  clearRoleDefault: (role: AiRole) => Promise<void>;

  reloadMessages: () => Promise<void>;
  setFilter: (accountId: string | null) => Promise<void>;
  setQuery: (q: string) => void;
  classifyVisibleMessages: () => Promise<void>;
  toggleCategoryFilter: (cat: Category) => void;
  setSortByPriority: (on: boolean) => void;
  classifySelectedMailbox: () => Promise<void>;

  clearError: () => void;
}

function pickInbox(mailboxes: Mailbox[]): Mailbox | null {
  // INBOX is case-sensitive per RFC 3501 but some providers (rare) return lower-case.
  return mailboxes.find((m) => m.name.toUpperCase() === 'INBOX') ?? null;
}

function errMsg(e: unknown): string {
  if (typeof e === 'string') {
    return e;
  }
  if (e instanceof Error) {
    return e.message;
  }
  return JSON.stringify(e);
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
  summary: null,
  translation: null,

  models: [],
  roleDefaults: [],

  categoryFilter: [],
  sortByPriority: false,

  composerOpen: false,

  query: '',
  syncErrors: {},

  syncing: false,
  loadingBody: false,
  summarizing: false,
  translating: false,
  error: null,

  loadAccounts: async () => {
    try {
      const accounts = await tauri.accountsList();
      set({ accounts, error: null });
      const first = accounts[0];
      if (first && get().selectedAccountId === null) {
        await get().selectAccount(first.id);
      }
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  addAccount: async (form) => {
    set({ error: null });
    try {
      const account = await tauri.accountAdd(form);
      set((s) => ({ accounts: [...s.accounts, account] }));
      await get().selectAccount(account.id);
      // First sync runs in the background — surface any error via the store but don't
      // block the dialog from closing on the user.
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
      set((s) => {
        const accounts = s.accounts.filter((a) => a.id !== id);
        const stillSelected = s.selectedAccountId === id ? null : s.selectedAccountId;
        return {
          accounts,
          selectedAccountId: stillSelected,
          mailboxes: stillSelected ? s.mailboxes : [],
          messages: stillSelected ? s.messages : [],
          selectedMailboxId: stillSelected ? s.selectedMailboxId : null,
          selectedMessageId: stillSelected ? s.selectedMessageId : null,
          body: stillSelected ? s.body : null,
        };
      });
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  selectAccount: async (id) => {
    set({
      selectedAccountId: id,
      mailboxes: [],
      messages: [],
      selectedMailboxId: null,
      selectedMessageId: null,
      body: null,
    });
    try {
      const mailboxes = await tauri.mailboxesList(id);
      set({ mailboxes });
      const inbox = pickInbox(mailboxes);
      if (inbox) {
        await get().selectMailbox(inbox.id);
      }
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  syncInbox: async (accountId?: string) => {
    const filter = accountId ?? get().selectedAccountId;
    const targets = filter == null ? get().accounts.map((a) => a.id) : [filter];
    set({ syncing: true, error: null, syncErrors: {} });
    const results = await Promise.allSettled(targets.map((id) => tauri.inboxSync(id)));
    const errors: Record<string, string> = {};
    results.forEach((r, i) => {
      if (r.status === 'rejected') errors[targets[i] ?? ''] = errMsg(r.reason);
    });
    const anyNew = results.some((r) => r.status === 'fulfilled' && r.value.newMessageCount > 0);
    set({ syncing: false, syncErrors: errors });
    await get().reloadMessages();
    if (anyNew)
      setTimeout(() => {
        void get().reloadMessages();
      }, 3500); // 有新邮件才延迟 reload（复用带 filter 守卫的 reloadMessages）
  },

  selectMailbox: async (id) => {
    set({
      selectedMailboxId: id,
      selectedMessageId: null,
      body: null,
    });
    try {
      const messages = await tauri.messagesList(id, 50, 0);
      set({ messages });
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  selectMessage: async (id) => {
    // Clear summary + translation too — each message owns its own. Cache lookup on the
    // backend means a second click on the same message round-trips against ai_results,
    // not the API.
    set({
      selectedMessageId: id,
      body: null,
      summary: null,
      translation: null,
      loadingBody: true,
      messageOpenSeq: get().messageOpenSeq + 1,
    });
    useAiStore.getState().resetForMessage(id);
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

  summarizeSelectedMessage: async () => {
    const id = get().selectedMessageId;
    if (id === null) return;
    set({ summarizing: true, error: null });
    try {
      const summary = await tauri.aiSummarize(id);
      // Drop the result if the user already switched away — late-arriving responses
      // shouldn't repaint the panel for a different message.
      if (get().selectedMessageId === id) {
        set({ summary });
      }
    } catch (e) {
      set({ error: errMsg(e) });
    } finally {
      set({ summarizing: false });
    }
  },

  translateSelectedMessage: async (target) => {
    const id = get().selectedMessageId;
    if (id === null) return;
    set({ translating: true, error: null });
    try {
      const translation = await tauri.aiTranslate(id, target);
      if (get().selectedMessageId === id) {
        set({ translation });
      }
    } catch (e) {
      set({ error: errMsg(e) });
    } finally {
      set({ translating: false });
    }
  },

  clearTranslation: () => {
    set({ translation: null });
  },

  openComposer: () => {
    set({ composerOpen: true });
  },
  closeComposer: () => {
    set({ composerOpen: false });
  },

  loadAiConfig: async () => {
    try {
      const [models, roleDefaults] = await Promise.all([
        tauri.modelsList(),
        tauri.roleDefaultsList(),
      ]);
      set({ models, roleDefaults });
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  addModel: async (form) => {
    try {
      const model = await tauri.modelAdd(form);
      set((s) => ({ models: [...s.models, model] }));
      return model;
    } catch (e) {
      set({ error: errMsg(e) });
      throw e;
    }
  },

  removeModel: async (id) => {
    try {
      await tauri.modelRemove(id);
      set((s) => ({ models: s.models.filter((m) => m.id !== id) }));
    } catch (e) {
      set({ error: errMsg(e) });
      throw e;
    }
  },

  setRoleDefault: async (role, modelId) => {
    try {
      await tauri.roleDefaultSet(role, modelId);
      set((s) => {
        const others = s.roleDefaults.filter((r) => r.role !== role);
        return { roleDefaults: [...others, { role, modelId }] };
      });
    } catch (e) {
      set({ error: errMsg(e) });
      throw e;
    }
  },

  clearRoleDefault: async (role) => {
    try {
      await tauri.roleDefaultClear(role);
      set((s) => ({ roleDefaults: s.roleDefaults.filter((r) => r.role !== role) }));
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  reloadMessages: async () => {
    const filter = get().selectedAccountId;
    try {
      const messages = await tauri.unifiedInbox({ accountId: filter });
      if (get().selectedAccountId === filter) set({ messages }); // filter 守卫，防迟到 reload 覆盖
    } catch (e) {
      set({ error: errMsg(e) });
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

  classifySelectedMailbox: async () => {
    const messages = get().messages;
    const ids = messages.map((m) => m.id);
    if (ids.length === 0) return;
    try {
      await tauri.aiClassify(ids);
      await get().reloadMessages();
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  clearError: () => {
    set({ error: null });
  },
}));
