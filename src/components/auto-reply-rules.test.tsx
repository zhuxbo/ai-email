import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

import { AutoReplyRules } from './auto-reply-rules';
import { useAutoReplyStore } from '../lib/store/auto-reply';
import type { AutoReplyRule } from '../lib/types';

const rule = (id: string, name: string): AutoReplyRule => ({
  id,
  accountId: 'acc-1',
  name,
  enabled: true,
  matchDomain: 'client.com',
  matchCategory: 'work',
  matchPriorityCeiling: 1,
  draftIntent: '礼貌确认',
  createdAt: '2026-06-22T00:00:00Z',
});

beforeEach(() => {
  // 注入 loadRules 为 no-op，避免组件 mount 的 useEffect 打到真 tauri。
  useAutoReplyStore.setState({
    rules: [rule('r1', '工作紧急')],
    rulesAccountId: 'acc-1',
    error: null,
    loadRules: vi.fn().mockResolvedValue(undefined),
  } as never);
});

describe('AutoReplyRules', () => {
  it('渲染规则列表', () => {
    render(<AutoReplyRules accountId="acc-1" />);
    expect(screen.getByText('工作紧急')).toBeInTheDocument();
  });

  it('提交新增调 addRule', () => {
    const addRule = vi.fn().mockResolvedValue(undefined);
    useAutoReplyStore.setState({ addRule } as never);
    render(<AutoReplyRules accountId="acc-1" />);
    fireEvent.change(screen.getByLabelText('规则名称'), { target: { value: '新规则' } });
    fireEvent.change(screen.getByLabelText('回复意图'), { target: { value: '稍后回复' } });
    fireEvent.click(screen.getByRole('button', { name: '新增规则' }));
    expect(addRule).toHaveBeenCalledWith(
      expect.objectContaining({ accountId: 'acc-1', name: '新规则', draftIntent: '稍后回复' }),
    );
  });

  it('切换启用调 toggleRule', () => {
    const toggleRule = vi.fn().mockResolvedValue(undefined);
    useAutoReplyStore.setState({ toggleRule } as never);
    render(<AutoReplyRules accountId="acc-1" />);
    fireEvent.click(screen.getByRole('checkbox', { name: /启用/ }));
    expect(toggleRule).toHaveBeenCalledWith('r1', false);
  });
});
