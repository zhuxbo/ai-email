// The ONLY place we touch `invoke`. Every Tauri command gets a typed wrapper here so
// components depend on stable TS signatures, not loose argument bags. Rust command names are
// snake_case and Tauri auto-converts to camelCase on the wire.

import { invoke } from '@tauri-apps/api/core';

import type {
  Account,
  AddAccountForm,
  AddModelForm,
  AiModel,
  AiRole,
  Mailbox,
  MessageBody,
  MessageHeader,
  RoleDefault,
  SummaryResult,
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

export async function aiSummarize(id: string): Promise<SummaryResult> {
  return invoke('ai_summarize', { id });
}

export async function modelsList(): Promise<AiModel[]> {
  return invoke('models_list');
}

export async function modelAdd(form: AddModelForm): Promise<AiModel> {
  return invoke('model_add', { form });
}

export async function modelRemove(id: string): Promise<void> {
  await invoke('model_remove', { id });
}

export async function roleDefaultsList(): Promise<RoleDefault[]> {
  return invoke('role_defaults_list');
}

export async function roleDefaultSet(role: AiRole, modelId: string): Promise<void> {
  await invoke('role_default_set', { form: { role, modelId } });
}

export async function roleDefaultClear(role: AiRole): Promise<void> {
  await invoke('role_default_clear', { role });
}
