// Domain types — mirror the Rust serde shapes (camelCase, RFC3339 timestamps).
// Update both sides together; mismatches surface as runtime invoke errors.

export interface Account {
  id: string;
  email: string;
  displayName: string | null;
  provider: string;
  imapHost: string;
  imapPort: number;
  smtpHost: string;
  smtpPort: number;
  createdAt: string;
  lastSyncedAt: string | null;
}

export interface Mailbox {
  id: string;
  accountId: string;
  name: string;
  delimiter: string | null;
  uidValidity: number | null;
  uidNext: number | null;
  lastSyncedAt: string | null;
}

export type Category = 'personal' | 'work' | 'notification' | 'promotion' | 'spam';

export interface MessageHeader {
  id: string;
  accountId: string;
  mailboxId: string;
  imapUid: number;
  rfcMessageId: string | null;
  threadId: string | null;
  subject: string | null;
  fromAddr: string | null;
  toAddrs: string[];
  ccAddrs: string[];
  sentAt: string | null;
  internalDate: string | null;
  flags: string[];
  sizeBytes: number | null;
  hasAttachment: boolean;
  snippet: string | null;
  priority: number | null;
  /** AI-assigned: 'personal' | 'work' | 'notification' | 'promotion' | 'spam' | null until classified. */
  category: Category | null;
  /** AI + user tags joined from message_tags. */
  tags: string[];
  bodyFetchedAt: string | null;
}

export interface Classification {
  category: Category;
  priority: number;
  tags: string[];
}

export interface ClassifyResult {
  messageId: string;
  category: Category;
  priority: number;
  tags: string[];
  source: 'fresh' | 'cached';
}

export interface MessageBody {
  messageId: string;
  textPlain: string | null;
  html: string | null;
  fetchedAt: string;
}

export interface SyncReport {
  newMessageCount: number;
  totalInMailbox: number;
}

export type AiProvider = 'anthropic' | 'openai';

export type AiRole = 'summary' | 'classify' | 'translate' | 'draft';

export interface AiModel {
  id: string;
  displayName: string;
  provider: AiProvider;
  modelId: string;
  /** null = use the provider's default base URL. */
  baseUrl: string | null;
  createdAt: string;
}

export interface RoleDefault {
  role: AiRole;
  modelId: string;
}

export interface AddModelForm {
  displayName: string;
  provider: AiProvider;
  modelId: string;
  baseUrl: string | null;
  apiKey: string;
}

export interface DraftResult {
  subject: string;
  body: string;
  tone: string;
  source: 'fresh' | 'cached';
  model: string;
  inputTokens: number | null;
  outputTokens: number | null;
  cacheReadTokens: number | null;
}

export interface SendDraft {
  accountId: string;
  to: string[];
  cc: string[];
  subject: string;
  body: string;
  inReplyTo: string | null;
  aiAssisted: boolean;
}

export interface SendLogRecord {
  id: string;
  accountId: string;
  inReplyTo: string | null;
  toAddrs: string[];
  subject: string;
  aiAssisted: boolean;
  sentAt: string;
  smtpResponse: string | null;
}

export interface SendReceipt {
  sendLog: SendLogRecord;
}

export interface TranslateResult {
  target: string;
  subject: string;
  body: string;
  source: 'fresh' | 'cached';
  model: string;
  inputTokens: number | null;
  outputTokens: number | null;
  cacheReadTokens: number | null;
}

export interface SummaryResult {
  tldr: string;
  bullets: string[];
  language: string;
  /** 'fresh' = new Anthropic API call, 'cached' = served from ai_results without network. */
  source: 'fresh' | 'cached';
  model: string;
  inputTokens: number | null;
  outputTokens: number | null;
  cacheReadTokens: number | null;
}

export interface AddAccountForm {
  email: string;
  displayName: string | null;
  provider: string;
  imapHost: string;
  imapPort: number;
  smtpHost: string;
  smtpPort: number;
  authCode: string;
}

export interface TextTranslation {
  text: string;
}

export interface AutoReplyRule {
  id: string;
  accountId: string;
  name: string;
  enabled: boolean;
  /** 发件地址子串匹配；null=不限。 */
  matchDomain: string | null;
  matchCategory: Category | null;
  /** 命中 message.priority <= 该值（1=最重要，3=最次）；null=不限。 */
  matchPriorityCeiling: number | null;
  draftIntent: string;
  createdAt: string;
}

export interface AutoReplyRuleInput {
  accountId: string;
  name: string;
  enabled: boolean;
  matchDomain: string | null;
  matchCategory: Category | null;
  matchPriorityCeiling: number | null;
  draftIntent: string;
}

export interface SuggestedReply {
  id: string;
  messageId: string;
  accountId: string;
  ruleNameSnapshot: string;
  intentSnapshot: string;
  subject: string | null;
  fromAddr: string | null;
  snippet: string | null;
  sentAt: string | null;
  category: Category | null;
  priority: number | null;
  createdAt: string;
}
