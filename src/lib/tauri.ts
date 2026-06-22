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
  AutoReplyRule,
  AutoReplyRuleInput,
  ClassifyResult,
  DraftResult,
  Mailbox,
  MessageBody,
  MessageHeader,
  RoleDefault,
  SendDraft,
  SendReceipt,
  SuggestedReply,
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

export async function aiDraftReply(
  id: string,
  intent: string | null,
  force = false,
): Promise<DraftResult> {
  return invoke('ai_draft_reply', { id, intent, force });
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

export async function autoReplyRulesList(accountId: string): Promise<AutoReplyRule[]> {
  return invoke('auto_reply_rules_list', { accountId });
}

export async function autoReplyRuleAdd(input: AutoReplyRuleInput): Promise<AutoReplyRule> {
  return invoke('auto_reply_rule_add', { input });
}

export async function autoReplyRuleUpdate(rule: AutoReplyRule): Promise<void> {
  await invoke('auto_reply_rule_update', { rule });
}

export async function autoReplyRuleRemove(id: string): Promise<void> {
  await invoke('auto_reply_rule_remove', { id });
}

export async function autoReplyRuleSetEnabled(id: string, enabled: boolean): Promise<void> {
  await invoke('auto_reply_rule_set_enabled', { id, enabled });
}

export async function suggestedRepliesList(): Promise<SuggestedReply[]> {
  return invoke('suggested_replies_list');
}

export async function suggestedReplyDismiss(id: string): Promise<void> {
  await invoke('suggested_reply_dismiss', { id });
}

/**
 * 合并多账户邮件列表，并执行：
 *
 * 1. 按 rfcMessageId 去重（#55）——同一封邮件被多账户同时收到时只保留一条。
 *    去重策略：保留 internalDate 最早的那条（最先到达本地服务器）；若均无
 *    internalDate 则保留先出现的那条。rfcMessageId 为 null 的条目不参与去重，全部保留。
 *
 * 2. 按 internalDate 降序排序（#56）——用 IMAP 服务器实际收信时间而非发件人 Date 头，
 *    确保跨账户的并列邮件按真实到达顺序排列。internalDate 为 null 的条目排在末尾。
 */
export function mergeBySentAt(lists: MessageHeader[][]): MessageHeader[] {
  const flat = lists.flat();

  // #55: 按 rfcMessageId 去重，保留 internalDate 最早的副本
  const seen = new Map<string, MessageHeader>();
  const deduped: MessageHeader[] = [];
  for (const msg of flat) {
    if (msg.rfcMessageId === null) {
      deduped.push(msg);
      continue;
    }
    const existing = seen.get(msg.rfcMessageId);
    if (existing === undefined) {
      seen.set(msg.rfcMessageId, msg);
      deduped.push(msg);
    } else {
      // 保留 internalDate 更早的那条（更可靠的本地收信时间）
      const existingDate = existing.internalDate;
      const msgDate = msg.internalDate;
      if (msgDate !== null && (existingDate === null || msgDate < existingDate)) {
        // msg 更早：替换 deduped 中的旧条目
        const idx = deduped.indexOf(existing);
        if (idx !== -1) deduped[idx] = msg;
        seen.set(msg.rfcMessageId, msg);
      }
      // 否则保留已有的，跳过 msg
    }
  }

  // #56: 按 internalDate 降序排序，null 排末尾
  return deduped.sort((x, y) => {
    if (x.internalDate === null && y.internalDate === null) return 0;
    if (x.internalDate === null) return 1;
    if (y.internalDate === null) return -1;
    return y.internalDate.localeCompare(x.internalDate);
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
