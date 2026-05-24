// One store for everything Sprint 1 needs: accounts, mailboxes, message list, selected
// message + body, transient sync state. Split into multiple stores once the surface grows
// (e.g. when AI panel data lands in Sprint 2+).

import { create } from 'zustand';

import * as tauri from '../tauri';
import type {
  Account,
  AddAccountForm,
  Mailbox,
  MessageBody,
  MessageHeader,
  SyncReport,
} from '../types';

interface MailState {
  accounts: Account[];
  selectedAccountId: string | null;

  mailboxes: Mailbox[];
  selectedMailboxId: string | null;

  messages: MessageHeader[];
  selectedMessageId: string | null;
  body: MessageBody | null;

  syncing: boolean;
  loadingBody: boolean;
  error: string | null;

  loadAccounts: () => Promise<void>;
  addAccount: (form: AddAccountForm) => Promise<Account>;
  removeAccount: (id: string) => Promise<void>;

  selectAccount: (id: string) => Promise<void>;
  syncInbox: (accountId: string) => Promise<SyncReport>;

  selectMailbox: (id: string) => Promise<void>;
  selectMessage: (id: string) => Promise<void>;

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
  body: null,

  syncing: false,
  loadingBody: false,
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

  syncInbox: async (accountId) => {
    set({ syncing: true, error: null });
    try {
      const report = await tauri.inboxSync(accountId);
      // Refresh mailbox list (sync may have created INBOX on first run) and message list.
      const mailboxes = await tauri.mailboxesList(accountId);
      set({ mailboxes });
      const inbox = pickInbox(mailboxes);
      if (inbox) {
        await get().selectMailbox(inbox.id);
      }
      return report;
    } catch (e) {
      set({ error: errMsg(e) });
      throw e;
    } finally {
      set({ syncing: false });
    }
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
    set({ selectedMessageId: id, body: null, loadingBody: true });
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

  clearError: () => {
    set({ error: null });
  },
}));
