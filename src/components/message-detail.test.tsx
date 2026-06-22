import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';

import { MessageDetail } from './message-detail';
import { useMailStore } from '../lib/store/mail';

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

describe('BodyView HTML iframe — #14 远程资源拦截', () => {
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

  it('srcdoc 内注入了 CSP meta 标签阻断远程资源', () => {
    render(<MessageDetail />);
    const iframe = document.querySelector('iframe');
    const srcdoc = iframe?.getAttribute('srcdoc') ?? '';
    // 必须包含 Content-Security-Policy meta，且 img-src / default-src 不包含 https:
    expect(srcdoc.toLowerCase()).toContain('content-security-policy');
    // img-src 不应允许任意 https: 远程源
    expect(srcdoc).not.toMatch(/img-src\s+https:/i);
    // default-src 'none' 或明确只允许 data: / 'unsafe-inline'
    expect(srcdoc).toMatch(/default-src\s+'none'/i);
  });

  it('原始 HTML 内容仍被包含在 srcdoc 中', () => {
    render(<MessageDetail />);
    const iframe = document.querySelector('iframe');
    const srcdoc = iframe?.getAttribute('srcdoc') ?? '';
    expect(srcdoc).toContain('<p>Hello');
  });
});
