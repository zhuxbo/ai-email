import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';

import { MaintenancePanel } from './maintenance-panel';
import * as tauri from '../lib/tauri';

vi.mock('../lib/tauri', async (importOriginal) => {
  const actual = await importOriginal<typeof tauri>();
  return {
    ...actual,
    cacheClear: vi.fn().mockResolvedValue({
      messageBodiesDeleted: 0,
      aiResultsDeleted: 0,
    }),
  };
});

describe('MaintenancePanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('点击清理按钮调用 cacheClear 并显示清理数量', async () => {
    vi.mocked(tauri.cacheClear).mockResolvedValueOnce({
      messageBodiesDeleted: 3,
      aiResultsDeleted: 2,
    });

    render(<MaintenancePanel />);
    fireEvent.click(screen.getByRole('button', { name: '清理本地缓存' }));

    await waitFor(() => {
      expect(tauri.cacheClear).toHaveBeenCalledOnce();
    });
    expect(await screen.findByText('已清理 3 条正文缓存、2 条 AI 缓存。')).toBeInTheDocument();
  });

  it('清理失败时显示错误且允许重试', async () => {
    vi.mocked(tauri.cacheClear).mockRejectedValueOnce(new Error('数据库被占用'));

    render(<MaintenancePanel />);
    const button = screen.getByRole('button', { name: '清理本地缓存' });
    fireEvent.click(button);

    expect(await screen.findByText('数据库被占用')).toBeInTheDocument();
    expect(button).not.toBeDisabled();
  });
});
