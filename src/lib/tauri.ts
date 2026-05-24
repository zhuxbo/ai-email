// The ONLY place we touch `invoke`. Every Tauri command gets a typed wrapper here so
// components depend on stable TS signatures, not loose argument bags. Rust command names are
// snake_case and Tauri auto-converts to camelCase on the wire.

import { invoke } from '@tauri-apps/api/core';

import type {
  Account,
  AddAccountForm,
  Mailbox,
  MessageBody,
  MessageHeader,
  SyncReport,
} from './types';

export async function accountsList(): Promise<Account[]> {
  return invoke('accounts_list');
}

export async function accountAdd(form: AddAccountForm): Promise<Account> {
  return invoke('account_add', { form });
}

export async function accountRemove(id: string): Promise<void> {
  await invoke('account_remove', { id });
}

export async function inboxSync(accountId: string): Promise<SyncReport> {
  return invoke('inbox_sync', { accountId });
}

export async function mailboxesList(accountId: string): Promise<Mailbox[]> {
  return invoke('mailboxes_list', { accountId });
}

export async function messagesList(
  mailboxId: string,
  limit = 50,
  offset = 0,
): Promise<MessageHeader[]> {
  return invoke('messages_list', { mailboxId, limit, offset });
}

export async function messageGet(id: string): Promise<MessageHeader> {
  return invoke('message_get', { id });
}

export async function messageBody(id: string): Promise<MessageBody> {
  return invoke('message_body', { id });
}
