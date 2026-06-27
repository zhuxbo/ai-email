import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { ConversationThread } from './conversation-thread';
import type { ConversationView } from '../lib/types';
import * as tauri from '../lib/tauri';

vi.mock('../lib/tauri');

// sentAt 用无时区偏移串：按本地时区解析、格式化也按本地 → 时间断言与运行时区无关。
const view: ConversationView = {
  threadId: 't1',
  sentSyncOk: true,
  messages: [
    {
      id: 'm1',
      accountId: 'acc1',
      fromAddr: 'peer@x.com',
      sentAt: '2026-06-25T10:00:00',
      snippet: '第一封预览',
      textPlain: '第一封',
      html: null,
      isOwn: false,
      hasAttachment: false,
    } as never,
    {
      id: 'm2',
      accountId: 'acc1',
      fromAddr: 'me@qq.com',
      sentAt: '2026-06-25T11:00:00',
      snippet: '我的回复预览',
      textPlain: '我的回复',
      html: null,
      isOwn: true,
      hasAttachment: false,
    } as never,
  ],
};

describe('ConversationThread', () => {
  beforeEach(() => {
    vi.mocked(tauri.messageAttachments).mockResolvedValue([]);
    vi.mocked(tauri.messageBody).mockResolvedValue({ textPlain: null, html: null } as never);
  });

  it('最新一封展开、其余折叠，最新排在最上（倒序）', () => {
    const { container } = render(<ConversationThread view={view} />);
    expect(screen.getByText('我的回复')).toBeInTheDocument(); // m2 正文展开
    expect(screen.getByText('第一封预览')).toBeInTheDocument(); // m1 折叠显示 snippet
    expect(screen.queryByText('第一封')).not.toBeInTheDocument(); // m1 正文未展开
    // 倒序：最新 m2 的正文在 DOM 中先于 m1 的 snippet
    const html = container.innerHTML;
    expect(html.indexOf('我的回复')).toBeLessThan(html.indexOf('第一封预览'));
  });

  it('折叠态显示中文格式时间', () => {
    render(<ConversationThread view={view} />);
    expect(screen.getByText('2026年6月25日 10:00')).toBeInTheDocument(); // m1 折叠态
  });

  it('显示真实发件人名而非「我」', () => {
    render(<ConversationThread view={view} />);
    // m2 自己发的，仍显示邮箱地址、不显示「我」
    expect(screen.getByText('me@qq.com')).toBeInTheDocument();
    expect(screen.queryByText('我')).not.toBeInTheDocument();
  });

  it('点击折叠行展开正文', () => {
    render(<ConversationThread view={view} />);
    fireEvent.click(screen.getByRole('button', { name: /展开.*peer@x\.com/ }));
    expect(screen.getByText('第一封')).toBeInTheDocument();
  });

  it('展开态点头部可收起正文', () => {
    render(<ConversationThread view={view} />);
    expect(screen.getByText('我的回复')).toBeInTheDocument(); // 展开态正文
    fireEvent.click(screen.getByRole('button', { name: /收起.*me@qq\.com/ }));
    expect(screen.queryByText('我的回复')).not.toBeInTheDocument(); // 正文收起
    expect(screen.getByText('我的回复预览')).toBeInTheDocument(); // 变为折叠 snippet
  });

  it('sentSyncOk=false 显示提示', () => {
    render(<ConversationThread view={{ ...view, sentSyncOk: false }} />);
    expect(screen.getByText(/已发件箱同步未完成/)).toBeInTheDocument();
  });

  it('空会话不渲染任何 block 也不崩溃', () => {
    const { container } = render(
      <ConversationThread view={{ threadId: null, sentSyncOk: true, messages: [] }} />,
    );
    expect(container.querySelectorAll('button').length).toBe(0);
    expect(container.querySelector('.conversation-thread')).toBeInTheDocument();
  });

  it('展开一个 block 不影响其他 block 的开合', () => {
    const v: ConversationView = {
      threadId: 't',
      sentSyncOk: true,
      messages: [
        {
          id: 'a',
          accountId: 'acc1',
          fromAddr: 'a@x.com',
          sentAt: null,
          snippet: 'A预览',
          textPlain: 'A正文',
          html: null,
          isOwn: false,
          hasAttachment: false,
        } as never,
        {
          id: 'b',
          accountId: 'acc1',
          fromAddr: 'b@x.com',
          sentAt: null,
          snippet: 'B预览',
          textPlain: 'B正文',
          html: null,
          isOwn: false,
          hasAttachment: false,
        } as never,
        {
          id: 'c',
          accountId: 'acc1',
          fromAddr: 'c@x.com',
          sentAt: null,
          snippet: 'C预览',
          textPlain: 'C正文',
          html: null,
          isOwn: false,
          hasAttachment: false,
        } as never,
      ],
    };
    render(<ConversationThread view={v} />);
    // 倒序后 c 在最上、默认展开；a/b 折叠
    expect(screen.getByText('C正文')).toBeInTheDocument();
    expect(screen.queryByText('A正文')).not.toBeInTheDocument();
    expect(screen.queryByText('B正文')).not.toBeInTheDocument();
    // 展开 a（折叠态按钮）
    fireEvent.click(screen.getByRole('button', { name: /展开.*a@x\.com/ }));
    expect(screen.getByText('A正文')).toBeInTheDocument();
    expect(screen.queryByText('B正文')).not.toBeInTheDocument();
    expect(screen.getByText('C正文')).toBeInTheDocument();
  });

  it('MessageBlock 展开且 hasAttachment 时调 messageAttachments 渲染附件、折叠态不调', async () => {
    // 构造 view：m3 有附件（首块，倒序后展开），m4 有附件（第二块，折叠）
    vi.mocked(tauri.messageAttachments).mockResolvedValue([
      { filename: 'doc.pdf', contentType: 'application/pdf', size: 1024 },
    ]);
    const v: ConversationView = {
      threadId: 'tx',
      sentSyncOk: true,
      messages: [
        {
          id: 'm3',
          accountId: 'acc1',
          fromAddr: 'a@x.com',
          sentAt: '2026-06-25T09:00:00',
          snippet: 'm3预览',
          textPlain: 'm3正文',
          html: null,
          isOwn: false,
          hasAttachment: true,
          category: 'inbox',
        } as never,
        {
          id: 'm4',
          accountId: 'acc1',
          fromAddr: 'b@x.com',
          sentAt: '2026-06-25T10:00:00',
          snippet: 'm4预览',
          textPlain: 'm4正文',
          html: null,
          isOwn: false,
          hasAttachment: true,
          category: 'inbox',
        } as never,
      ],
    };
    render(<ConversationThread view={v} />);
    // 倒序后 m4 在最上、展开 → messageAttachments 被以 m4.id 调用，doc.pdf 出现
    await waitFor(() => {
      expect(tauri.messageAttachments).toHaveBeenCalledWith('m4');
      expect(screen.getByText(/doc\.pdf/)).toBeInTheDocument();
    });
    // m3 折叠 → messageAttachments 未以 m3.id 调用
    expect(tauri.messageAttachments).not.toHaveBeenCalledWith('m3');
  });

  it('展开块调 messageAttachments 但不调 messageBody（正文预填，无 body 懒拉）', async () => {
    vi.mocked(tauri.messageAttachments).mockResolvedValue([
      { filename: 'img.png', contentType: 'image/png', size: 2048 },
    ]);
    const v: ConversationView = {
      threadId: 'ty',
      sentSyncOk: true,
      messages: [
        {
          id: 'm5',
          accountId: 'acc1',
          fromAddr: 'c@x.com',
          sentAt: '2026-06-25T12:00:00',
          snippet: 'm5预览',
          textPlain: 'm5正文',
          html: null,
          isOwn: false,
          hasAttachment: true,
          category: 'inbox',
        } as never,
      ],
    };
    render(<ConversationThread view={v} />);
    // 单封 → 默认展开，应拉 attachments
    await waitFor(() => {
      expect(tauri.messageAttachments).toHaveBeenCalledWith('m5');
    });
    // 正文已预填（textPlain），不应触发 messageBody
    expect(tauri.messageBody).not.toHaveBeenCalled();
  });
});
