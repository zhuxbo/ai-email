import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';

import { MessageDetail } from './message-detail';
import { useMailStore } from '../lib/store/mail';
import * as tauri from '../lib/tauri';

vi.mock('../lib/tauri', async (importOriginal) => {
  const actual = await importOriginal<typeof tauri>();
  return {
    ...actual,
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
