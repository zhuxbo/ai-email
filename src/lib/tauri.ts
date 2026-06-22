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
  ClassifyResult,
  DraftResult,
  Mailbox,
  MessageBody,
  MessageHeader,
  RoleDefault,
  SendDraft,
  SendReceipt,
  SummaryResult,
  SyncReport,
  TextTranslation,
  TranslateResult,
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

export async function aiClassify(ids: string[]): Promise<ClassifyResult[]> {
  return invoke('ai_classify', { ids });
}

export async function aiTranslate(id: string, target: string): Promise<TranslateResult> {
  return invoke('ai_translate', { id, target });
}

export async function aiTranslateText(text: string, target: string): Promise<TextTranslation> {
  return invoke('ai_translate_text', { text, target });
}

export async function aiDraftReply(id: string, intent: string | null): Promise<DraftResult> {
  return invoke('ai_draft_reply', { id, intent });
}

export async function smtpSend(draft: SendDraft): Promise<SendReceipt> {
  return invoke('smtp_send', { draft });
}

export async function messageSetSeen(id: string, seen: boolean): Promise<void> {
  await invoke('message_set_seen', { id, seen });
}

export async function messageSetFlagged(id: string, flagged: boolean): Promise<void> {
  await invoke('message_set_flagged', { id, flagged });
}

export async function messageDelete(id: string): Promise<void> {
  await invoke('message_delete', { id });
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

export function mergeBySentAt(lists: MessageHeader[][]): MessageHeader[] {
  return lists.flat().sort((x, y) => {
    if (x.sentAt === null && y.sentAt === null) return 0;
    if (x.sentAt === null) return 1;
    if (y.sentAt === null) return -1;
    return y.sentAt.localeCompare(x.sentAt);
  });
}

export interface UnifiedInboxResult {
  messages: MessageHeader[];
  /** 加载失败的账户：accountId → 错误信息。allSettled 下单账户失败不丢，集中上报供 UI 提示。 */
  errors: Record<string, string>;
}

/** 前端聚合。accountId 省略/null=全部。P2 不分页（每账户前 50）。单账户失败只标记该账户、不整体 reject。 */
export async function unifiedInbox(opts: {
  accountId?: string | null;
}): Promise<UnifiedInboxResult> {
  const accounts = await accountsList();
  const targets =
    opts.accountId == null ? accounts : accounts.filter((a) => a.id === opts.accountId);
  const settled = await Promise.allSettled(
    targets.map(async (acc) => {
      const inbox = (await mailboxesList(acc.id)).find((m) => m.name.toUpperCase() === 'INBOX');
      return inbox ? messagesList(inbox.id, 50, 0) : [];
    }),
  );
  const lists: MessageHeader[][] = [];
  const errors: Record<string, string> = {};
  settled.forEach((r, i) => {
    const acc = targets[i];
    if (r.status === 'fulfilled') {
      lists.push(r.value);
    } else if (acc) {
      errors[acc.id] = r.reason instanceof Error ? r.reason.message : String(r.reason);
    }
  });
  return { messages: mergeBySentAt(lists), errors };
}
