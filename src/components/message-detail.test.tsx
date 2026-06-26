import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
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
    // 每个 describe 块里触发的 loadConversation 效果都需要此 stub，防止悬挂 promise
    conversationThread: vi.fn().mockResolvedValue({
      threadId: 't',
      sentSyncOk: true,
      messages: [
        {
          id: 'm1',
          isOwn: false,
          textPlain: '会话正文',
          html: null,
          fromAddr: 'p@x.com',
          sentAt: null,
        } as never,
      ],
    }),
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

  it('渲染主题与正文', async () => {
    render(<MessageDetail />);
    // 主题仍渲染（header 区不受 conversation 切换影响）
    expect(screen.getByText('S')).toBeInTheDocument();
    // 正文现在由对话流渲染，store.body.textPlain('B') 不再直接出现在 body 区
    // 等待 loadConversation effect 完成，避免状态更新落在 act() 外
    await waitFor(() => {
      expect(tauri.conversationThread).toHaveBeenCalledWith('m1');
    });
  });

  it('选中邮件渲染对话流正文', async () => {
    useMailStore.setState({
      messages: [{ ...BASE_MSG, subject: 's', fromAddr: 'p@x.com' }],
      selectedMessageId: 'm1',
      conversation: {
        threadId: 't',
        sentSyncOk: true,
        messages: [
          {
            id: 'm1',
            isOwn: false,
            textPlain: '会话正文',
            html: null,
            fromAddr: 'p@x.com',
            sentAt: null,
          } as never,
        ],
      },
    } as never);
    render(<MessageDetail />);
    expect(await screen.findByText('会话正文')).toBeInTheDocument();
  });
});

describe('BodyView HTML iframe — 默认下载图片 + 脚本仍屏蔽', () => {
  const HTML_CONVERSATION = {
    threadId: 't',
    sentSyncOk: true,
    messages: [
      {
        id: 'm1',
        isOwn: false,
        html: '<p>Hello <img src="https://tracker.example.com/pixel.gif"></p>',
        textPlain: null,
        fromAddr: 'p@x.com',
        sentAt: null,
      } as never,
    ],
  };

  beforeEach(() => {
    // conversationThread mock 返回含 HTML 的会话（覆盖文件级 stub）
    vi.mocked(tauri.conversationThread).mockResolvedValue(HTML_CONVERSATION);
    // 通过 conversation 驱动 HTML 渲染（Task 13 后 body 区渲染 ConversationThread）
    useMailStore.setState({
      messages: [BASE_MSG],
      selectedMessageId: 'm1',
      body: { messageId: 'm1', textPlain: null, html: null, fetchedAt: '' },
      loadingBody: false,
      conversation: null,
      loadingConversation: false,
    } as never);
  });

  // loadConversation 先 set conversation=null 再回填，所以需要等 iframe 出现后再断言
  async function getIframe() {
    return waitFor(() => {
      const el = document.querySelector('iframe');
      if (el === null) throw new Error('iframe not found');
      return el;
    });
  }

  it('iframe 设置了 sandbox 属性', async () => {
    render(<MessageDetail />);
    const iframe = await getIframe();
    expect(iframe.hasAttribute('sandbox')).toBe(true);
  });

  it('iframe sandbox 不含 allow-same-origin（防止绕过 CSP）', async () => {
    render(<MessageDetail />);
    const sandbox = (await getIframe()).getAttribute('sandbox') ?? '';
    expect(sandbox).not.toContain('allow-same-origin');
  });

  it('srcdoc 注入 CSP meta，并以 default-src none 兜底屏蔽脚本等远程资源', async () => {
    render(<MessageDetail />);
    const srcdoc = (await getIframe()).getAttribute('srcdoc') ?? '';
    expect(srcdoc.toLowerCase()).toContain('content-security-policy');
    // 仍以 default-src 'none' 兜底：脚本 / 字体 / connect 等远程资源默认屏蔽
    expect(srcdoc).toMatch(/default-src\s+'none'/i);
  });

  it('默认下载远程图片：img-src 允许 https（同时保留 data 内嵌图）', async () => {
    render(<MessageDetail />);
    const srcdoc = (await getIframe()).getAttribute('srcdoc') ?? '';
    // 「默认下载图片」：放开 img-src 到远程源，tracking 防护让位于显示便利
    expect(srcdoc).toMatch(/img-src[^;]*\bhttps:/i);
    expect(srcdoc).toMatch(/img-src[^;]*\bdata:/i);
  });

  it('放开图片不放开脚本：sandbox 不含 allow-scripts', async () => {
    render(<MessageDetail />);
    const sandbox = (await getIframe()).getAttribute('sandbox') ?? '';
    expect(sandbox).not.toContain('allow-scripts');
  });

  it('原始 HTML 内容仍被包含在 srcdoc 中', async () => {
    render(<MessageDetail />);
    const srcdoc = (await getIframe()).getAttribute('srcdoc') ?? '';
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

  it('顶部「含附件」可点击并滚动到附件区', async () => {
    vi.mocked(tauri.messageAttachments).mockResolvedValueOnce([
      { filename: 'doc.pdf', contentType: 'application/pdf', size: 2048 },
    ]);
    const scrollSpy = vi.fn();
    Element.prototype.scrollIntoView = scrollSpy;
    render(<MessageDetail />);
    const headerBtn = await screen.findByRole('button', { name: '📎 含附件' });
    fireEvent.click(headerBtn);
    expect(scrollSpy).toHaveBeenCalled();
  });
});
