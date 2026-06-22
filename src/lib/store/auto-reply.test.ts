import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('../tauri', () => ({
  suggestedRepliesList: vi.fn(),
  suggestedReplyDismiss: vi.fn(),
  autoReplyRulesList: vi.fn(),
  autoReplyRuleAdd: vi.fn(),
  autoReplyRuleUpdate: vi.fn(),
  autoReplyRuleRemove: vi.fn(),
  autoReplyRuleSetEnabled: vi.fn(),
}));

import * as tauri from '../tauri';
import { useAutoReplyStore } from './auto-reply';
import type { SuggestedReply } from '../types';

const sug = (id: string): SuggestedReply => ({
  id,
  messageId: `m-${id}`,
  accountId: 'acc-1',
  ruleNameSnapshot: 'r',
  intentSnapshot: '意图',
  subject: 's',
  fromAddr: 'x@y.com',
  sentAt: null,
  category: null,
  priority: null,
  createdAt: '2026-06-22T00:00:00Z',
});

beforeEach(() => {
  useAutoReplyStore.setState({ rules: [], rulesAccountId: null, queue: [], error: null });
  vi.clearAllMocks();
});

describe('autoReply store', () => {
  it('loadQueue 填充队列', async () => {
    vi.mocked(tauri.suggestedRepliesList).mockResolvedValue([sug('a'), sug('b')]);
    await useAutoReplyStore.getState().loadQueue();
    expect(useAutoReplyStore.getState().queue).toHaveLength(2);
  });

  it('dismiss 乐观移除；失败回滚 + 写 error', async () => {
    useAutoReplyStore.setState({ queue: [sug('a'), sug('b')], error: '旧错误' });
    let reject!: (e: unknown) => void;
    vi.mocked(tauri.suggestedReplyDismiss).mockReturnValue(
      new Promise<void>((_, r) => {
        reject = r;
      }),
    );
    const p = useAutoReplyStore.getState().dismiss('a');
    expect(useAutoReplyStore.getState().queue.map((s) => s.id)).toEqual(['b']);
    expect(useAutoReplyStore.getState().error).toBeNull();
    reject(new Error('boom'));
    await p;
    expect(useAutoReplyStore.getState().queue.map((s) => s.id)).toEqual(['a', 'b']);
    expect(useAutoReplyStore.getState().error).toContain('boom');
  });

  it('dismiss 成功后不回滚', async () => {
    useAutoReplyStore.setState({ queue: [sug('a')] });
    vi.mocked(tauri.suggestedReplyDismiss).mockResolvedValue();
    await useAutoReplyStore.getState().dismiss('a');
    expect(useAutoReplyStore.getState().queue).toHaveLength(0);
  });
});
