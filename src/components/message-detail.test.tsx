import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { save } from '@tauri-apps/plugin-dialog';

import { MessageDetail } from './message-detail';
import { useMailStore } from '../lib/store/mail';
import * as tauri from '../lib/tauri';

vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: vi.fn().mockResolvedValue(null),
}));
vi.mock('../lib/tauri', async (importOriginal) => {
  const actual = await importOriginal<typeof tauri>();
  return {
    ...actual,
    messageAttachments: vi.fn().mockResolvedValue([]),
    messageAttachmentSave: vi.fn().mockResolvedValue(undefined),
  };
});

const BASE_MSG = {
  id: 'm1',
  accountId: 'a1',
  subject: 'S',
  toAddrs: [],
  ccAddrs: [],
  fromAddr: 'x',
  sentAt: null,
  hasAttachment: false,
  flags: [],
};

describe('MessageDetail', () => {
  beforeEach(() => {
    useMailStore.setState({
      messages: [BASE_MSG],
      selectedMessageId: 'm1',
      body: { messageId: 'm1', textPlain: 'B', html: null, fetchedAt: '' },
      loadingBody: false,
    } as never);
  });

  it('渲染主题与正文', () => {
    render(<MessageDetail />);
    expect(screen.getByText('S')).toBeInTheDocument();
    expect(screen.getByText('B')).toBeInTheDocument();
  });
});

describe('BodyView HTML iframe — 默认下载图片 + 脚本仍屏蔽', () => {
  beforeEach(() => {
    useMailStore.setState({
      messages: [BASE_MSG],
      selectedMessageId: 'm1',
      body: {
        messageId: 'm1',
        textPlain: null,
        html: '<p>Hello <img src="https://tracker.example.com/pixel.gif"></p>',
        fetchedAt: '',
      },
      loadingBody: false,
    } as never);
  });

  it('iframe 设置了 sandbox 属性', () => {
    render(<MessageDetail />);
    const iframe = document.querySelector('iframe');
    expect(iframe).not.toBeNull();
    expect(iframe?.hasAttribute('sandbox')).toBe(true);
  });

  it('iframe sandbox 不含 allow-same-origin（防止绕过 CSP）', () => {
    render(<MessageDetail />);
    const iframe = document.querySelector('iframe');
    const sandbox = iframe?.getAttribute('sandbox') ?? '';
    expect(sandbox).not.toContain('allow-same-origin');
  });

  it('srcdoc 注入 CSP meta，并以 default-src none 兜底屏蔽脚本等远程资源', () => {
    render(<MessageDetail />);
    const iframe = document.querySelector('iframe');
    const srcdoc = iframe?.getAttribute('srcdoc') ?? '';
    expect(srcdoc.toLowerCase()).toContain('content-security-policy');
    // 仍以 default-src 'none' 兜底：脚本 / 字体 / connect 等远程资源默认屏蔽
    expect(srcdoc).toMatch(/default-src\s+'none'/i);
  });

  it('默认下载远程图片：img-src 允许 https（同时保留 data 内嵌图）', () => {
    render(<MessageDetail />);
    const iframe = document.querySelector('iframe');
    const srcdoc = iframe?.getAttribute('srcdoc') ?? '';
    // 「默认下载图片」：放开 img-src 到远程源，tracking 防护让位于显示便利
    expect(srcdoc).toMatch(/img-src[^;]*\bhttps:/i);
    expect(srcdoc).toMatch(/img-src[^;]*\bdata:/i);
  });

  it('放开图片不放开脚本：sandbox 不含 allow-scripts', () => {
    render(<MessageDetail />);
    const iframe = document.querySelector('iframe');
    const sandbox = iframe?.getAttribute('sandbox') ?? '';
    expect(sandbox).not.toContain('allow-scripts');
  });

  it('原始 HTML 内容仍被包含在 srcdoc 中', () => {
    render(<MessageDetail />);
    const iframe = document.querySelector('iframe');
    const srcdoc = iframe?.getAttribute('srcdoc') ?? '';
    expect(srcdoc).toContain('<p>Hello');
  });
});

describe('MessageDetail 附件', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useMailStore.setState({
      messages: [{ ...BASE_MSG, id: 'm1', hasAttachment: true }],
      selectedMessageId: 'm1',
      body: { messageId: 'm1', textPlain: 'B', html: null, fetchedAt: '' },
      loadingBody: false,
    } as never);
  });

  it('有附件邮件渲染附件名与下载按钮', async () => {
    vi.mocked(tauri.messageAttachments).mockResolvedValueOnce([
      { filename: 'doc.pdf', contentType: 'application/pdf', size: 2048 },
    ]);
    render(<MessageDetail />);
    expect(await screen.findByText(/doc\.pdf/)).toBeInTheDocument();
  });

  it('点击附件触发 save + messageAttachmentSave', async () => {
    vi.mocked(tauri.messageAttachments).mockResolvedValueOnce([
      { filename: 'doc.pdf', contentType: 'application/pdf', size: 2048 },
    ]);
    vi.mocked(save).mockResolvedValueOnce('/tmp/doc.pdf');
    render(<MessageDetail />);
    const btn = await screen.findByRole('button', { name: /doc\.pdf/ });
    fireEvent.click(btn);
    await vi.waitFor(() => {
      expect(tauri.messageAttachmentSave).toHaveBeenCalledWith('m1', 0, '/tmp/doc.pdf');
    });
  });
});
