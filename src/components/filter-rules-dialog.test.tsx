import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('../lib/store/filter-rules', () => ({
  useFilterRulesStore: vi.fn(),
}));

import { useFilterRulesStore } from '../lib/store/filter-rules';
import { FilterRulesPanel } from './filter-rules-dialog';
import type { FilterRule } from '../lib/types';

const baseState = {
  rules: [] as FilterRule[],
  loading: false,
  error: null as string | null,
  loadRules: vi.fn(),
  addRule: vi.fn(),
  updateRule: vi.fn(),
  removeRule: vi.fn(),
  toggleRule: vi.fn(),
  clearError: vi.fn(),
};

function mockStore(partial: Partial<typeof baseState>) {
  const state = { ...baseState, ...partial };
  vi.mocked(useFilterRulesStore).mockImplementation((sel: (s: typeof state) => unknown) =>
    sel(state),
  );
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe('FilterRulesPanel', () => {
  it('渲染规则列表', () => {
    mockStore({
      rules: [
        {
          id: 'r1',
          scope: 'domain',
          scopeValue: 'cnssl.cn',
          target: 'signature',
          action: 'strip',
          pattern: '免责声明',
          enabled: true,
          note: null,
          createdAt: '2026-06-25T00:00:00Z',
        },
      ],
    });
    render(<FilterRulesPanel />);
    expect(screen.getByText(/cnssl\.cn/)).toBeInTheDocument();
  });

  it('global signature strip 提示影响翻译', () => {
    mockStore({});
    render(<FilterRulesPanel />);
    // 选 global + signature + strip → 出现翻译提示。
    fireEvent.change(screen.getByLabelText('作用域'), { target: { value: 'global' } });
    fireEvent.change(screen.getByLabelText('目标'), { target: { value: 'signature' } });
    fireEvent.change(screen.getByLabelText('动作'), { target: { value: 'strip' } });
    expect(screen.getByText(/将同时影响翻译/)).toBeInTheDocument();
  });

  it('提交调用 addRule', () => {
    const addRule = vi.fn();
    mockStore({ addRule });
    render(<FilterRulesPanel />);
    fireEvent.change(screen.getByLabelText('作用域'), { target: { value: 'domain' } });
    fireEvent.change(screen.getByLabelText('作用域值'), { target: { value: 'x.com' } });
    fireEvent.click(screen.getByRole('button', { name: '新增规则' }));
    expect(addRule).toHaveBeenCalledOnce();
  });

  it('非 global 且空 scopeValue 不调用 addRule', () => {
    const addRule = vi.fn();
    mockStore({ addRule });
    render(<FilterRulesPanel />);
    // scope 切到 domain 但不填 scopeValue
    fireEvent.change(screen.getByLabelText('作用域'), { target: { value: 'domain' } });
    fireEvent.click(screen.getByRole('button', { name: '新增规则' }));
    expect(addRule).not.toHaveBeenCalled();
  });

  it('error 显示', () => {
    mockStore({ error: '非法正则 pattern' });
    render(<FilterRulesPanel />);
    expect(screen.getByText(/非法正则/)).toBeInTheDocument();
  });
});
