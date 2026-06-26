import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('../tauri', () => ({
  filterRulesList: vi.fn(),
  filterRuleAdd: vi.fn(),
  filterRuleUpdate: vi.fn(),
  filterRuleRemove: vi.fn(),
  filterRuleSetEnabled: vi.fn(),
}));

import * as tauri from '../tauri';
import { useFilterRulesStore } from './filter-rules';
import type { FilterRule } from '../types';

const rule = (id: string): FilterRule => ({
  id,
  scope: 'global',
  scopeValue: '',
  target: 'signature',
  action: 'strip',
  pattern: null,
  enabled: true,
  note: null,
  createdAt: '2026-06-25T00:00:00Z',
});

beforeEach(() => {
  useFilterRulesStore.setState({ rules: [], error: null });
  vi.clearAllMocks();
});

describe('filterRules store', () => {
  it('loadRules 填充', async () => {
    vi.mocked(tauri.filterRulesList).mockResolvedValue([rule('a'), rule('b')]);
    await useFilterRulesStore.getState().loadRules();
    expect(useFilterRulesStore.getState().rules).toHaveLength(2);
  });

  it('addRule 成功后重拉', async () => {
    vi.mocked(tauri.filterRuleAdd).mockResolvedValue(rule('a'));
    vi.mocked(tauri.filterRulesList).mockResolvedValue([rule('a')]);
    await useFilterRulesStore.getState().addRule({
      scope: 'global',
      scopeValue: '',
      target: 'signature',
      action: 'strip',
      pattern: null,
      enabled: true,
      note: null,
    });
    expect(tauri.filterRuleAdd).toHaveBeenCalledOnce();
    expect(useFilterRulesStore.getState().rules).toHaveLength(1);
  });

  it('addRule 失败写 error', async () => {
    vi.mocked(tauri.filterRuleAdd).mockRejectedValue(new Error('非法正则 pattern'));
    await useFilterRulesStore.getState().addRule({
      scope: 'domain',
      scopeValue: 'x.com',
      target: 'signature',
      action: 'strip',
      pattern: '(',
      enabled: true,
      note: null,
    });
    expect(useFilterRulesStore.getState().error).toContain('非法正则');
  });
});
