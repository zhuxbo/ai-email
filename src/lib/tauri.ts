// The ONLY place we touch `invoke` and `listen`. Every Tauri command/event gets a typed
// wrapper here so components depend on stable TS signatures, not loose argument bags. Rust
// command names are snake_case and Tauri auto-converts to camelCase on the wire.

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { UnlistenFn } from '@tauri-apps/api/event';

import type {
  Account,
  AttachmentMeta,
  AddAccountForm,
  UpdateAccountForm,
  AddModelForm,
  UpdateModelForm,
  AiModel,
  AiRole,
  AutoReplyRule,
  AutoReplyRuleInput,
  ClassifyResult,
  ConversationView,
  DraftResult,
  FilterRule,
  FilterRuleInput,
  Mailbox,
  MessageBody,
  MessageFilterPreview,
  MessageHeader,
  RoleDefault,
  SendDraft,
  SendReceipt,
  SenderFilter,
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

export async function accountUpdate(id: string, form: UpdateAccountForm): Promise<Account> {
  return invoke('account_update', { id, form });
}

export async function inboxSync(accountId: string): Promise<SyncReport> {
  return invoke('inbox_sync', { accountId });
}

export async function mailboxSync(accountId: string, mailboxName: string): Promise<SyncReport> {
  return invoke('mailbox_sync', { accountId, mailboxName });
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

export async function messagesMarkSeenBulk(ids: string[]): Promise<void> {
  await invoke('messages_mark_seen_bulk', { ids });
}

export async function messageSetFlagged(id: string, flagged: boolean): Promise<void> {
  await invoke('message_set_flagged', { id, flagged });
}

export async function messageDelete(id: string): Promise<void> {
  await invoke('message_delete', { id });
}

export async function messageAttachments(id: string): Promise<AttachmentMeta[]> {
  return invoke('message_attachments', { id });
}

export async function messageAttachmentSave(
  id: string,
  index: number,
  dest: string,
): Promise<void> {
  await invoke('message_attachment_save', { id, index, dest });
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

export async function modelUpdate(id: string, form: UpdateModelForm): Promise<AiModel> {
  return invoke('model_update', { id, form });
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

export async function senderFiltersList(): Promise<SenderFilter[]> {
  return invoke<SenderFilter[]>('sender_filters_list');
}

export async function senderFiltersAdd(
  listType: 'black' | 'white',
  value: string,
  note?: string,
): Promise<SenderFilter> {
  return invoke<SenderFilter>('sender_filters_add', { listType, value, note: note ?? null });
}

export async function senderFiltersRemove(id: string): Promise<void> {
  await invoke('sender_filters_remove', { id });
}

export async function conversationThread(messageId: string): Promise<ConversationView> {
  return invoke('conversation_thread', { messageId });
}

export async function filterRulesList(): Promise<FilterRule[]> {
  return invoke('filter_rules_list');
}

export async function filterRuleAdd(input: FilterRuleInput): Promise<FilterRule> {
  return invoke('filter_rule_add', { input });
}

export async function filterRuleUpdate(rule: FilterRule): Promise<void> {
  await invoke('filter_rule_update', { rule });
}

export async function filterRuleRemove(id: string): Promise<void> {
  await invoke('filter_rule_remove', { id });
}

export async function filterRuleSetEnabled(id: string, enabled: boolean): Promise<void> {
  await invoke('filter_rule_set_enabled', { id, enabled });
}

// ---------------------------------------------------------------------------
// Tauri event subscriptions — backend pushes these after background AI tasks
// finish, or after the DB is ready/failed to initialize.
// ---------------------------------------------------------------------------

/** Payload emitted by `db://ready` when DB connect + migrate succeeds. */
export type DbReadyPayload = Record<string, never>;

/** Payload emitted by `db://error` when DB init fails or times out. */
export interface DbErrorPayload {
  message: string;
}

/** Payload returned by `db_status` command. status: "initializing"|"ready"|"error" */
export interface DbStatusPayload {
  status: 'initializing' | 'ready' | 'error';
  message: string | null;
}

/**
 * 主动查询 DB 初始化状态。不依赖事件系统，pool 未就绪时也可调用。
 * 用于注册 db://ready / db://error 监听后的兜底查询，消除 emit-before-listen 竞态。
 */
export async function getDbStatus(): Promise<DbStatusPayload> {
  return invoke('db_status');
}

/**
 * Subscribe to `db://ready` — fired once when the database has finished
 * connecting and running migrations. Call the returned unlisten fn on cleanup.
 */
export async function onDbReady(cb: (payload: DbReadyPayload) => void): Promise<UnlistenFn> {
  return listen<DbReadyPayload>('db://ready', (event) => {
    cb(event.payload);
  });
}

/**
 * Subscribe to `db://error` — fired when database initialization fails or
 * times out. The payload includes a human-readable message. Call the returned
 * unlisten fn on cleanup.
 */
export async function onDbError(cb: (payload: DbErrorPayload) => void): Promise<UnlistenFn> {
  return listen<DbErrorPayload>('db://error', (event) => {
    cb(event.payload);
  });
}

/** Payload emitted by `mail://classified` when background classify finishes. */
export interface MailClassifiedPayload {
  accountId: string;
  count: number;
}

/** Payload emitted by `autoreply://updated` when evaluate_rules finishes. */
export interface AutoReplyPayload {
  accountId: string;
}

/**
 * Subscribe to `mail://classified` — fired after background classify writes
 * category/priority back to the messages table. Call the returned unlisten fn
 * on cleanup (component unmount / store teardown).
 */
export async function onMailClassified(
  cb: (payload: MailClassifiedPayload) => void,
): Promise<UnlistenFn> {
  return listen<MailClassifiedPayload>('mail://classified', (event) => {
    cb(event.payload);
  });
}

/**
 * Subscribe to `autoreply://updated` — fired after evaluate_rules inserts
 * new suggested replies. Call the returned unlisten fn on cleanup.
 */
export async function onAutoReplyUpdated(
  cb: (payload: AutoReplyPayload) => void,
): Promise<UnlistenFn> {
  return listen<AutoReplyPayload>('autoreply://updated', (event) => {
    cb(event.payload);
  });
}

/**
 * 合并多账户邮件列表，并执行：
 *
 * 1. 按 rfcMessageId 去重（#55）——同一封邮件被多账户同时收到时只保留一条。
 *    去重策略：保留到达键（internalDate ?? sentAt）最早的副本；两者皆 null 时保留先出现者。
 *    rfcMessageId 为 null 或空串的条目不参与去重，全部保留（空串来自畸形 Message-ID: <> 头）。
 *
 *    ⚠️ 取舍（P2）：多账户同收一封时，被丢弃副本的 per-account flags（已读/星标）、
 *    category、tags 不在统一视图合并——统一收件箱只展示一条，其余账户的状态不可见。
 *
 * 2. 按 internalDate ?? sentAt 降序排序（#56）——internalDate 投产前回落到 sentAt 保持旧
 *    行为；internalDate 投产后自动升级为服务器到达时间。两者皆 null 排末尾。
 *    主键并列时以 imapUid 降序为稳定次级键，保证排序结果确定。
 */
export function mergeBySentAt(lists: MessageHeader[][]): MessageHeader[] {
  const flat = lists.flat();

  /** 到达键：internalDate 可用时优先，否则回落 sentAt（Sprint 1.4 前 internalDate 恒 null） */
  const arrivalKey = (m: MessageHeader): string | null => m.internalDate ?? m.sentAt;

  // #55: 按 rfcMessageId 去重，保留到达键最早的副本
  const seen = new Map<string, MessageHeader>();
  const deduped: MessageHeader[] = [];
  for (const msg of flat) {
    if (msg.rfcMessageId === null || msg.rfcMessageId === '') {
      // 空串来自畸形 Message-ID: <> 头，视同无 ID，不参与去重
      deduped.push(msg);
      continue;
    }
    const existing = seen.get(msg.rfcMessageId);
    if (existing === undefined) {
      seen.set(msg.rfcMessageId, msg);
      deduped.push(msg);
    } else {
      // 保留到达键更早的那条
      const existingKey = arrivalKey(existing);
      const msgKey = arrivalKey(msg);
      if (msgKey !== null && (existingKey === null || msgKey < existingKey)) {
        // msg 更早：替换 deduped 中的旧条目
        const idx = deduped.indexOf(existing);
        if (idx !== -1) deduped[idx] = msg;
        seen.set(msg.rfcMessageId, msg);
      }
      // 否则保留已有的，跳过 msg
    }
  }

  // #56: 按到达键降序排序，null 排末尾；并列时以 imapUid 降序作稳定次级键
  return deduped.sort((x, y) => {
    const kx = arrivalKey(x);
    const ky = arrivalKey(y);
    if (kx === null && ky === null) return y.imapUid - x.imapUid;
    if (kx === null) return 1;
    if (ky === null) return -1;
    const cmp = ky.localeCompare(kx);
    return cmp !== 0 ? cmp : y.imapUid - x.imapUid;
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
      // 默认聚合「收到的」邮件：收件箱 + 自定义文件夹；排除 已发送/草稿/垃圾/废纸篓。
      const boxes = (await mailboxesList(acc.id)).filter(
        (m) => m.specialUse === null || m.specialUse === 'inbox',
      );
      const lists = await Promise.all(boxes.map((m) => messagesList(m.id, 50, 0)));
      return lists.flat();
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

export async function messageFilterPreview(messageId: string): Promise<MessageFilterPreview> {
  return invoke('message_filter_preview', { messageId });
}

export async function messageSetFilterDisabled(
  messageId: string,
  disabled: boolean,
): Promise<void> {
  await invoke('message_set_filter_disabled', { messageId, disabled });
}
