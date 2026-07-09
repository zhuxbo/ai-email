import { useEffect, useRef, useState } from 'react';
import DOMPurify from 'dompurify';

import type { Category } from '../lib/types';

// 私人/工作默认显示远程图片；其余（通知/推广/垃圾/未分类）默认拦截，防 tracking pixel 暴露已读+IP。
export function imagesAllowedByDefault(category: Category | null): boolean {
  return category === 'personal' || category === 'work';
}

// 清洗 hook（全局注册一次）：拦截图片时把 src 移到 data-blocked-src（渲染不发请求）；
// 外链强制新窗口 + noopener（Tauri 拦截 _blank 导航到系统浏览器）。
// blockImages 为模块级标志，由 sanitizeEmail 在同步 sanitize 前设置（JS 单线程，无并发竞态）。
let blockImages = false;
DOMPurify.addHook('afterSanitizeAttributes', (node) => {
  if (node.nodeName === 'IMG' && blockImages) {
    const src = node.getAttribute('src');
    if (src !== null) {
      node.setAttribute('data-blocked-src', src);
      node.removeAttribute('src');
    }
  }
  if (node.nodeName === 'A') {
    node.setAttribute('target', '_blank');
    node.setAttribute('rel', 'noopener noreferrer');
  }
});

// DOMPurify 默认即移除 <script> / 事件处理器(on*) / javascript: 等危险协议；
// 额外禁掉表单类与可嵌套浏览上下文的标签，缩小邮件可用的交互面。
export function sanitizeEmail(html: string, allowImages: boolean): string {
  blockImages = !allowImages;
  return DOMPurify.sanitize(html, {
    ADD_ATTR: ['target'],
    FORBID_TAGS: ['iframe', 'object', 'embed', 'form', 'input', 'button', 'textarea'],
    FORBID_ATTR: ['style'],
  });
}

const EMAIL_BASE_CSS = `
  :host {
    display: block;
    max-width: 100%;
    overflow-wrap: anywhere;
    color: inherit;
    font: inherit;
  }
  *, *::before, *::after {
    box-sizing: border-box;
  }
  img, video {
    max-width: 100%;
    height: auto;
  }
  table {
    max-width: 100%;
  }
  pre {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
`;

function HtmlBody({ html, category }: { html: string; category: Category | null }) {
  const hostRef = useRef<HTMLDivElement>(null);
  const [showImages, setShowImages] = useState(imagesAllowedByDefault(category));
  const [hasBlocked, setHasBlocked] = useState(false);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    // Shadow DOM 隔离邮件自带 CSS，避免污染 app 界面；容器高度由内容自然撑开（不固定、不滚动）。
    const root = host.shadowRoot ?? host.attachShadow({ mode: 'open' });
    root.innerHTML = `<style>${EMAIL_BASE_CSS}</style>${sanitizeEmail(html, showImages)}`;
    setHasBlocked(root.querySelector('[data-blocked-src]') !== null);
  }, [html, showImages]);

  return (
    <div>
      {hasBlocked && !showImages && (
        <button
          type="button"
          onClick={() => {
            setShowImages(true);
          }}
          className="mb-2 rounded border border-[var(--color-border)] bg-panel px-2 py-1 text-xs text-text-2 hover:opacity-90"
        >
          🖼 图片已拦截（防追踪），点击显示
        </button>
      )}
      <div ref={hostRef} />
    </div>
  );
}

export function BodyView({
  html,
  textPlain,
  category = null,
}: {
  html: string | null;
  textPlain: string | null;
  category?: Category | null;
}) {
  if (html) {
    return <HtmlBody html={html} category={category} />;
  }
  if (textPlain) {
    return (
      <pre className="whitespace-pre-wrap break-words font-sans text-sm text-slate-800 dark:text-slate-200">
        {textPlain}
      </pre>
    );
  }
  return <div className="text-sm text-slate-500">这封邮件没有可显示的正文。</div>;
}
