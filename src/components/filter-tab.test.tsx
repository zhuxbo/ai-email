import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('../lib/store/mail', () => ({ useMailStore: vi.fn() }));
vi.mock('../lib/tauri', () => ({
  messageFilterPreview: vi.fn(),
  messageSetFilterDisabled: vi.fn(),
}));

import { useMailStore } from '../lib/store/mail';
import * as tauri from '../lib/tauri';
import { FilterTab } from './filter-tab';
import type { MessageFilterPreview } from '../lib/types';

function mockSelected(id: string | null) {
  vi.mocked(useMailStore).mockImplementation((sel) => sel({ selectedMessageId: id } as never));
}

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

describe('FilterTab', () => {
  it('挂载即显示净增量与被剥块', async () => {
    mockSelected('m1');
    vi.mocked(tauri.messageFilterPreview).mockResolvedValue(preview());
    render(<FilterTab />);
    expect(await screen.findByText('净增量内容')).toBeInTheDocument();
    expect(screen.getByText(/签名分隔符/)).toBeInTheDocument(); // 被剥块 reason（唯一）
  });

  it('未选邮件时提示', () => {
    mockSelected(null);
    render(<FilterTab />);
    expect(screen.getByText(/在左侧选一封邮件/)).toBeInTheDocument();
  });

  it('disabled 态标注 AI 收完整原文', async () => {
    mockSelected('m1');
    vi.mocked(tauri.messageFilterPreview).mockResolvedValue(preview({ disabled: true }));
    render(<FilterTab />);
    expect(await screen.findByText(/已禁用.*完整原文/)).toBeInTheDocument();
  });

  it('切换本封不过滤调用命令并重拉', async () => {
    mockSelected('m1');
    vi.mocked(tauri.messageFilterPreview).mockResolvedValue(preview());
    vi.mocked(tauri.messageSetFilterDisabled).mockResolvedValue(undefined);
    render(<FilterTab />);
    await screen.findByText('净增量内容');
    fireEvent.click(screen.getByLabelText('本封不过滤（AI 收完整原文）'));
    await waitFor(() => {
      expect(tauri.messageSetFilterDisabled).toHaveBeenCalledWith('m1', true);
    });
    // 挂载 1 次 + 切换后重拉 1 次 = 2 次
    expect(tauri.messageFilterPreview).toHaveBeenCalledTimes(2);
  });

  it('加载失败显示错误信息', async () => {
    mockSelected('m1');
    vi.mocked(tauri.messageFilterPreview).mockRejectedValue(new Error('网络超时'));
    render(<FilterTab />);
    expect(await screen.findByText('网络超时')).toBeInTheDocument();
  });
});
