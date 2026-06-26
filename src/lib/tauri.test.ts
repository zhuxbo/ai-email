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
    imapUid?: number;
  },
): MessageHeader {
  return {
    id,
    accountId: opts?.accountId ?? 'a',
    mailboxId: 'm',
    imapUid: opts?.imapUid ?? 1,
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
    referencesHeader: null,
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

  it('#56 internalDate 相同时按 imapUid 降序作稳定次级键', () => {
    const same = '2026-06-01T10:00:00Z';
    // a1: imapUid=10, b1: imapUid=20 → b1 应排在前（imapUid 降序）
    const result1 = mergeBySentAt([
      [mh('a1', same, { internalDate: same, accountId: 'a', imapUid: 10 })],
      [mh('b1', same, { internalDate: same, accountId: 'b', imapUid: 20 })],
    ]).map((m) => m.id);
    const result2 = mergeBySentAt([
      [mh('b1', same, { internalDate: same, accountId: 'b', imapUid: 20 })],
      [mh('a1', same, { internalDate: same, accountId: 'a', imapUid: 10 })],
    ]).map((m) => m.id);
    // 两种输入顺序均产出相同确定结果（imapUid 大的排前）
    expect(result1).toEqual(['b1', 'a1']);
    expect(result2).toEqual(['b1', 'a1']);
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

  it('#55 rfcMessageId 为空串（畸形 Message-ID: <>）的消息不参与去重（均保留）', () => {
    // mail-parser 对畸形 Message-ID: <> 产出 Some("")，不应把两封不同邮件合并
    const m1 = mh('a1', '2026-06-01T10:00:00Z', {
      internalDate: '2026-06-01T10:00:00Z',
      rfcMessageId: '',
    });
    const m2 = mh('b1', '2026-06-01T11:00:00Z', {
      internalDate: '2026-06-01T11:00:00Z',
      rfcMessageId: '',
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

  // #56 回归：internalDate 恒 null（Sprint 1.4 前现状），应回落到 sentAt 正确排序
  // 修复前此用例会失败——排序键全 null → comparator 永远 0 → 退化为插入顺序
  it('#56 所有 internalDate=null 时回落 sentAt 正确时间排序', () => {
    const result = mergeBySentAt([
      [
        mh('a1', '2026-06-01T08:00:00Z', { internalDate: null, accountId: 'a', imapUid: 1 }),
        mh('a2', '2026-06-03T12:00:00Z', { internalDate: null, accountId: 'a', imapUid: 2 }),
      ],
      [
        mh('b1', '2026-06-02T10:00:00Z', { internalDate: null, accountId: 'b', imapUid: 3 }),
        mh('b2', null, { internalDate: null, accountId: 'b', imapUid: 4 }),
      ],
    ]).map((m) => m.id);
    // 按 sentAt 降序：a2(2026-06-03) > b1(2026-06-02) > a1(2026-06-01) > b2(null 末尾)
    expect(result).toEqual(['a2', 'b1', 'a1', 'b2']);
  });

  // #55 + #56：去重保留策略也用 internalDate ?? sentAt 复合键
  it('#55 internalDate 全 null 时去重按 sentAt 保留最早副本', () => {
    const dup = '<dup@example.com>';
    // 两条 internalDate 均为 null，按 sentAt 比较：earlier 应被保留
    const earlier = mh('a1', '2026-06-01T09:00:00Z', {
      internalDate: null,
      rfcMessageId: dup,
      accountId: 'a',
    });
    const later = mh('b1', '2026-06-01T11:00:00Z', {
      internalDate: null,
      rfcMessageId: dup,
      accountId: 'b',
    });
    const result = mergeBySentAt([[later], [earlier]]);
    expect(result.filter((m) => m.rfcMessageId === dup)).toHaveLength(1);
    expect(result.find((m) => m.rfcMessageId === dup)?.id).toBe('a1');
  });
});
