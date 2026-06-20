import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useAiStore } from './ai';
import type { SummaryResult } from '../types';
vi.mock('../tauri', () => ({
  aiSummarize: vi.fn(),
  aiTranslate: vi.fn(),
  modelsList: vi.fn().mockResolvedValue([]),
  roleDefaultsList: vi.fn().mockResolvedValue([]),
}));
import * as tauri from '../tauri';
const sum = (t: string): SummaryResult => ({
  tldr: t,
  bullets: [],
  language: 'zh',
  source: 'fresh',
  model: 'm',
  inputTokens: 1,
  outputTokens: 1,
  cacheReadTokens: 0,
});

describe('ai store in-flight 守卫', () => {
  beforeEach(() => {
    useAiStore.setState({
      summary: null,
      summarizing: false,
      summarizingFor: null,
      translation: null,
      translating: false,
      translatingFor: null,
    });
    vi.clearAllMocks();
  });
  it('切到 B 后，A 的迟到摘要被丢弃（store 不脏）', async () => {
    let resolveA!: (v: SummaryResult) => void;
    vi.mocked(tauri.aiSummarize).mockReturnValueOnce(
      new Promise((r) => {
        resolveA = r;
      }),
    );
    const p = useAiStore.getState().summarize('A'); // 发起 A，summarizingFor='A'
    useAiStore.getState().resetForMessage('B'); // 切到 B：token 清 null
    resolveA(sum('A 摘要')); // A 现在才返回
    await p;
    expect(useAiStore.getState().summary).toBeNull(); // 判别力：若守卫失效这里会是 'A 摘要'
    expect(useAiStore.getState().summarizing).toBe(false);
  });
  it('resetForMessage 清结果 + loading + in-flight token', () => {
    useAiStore.setState({
      summary: sum('旧'),
      summarizing: true,
      summarizingFor: 'X',
      translating: true,
      translatingFor: 'Y',
    });
    useAiStore.getState().resetForMessage('Z');
    const s = useAiStore.getState();
    expect([s.summary, s.summarizingFor, s.translatingFor]).toEqual([null, null, null]);
    expect([s.summarizing, s.translating]).toEqual([false, false]);
  });
});
