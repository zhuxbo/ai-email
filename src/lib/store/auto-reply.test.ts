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
  snippet: null,
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

  it('dismiss 乐观移除；失败从后端重拉权威队列 + 写 error', async () => {
    useAutoReplyStore.setState({ queue: [sug('a'), sug('b')], error: '旧错误' });
    let reject!: (e: unknown) => void;
    vi.mocked(tauri.suggestedReplyDismiss).mockReturnValue(
      new Promise<void>((_, r) => {
        reject = r;
      }),
    );
    // 失败时不还原陈旧快照，而是 loadQueue 从后端重拉（此处后端仍含 a、b）。
    vi.mocked(tauri.suggestedRepliesList).mockResolvedValue([sug('a'), sug('b')]);
    const p = useAutoReplyStore.getState().dismiss('a');
    expect(useAutoReplyStore.getState().queue.map((s) => s.id)).toEqual(['b']);
    expect(useAutoReplyStore.getState().error).toBeNull();
    reject(new Error('boom'));
    await p;
    expect(useAutoReplyStore.getState().queue.map((s) => s.id)).toEqual(['a', 'b']);
    expect(useAutoReplyStore.getState().error).toContain('boom');
  });

  it('并发 dismiss：一项失败重拉后端，不复活另一项已成功移除项', async () => {
    useAutoReplyStore.setState({ queue: [sug('a'), sug('b'), sug('c')], error: null });
    // a 失败、b 成功；失败的 a 走 loadQueue 重拉后端权威队列（后端已删 b → 只剩 a、c）。
    vi.mocked(tauri.suggestedReplyDismiss).mockImplementation((id: string) =>
      id === 'a' ? Promise.reject(new Error('boom')) : Promise.resolve(),
    );
    vi.mocked(tauri.suggestedRepliesList).mockResolvedValue([sug('a'), sug('c')]);
    await Promise.all([
      useAutoReplyStore.getState().dismiss('a'),
      useAutoReplyStore.getState().dismiss('b'),
    ]);
    // a 的失败回滚绝不能用陈旧快照复活已移除的 b；最终 = 后端权威 [a, c]。
    const ids = useAutoReplyStore.getState().queue.map((s) => s.id);
    expect(ids).not.toContain('b');
    expect(ids).toEqual(['a', 'c']);
  });

  it('dismiss 成功后不回滚', async () => {
    useAutoReplyStore.setState({ queue: [sug('a')] });
    vi.mocked(tauri.suggestedReplyDismiss).mockResolvedValue();
    await useAutoReplyStore.getState().dismiss('a');
    expect(useAutoReplyStore.getState().queue).toHaveLength(0);
  });
});
