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

describe('BodyView HTML — DOMPurify 渲染（不再用 iframe）', () => {
  const HTML_CONVERSATION = {
    threadId: 't',
    sentSyncOk: true,
    messages: [
      {
        id: 'm1',
        accountId: 'a1',
        category: 'promotion',
        isOwn: false,
        html: '<p>Hello <img src="https://tracker.example.com/pixel.gif"></p>',
        textPlain: null,
        fromAddr: 'p@x.com',
        sentAt: null,
      } as never,
    ],
  };

  beforeEach(() => {
    vi.mocked(tauri.conversationThread).mockResolvedValue(HTML_CONVERSATION);
    useMailStore.setState({
      messages: [BASE_MSG],
      selectedMessageId: 'm1',
      body: { messageId: 'm1', textPlain: null, html: null, fetchedAt: '' },
      loadingBody: false,
      conversation: null,
      loadingConversation: false,
    } as never);
  });

  it('html 邮件用 DOMPurify 渲染、不再出现 iframe', async () => {
    render(<MessageDetail />);
    await screen.findByText(/p@x\.com/);
    expect(document.querySelector('iframe')).toBeNull();
  });

  it('推广类邮件含远程图片 → 默认拦截、出现「显示图片」按钮', async () => {
    render(<MessageDetail />);
    expect(await screen.findByText(/图片已拦截/)).toBeInTheDocument();
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
    const headerBtn = await screen.findByRole('button', { name: '跳到下方附件区' });
    fireEvent.click(headerBtn);
    expect(scrollSpy).toHaveBeenCalled();
  });
});
