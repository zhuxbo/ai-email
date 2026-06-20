import { describe, it, expect } from 'vitest';
import { mergeBySentAt } from './tauri';
import type { MessageHeader } from './types';

function mh(id: string, sentAt: string | null): MessageHeader {
  return {
    id,
    accountId: 'a',
    mailboxId: 'm',
    imapUid: 1,
    rfcMessageId: null,
    threadId: null,
    subject: id,
    fromAddr: null,
    toAddrs: [],
    ccAddrs: [],
    sentAt,
    internalDate: null,
    flags: [],
    sizeBytes: null,
    hasAttachment: false,
    snippet: null,
    priority: null,
    category: null,
    tags: [],
    bodyFetchedAt: null,
  };
}

describe('mergeBySentAt', () => {
  it('sentAt 降序、null 末尾', () => {
    expect(
      mergeBySentAt([
        [mh('a1', '2026-06-01T10:00:00Z'), mh('a2', null)],
        [mh('b1', '2026-06-02T10:00:00Z')],
      ]).map((m) => m.id),
    ).toEqual(['b1', 'a1', 'a2']);
  });

  it('空输入空输出', () => {
    expect(mergeBySentAt([])).toEqual([]);
  });
});
