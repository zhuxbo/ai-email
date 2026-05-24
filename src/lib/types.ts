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
  bodyFetchedAt: string | null;
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
