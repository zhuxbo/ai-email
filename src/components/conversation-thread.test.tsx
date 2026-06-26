import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { ConversationThread } from './conversation-thread';
import type { ConversationView } from '../lib/types';

const view: ConversationView = {
  threadId: 't1',
  sentSyncOk: true,
  messages: [
    {
      id: 'm1',
      fromAddr: 'peer@x.com',
      sentAt: '2026-06-25T10:00:00+00:00',
      snippet: '第一封预览',
      textPlain: '第一封',
      html: null,
      isOwn: false,
    } as never,
    {
      id: 'm2',
      fromAddr: 'me@qq.com',
      sentAt: '2026-06-25T11:00:00+00:00',
      textPlain: '我的回复',
      html: null,
      isOwn: true,
    } as never,
  ],
};

describe('ConversationThread', () => {
  it('最新一封展开，其余折叠', () => {
    render(<ConversationThread view={view} />);
    expect(screen.getByText('我的回复')).toBeInTheDocument(); // m2 正文展开
    expect(screen.getByText('第一封预览')).toBeInTheDocument(); // m1 折叠显示 snippet
    expect(screen.queryByText('第一封')).not.toBeInTheDocument(); // m1 正文未展开（snippet≠textPlain）
  });
  it('点击折叠行展开正文', () => {
    render(<ConversationThread view={view} />);
    fireEvent.click(screen.getByRole('button', { name: /peer@x.com/ }));
    expect(screen.getByText('第一封')).toBeInTheDocument();
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
    // 无 banner（sentSyncOk=true），容器存在即不崩
    expect(container.querySelector('.conversation-thread')).toBeInTheDocument();
  });
  it('展开一个 block 不影响其他 block 的开合', () => {
    // 三封：c 默认展开，a/b 折叠
    const v: ConversationView = {
      threadId: 't',
      sentSyncOk: true,
      messages: [
        {
          id: 'a',
          fromAddr: 'a@x.com',
          sentAt: null,
          snippet: 'A预览',
          textPlain: 'A正文',
          html: null,
          isOwn: false,
        } as never,
        {
          id: 'b',
          fromAddr: 'b@x.com',
          sentAt: null,
          snippet: 'B预览',
          textPlain: 'B正文',
          html: null,
          isOwn: false,
        } as never,
        {
          id: 'c',
          fromAddr: 'c@x.com',
          sentAt: null,
          snippet: 'C预览',
          textPlain: 'C正文',
          html: null,
          isOwn: false,
        } as never,
      ],
    };
    render(<ConversationThread view={v} />);
    // 初始：c 展开（C正文 可见），a/b 折叠（A正文/B正文 不可见）
    expect(screen.getByText('C正文')).toBeInTheDocument();
    expect(screen.queryByText('A正文')).not.toBeInTheDocument();
    expect(screen.queryByText('B正文')).not.toBeInTheDocument();
    // 展开 a
    fireEvent.click(screen.getByRole('button', { name: /a@x.com/ }));
    expect(screen.getByText('A正文')).toBeInTheDocument();
    // b 仍折叠、c 仍展开（各块独立）
    expect(screen.queryByText('B正文')).not.toBeInTheDocument();
    expect(screen.getByText('C正文')).toBeInTheDocument();
  });
});
