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
