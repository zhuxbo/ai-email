import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('../lib/tauri', () => ({
  messageFilterPreview: vi.fn(),
  messageSetFilterDisabled: vi.fn(),
}));

import * as tauri from '../lib/tauri';
import { FilterPreview } from './filter-preview';
import type { MessageFilterPreview } from '../lib/types';

const preview = (over: Partial<MessageFilterPreview> = {}): MessageFilterPreview => ({
  net: '净增量内容',
  removed: [{ kind: 'signature', text: '-- \nAlice\n138xxxx', reason: '签名分隔符' }],
  disabled: false,
  original: '完整原文含签名',
  ...over,
});

beforeEach(() => {
  vi.clearAllMocks();
});

describe('FilterPreview', () => {
  it('展开后显示净增量与被剥块', async () => {
    vi.mocked(tauri.messageFilterPreview).mockResolvedValue(preview());
    render(<FilterPreview messageId="m1" />);
    fireEvent.click(screen.getByRole('button', { name: /按当前规则会剥成/ }));
    expect(await screen.findByText('净增量内容')).toBeInTheDocument();
    // 被剥块折叠摘要(类型 + 原因)。
    expect(screen.getByText(/签名/)).toBeInTheDocument();
    expect(screen.getByText(/签名分隔符/)).toBeInTheDocument();
  });

  it('disabled 态标注 AI 收完整原文', async () => {
    vi.mocked(tauri.messageFilterPreview).mockResolvedValue(preview({ disabled: true }));
    render(<FilterPreview messageId="m1" />);
    fireEvent.click(screen.getByRole('button', { name: /按当前规则会剥成/ }));
    expect(await screen.findByText(/已禁用.*完整原文/)).toBeInTheDocument();
  });

  it('切换本封不过滤调用命令并重拉', async () => {
    vi.mocked(tauri.messageFilterPreview).mockResolvedValue(preview());
    vi.mocked(tauri.messageSetFilterDisabled).mockResolvedValue(undefined);
    render(<FilterPreview messageId="m1" />);
    fireEvent.click(screen.getByRole('button', { name: /按当前规则会剥成/ }));
    await screen.findByText('净增量内容');
    fireEvent.click(screen.getByLabelText('本封不过滤（AI 收完整原文）'));
    await waitFor(() => {
      expect(tauri.messageSetFilterDisabled).toHaveBeenCalledWith('m1', true);
    });
    // 重拉：展开时 1 次 + 切换后 1 次 = 2 次
    expect(tauri.messageFilterPreview).toHaveBeenCalledTimes(2);
  });

  it('加载失败显示错误信息', async () => {
    vi.mocked(tauri.messageFilterPreview).mockRejectedValue(new Error('网络超时'));
    render(<FilterPreview messageId="m1" />);
    fireEvent.click(screen.getByRole('button', { name: /按当前规则会剥成/ }));
    expect(await screen.findByText('网络超时')).toBeInTheDocument();
  });
});
