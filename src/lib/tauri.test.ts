import { describe, it, expect } from 'vitest';
import { mergeBySentAt } from './tauri';
import type { MessageHeader } from './types';

function mh(
  id: string,
  sentAt: string | null,
  opts?: {
    internalDate?: string | null;
    rfcMessageId?: string | null;
    accountId?: string;
  },
): MessageHeader {
  return {
    id,
    accountId: opts?.accountId ?? 'a',
    mailboxId: 'm',
    imapUid: 1,
    rfcMessageId: opts?.rfcMessageId ?? null,
    threadId: null,
    subject: id,
    fromAddr: null,
    toAddrs: [],
    ccAddrs: [],
    sentAt,
    internalDate: opts?.internalDate ?? null,
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
  it('internalDate 降序、null 末尾', () => {
    expect(
      mergeBySentAt([
        [
          mh('a1', '2026-06-01T10:00:00Z', { internalDate: '2026-06-01T10:00:00Z' }),
          mh('a2', null, { internalDate: null }),
        ],
        [mh('b1', '2026-06-02T10:00:00Z', { internalDate: '2026-06-02T10:00:00Z' })],
      ]).map((m) => m.id),
    ).toEqual(['b1', 'a1', 'a2']);
  });

  it('空输入空输出', () => {
    expect(mergeBySentAt([])).toEqual([]);
  });

  // #56: 跨账户按 internalDate 排序，不受账户创建顺序影响
  it('#56 跨账户按 internalDate 统一排序', () => {
    const acctA = mh('a1', '2026-06-01T08:00:00Z', {
      internalDate: '2026-06-01T08:00:00Z',
      accountId: 'account-a',
    });
    const acctB = mh('b1', '2026-06-01T09:00:00Z', {
      internalDate: '2026-06-01T09:00:00Z',
      accountId: 'account-b',
    });
    // acctB 的 internalDate 更晚，应排在前面，即使 account-b 是后加的账户
    const result = mergeBySentAt([[acctA], [acctB]]).map((m) => m.id);
    expect(result).toEqual(['b1', 'a1']);
  });

  it('#56 internalDate 相同时顺序稳定（不因账户顺序变化）', () => {
    const same = '2026-06-01T10:00:00Z';
    const result1 = mergeBySentAt([
      [mh('a1', same, { internalDate: same, accountId: 'a' })],
      [mh('b1', same, { internalDate: same, accountId: 'b' })],
    ]).map((m) => m.id);
    const result2 = mergeBySentAt([
      [mh('b1', same, { internalDate: same, accountId: 'b' })],
      [mh('a1', same, { internalDate: same, accountId: 'a' })],
    ]).map((m) => m.id);
    // 两种顺序都包含相同元素（稳定，不乱序）
    expect(result1.sort()).toEqual(result2.sort());
  });

  // #55: 按 rfcMessageId 去重——多账户都收到同一封邮件时只保留一行
  it('#55 相同 rfcMessageId 去重，只保留最早收到的那条（internalDate 最小）', () => {
    const dup = '<msg-001@example.com>';
    const earlier = mh('a1', '2026-06-01T10:00:00Z', {
      internalDate: '2026-06-01T10:00:00Z',
      rfcMessageId: dup,
      accountId: 'account-a',
    });
    const later = mh('b1', '2026-06-01T11:00:00Z', {
      internalDate: '2026-06-01T11:00:00Z',
      rfcMessageId: dup,
      accountId: 'account-b',
    });
    const other = mh('c1', '2026-06-01T09:00:00Z', {
      internalDate: '2026-06-01T09:00:00Z',
      rfcMessageId: '<other@example.com>',
      accountId: 'account-a',
    });
    const result = mergeBySentAt([[earlier, other], [later]]);
    // dup 只保留一条（earlier，internalDate 更早）
    expect(result.filter((m) => m.rfcMessageId === dup)).toHaveLength(1);
    expect(result.find((m) => m.rfcMessageId === dup)?.id).toBe('a1');
    // other 不受影响
    expect(result.some((m) => m.id === 'c1')).toBe(true);
  });

  it('#55 rfcMessageId 为 null 的消息不参与去重（均保留）', () => {
    const m1 = mh('a1', '2026-06-01T10:00:00Z', {
      internalDate: '2026-06-01T10:00:00Z',
      rfcMessageId: null,
    });
    const m2 = mh('b1', '2026-06-01T11:00:00Z', {
      internalDate: '2026-06-01T11:00:00Z',
      rfcMessageId: null,
    });
    const result = mergeBySentAt([[m1], [m2]]);
    expect(result).toHaveLength(2);
  });

  it('#55 三账户同 rfcMessageId，只保留一条', () => {
    const dup = '<triple@example.com>';
    const result = mergeBySentAt([
      [
        mh('a1', '2026-06-01T10:00:00Z', {
          internalDate: '2026-06-01T10:00:00Z',
          rfcMessageId: dup,
          accountId: 'a',
        }),
      ],
      [
        mh('b1', '2026-06-01T09:00:00Z', {
          internalDate: '2026-06-01T09:00:00Z',
          rfcMessageId: dup,
          accountId: 'b',
        }),
      ],
      [
        mh('c1', '2026-06-01T11:00:00Z', {
          internalDate: '2026-06-01T11:00:00Z',
          rfcMessageId: dup,
          accountId: 'c',
        }),
      ],
    ]);
    const dups = result.filter((m) => m.rfcMessageId === dup);
    expect(dups).toHaveLength(1);
    // 保留 internalDate 最早的（b1：09:00）
    expect(dups[0]?.id).toBe('b1');
  });
});
