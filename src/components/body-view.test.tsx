import { render } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { BodyView, buildSrcdoc } from './body-view';

describe('BodyView', () => {
  it('HTML 正文注入 CSP 且 sandbox 无 allow-scripts', () => {
    const { container } = render(<BodyView html="<p>hi</p>" textPlain={null} />);
    const iframe = container.querySelector('iframe');
    expect(iframe).not.toBeNull();
    expect(iframe?.getAttribute('sandbox')).toBe('allow-popups');
    expect(iframe?.getAttribute('srcdoc')).toContain("default-src 'none'");
  });
  it('buildSrcdoc 前置 CSP meta', () => {
    expect(buildSrcdoc('<p>x</p>')).toContain('Content-Security-Policy');
  });
  it('纯文本走 pre', () => {
    const { container } = render(<BodyView html={null} textPlain="你好" />);
    expect(container.querySelector('pre')?.textContent).toBe('你好');
  });
});
