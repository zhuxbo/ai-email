import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect } from 'vitest';

import { BodyView, sanitizeEmail, imagesAllowedByDefault } from './body-view';

describe('sanitizeEmail（DOMPurify 清洗 — 安全核心）', () => {
  it('移除 <script>', () => {
    const out = sanitizeEmail('<p>hi</p><script>alert(1)</script>', true);
    expect(out).not.toContain('<script');
    expect(out).toContain('hi');
  });

  it('移除事件处理器 on*', () => {
    const out = sanitizeEmail('<img src="x.png" onerror="alert(1)">', true);
    expect(out).not.toContain('onerror');
  });

  it('移除 javascript: 协议', () => {
    const out = sanitizeEmail('<a href="javascript:alert(1)">x</a>', true);
    expect(out.toLowerCase()).not.toContain('javascript:');
  });

  it('外链加 target=_blank + rel noopener', () => {
    const out = sanitizeEmail('<a href="https://example.com">x</a>', true);
    expect(out).toContain('target="_blank"');
    expect(out).toContain('noopener');
  });

  it('禁表单与可嵌套框架标签', () => {
    const out = sanitizeEmail('<form><input></form><iframe src="x"></iframe>', true);
    expect(out).not.toContain('<form');
    expect(out).not.toContain('<iframe');
    expect(out).not.toContain('<input');
  });

  it('allowImages=true 保留 img src', () => {
    const out = sanitizeEmail('<img src="https://example.com/a.png">', true);
    expect(out).toContain('src="https://example.com/a.png"');
    expect(out).not.toContain('data-blocked-src');
  });

  it('allowImages=false 拦截 img：src 移到 data-blocked-src、不再有真实 src', () => {
    const out = sanitizeEmail('<img src="https://example.com/track.png">', false);
    expect(out).toContain('data-blocked-src="https://example.com/track.png"');
    // 真实加载用的 src 属性（前有空格）不再存在；data-blocked-src 不算
    expect(out).not.toMatch(/\ssrc="https/);
  });
});

describe('imagesAllowedByDefault（图片策略）', () => {
  it('私人/工作默认显示', () => {
    expect(imagesAllowedByDefault('personal')).toBe(true);
    expect(imagesAllowedByDefault('work')).toBe(true);
  });
  it('通知/推广/垃圾/未分类默认拦截', () => {
    expect(imagesAllowedByDefault('notification')).toBe(false);
    expect(imagesAllowedByDefault('promotion')).toBe(false);
    expect(imagesAllowedByDefault('spam')).toBe(false);
    expect(imagesAllowedByDefault(null)).toBe(false);
  });
});

describe('BodyView', () => {
  it('纯文本走 pre', () => {
    const { container } = render(<BodyView html={null} textPlain="你好" />);
    expect(container.querySelector('pre')?.textContent).toBe('你好');
  });

  it('拦截类邮件含图片时出现「显示图片」按钮，点击后消失', async () => {
    render(
      <BodyView
        html='<img src="https://example.com/track.png">'
        textPlain={null}
        category="promotion"
      />,
    );
    await waitFor(() => {
      expect(screen.getByText(/图片已拦截/)).toBeInTheDocument();
    });
    fireEvent.click(screen.getByText(/图片已拦截/));
    await waitFor(() => {
      expect(screen.queryByText(/图片已拦截/)).toBeNull();
    });
  });
});
