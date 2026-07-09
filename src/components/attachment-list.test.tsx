import { render, screen } from '@testing-library/react';
import { fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';

import { AttachmentList } from './attachment-list';
import { useMailStore } from '../lib/store/mail';
import * as tauri from '../lib/tauri';

vi.mock('../lib/tauri', async (importOriginal) => {
  const actual = await importOriginal<typeof tauri>();
  return {
    ...actual,
    messageAttachments: vi.fn().mockResolvedValue([]),
    messageAttachmentSave: vi.fn().mockResolvedValue(undefined),
  };
});

describe('AttachmentList', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('挂载且 hasAttachment 时懒加载附件元信息并渲染下载按钮', async () => {
    vi.mocked(tauri.messageAttachments).mockResolvedValueOnce([
      { filename: 'a.pdf', contentType: 'application/pdf', size: 2048 },
    ]);
    render(<AttachmentList messageId="m1" hasAttachment />);
    expect(await screen.findByText(/a\.pdf/)).toBeInTheDocument();
  });

  it('hasAttachment=false 时不调用 messageAttachments', () => {
    render(<AttachmentList messageId="m1" hasAttachment={false} />);
    expect(tauri.messageAttachments).not.toHaveBeenCalled();
  });

  it('加载中显示 loading 文案', async () => {
    // 不 resolve，让它停在 loading 状态
    vi.mocked(tauri.messageAttachments).mockReturnValue(
      new Promise(() => {
        /* never resolves */
      }),
    );
    render(<AttachmentList messageId="m1" hasAttachment />);
    expect(await screen.findByText(/正在读取附件/)).toBeInTheDocument();
  });

  it('附件列表展示文件名和格式化大小', async () => {
    vi.mocked(tauri.messageAttachments).mockResolvedValueOnce([
      { filename: 'report.xlsx', contentType: 'application/vnd.ms-excel', size: 1536 },
    ]);
    render(<AttachmentList messageId="m2" hasAttachment />);
    // 文件名存在
    expect(await screen.findByText(/report\.xlsx/)).toBeInTheDocument();
    // 大小格式化为 KB
    expect(screen.getByText(/1\.5\s*KB/)).toBeInTheDocument();
  });

  it('点击附件下载按钮触发后端另存为命令，不从前端传入文件路径', async () => {
    vi.mocked(tauri.messageAttachments).mockResolvedValueOnce([
      { filename: 'doc.pdf', contentType: 'application/pdf', size: 2048 },
    ]);
    render(<AttachmentList messageId="m3" hasAttachment />);
    const btn = await screen.findByRole('button', { name: /doc\.pdf/ });
    fireEvent.click(btn);
    await waitFor(() => {
      expect(tauri.messageAttachmentSave).toHaveBeenCalledWith('m3', 0);
    });
  });

  it('下载失败时设置全局 error', async () => {
    vi.mocked(tauri.messageAttachments).mockResolvedValueOnce([
      { filename: 'fail.pdf', contentType: 'application/pdf', size: 512 },
    ]);
    vi.mocked(tauri.messageAttachmentSave).mockRejectedValueOnce(new Error('写盘失败'));
    render(<AttachmentList messageId="m5" hasAttachment />);
    const btn = await screen.findByRole('button', { name: /fail\.pdf/ });
    fireEvent.click(btn);
    await waitFor(() => {
      expect(useMailStore.getState().error).toBe('写盘失败');
    });
  });

  it('切换 messageId 时 active 守卫防止迟到结果覆盖', async () => {
    let resolveFirst!: (v: { filename: string; contentType: string; size: number }[]) => void;
    const first = new Promise<{ filename: string; contentType: string; size: number }[]>(
      (r) => (resolveFirst = r),
    );
    vi.mocked(tauri.messageAttachments)
      .mockReturnValueOnce(first)
      .mockResolvedValueOnce([{ filename: 'b.pdf', contentType: 'application/pdf', size: 1024 }]);

    const { rerender } = render(<AttachmentList messageId="m1" hasAttachment />);
    // 切换到新 messageId
    rerender(<AttachmentList messageId="m2" hasAttachment />);
    // 让第二个 resolve 先到
    await screen.findByText(/b\.pdf/);
    // 然后 resolve 第一个（迟到）
    resolveFirst([{ filename: 'a.pdf', contentType: 'application/pdf', size: 512 }]);
    // a.pdf 不应该出现（被 active 守卫挡住）
    await waitFor(() => {
      expect(screen.queryByText(/a\.pdf/)).toBeNull();
    });
  });
});
