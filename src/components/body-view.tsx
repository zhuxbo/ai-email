// CSP injected into every HTML email srcdoc:
// - default-src 'none' 兜底屏蔽脚本 / 字体 / frame / XHR 等远程资源
// - style-src 'unsafe-inline' allows inline CSS (needed for most HTML email layouts)
// - img-src data: https: http: 默认下载远程图片（含 data 内嵌图）。取舍：放开图片即允许
//   tracking pixel 加载（暴露已读 + IP）；脚本仍由 sandbox(无 allow-scripts) + default-src
//   'none' 双重屏蔽，放开图片不影响 XSS 防护。
// Links to external sites are handled by allow-popups on the sandbox (Tauri intercepts them).
export const EMAIL_CSP =
  `<meta http-equiv="Content-Security-Policy" ` +
  `content="default-src 'none'; style-src 'unsafe-inline'; img-src data: https: http:;">`;

export function buildSrcdoc(html: string): string {
  return `${EMAIL_CSP}${html}`;
}

export function BodyView({ html, textPlain }: { html: string | null; textPlain: string | null }) {
  if (html) {
    return (
      <iframe
        // sandbox without allow-same-origin: scripts are blocked and the iframe cannot
        // access parent-frame cookies or storage. allow-popups lets link clicks open in
        // the system browser (Tauri intercepts navigation requests).
        sandbox="allow-popups"
        srcDoc={buildSrcdoc(html)}
        title="message body"
        className="h-full min-h-[300px] w-full rounded border border-slate-200 bg-white dark:border-slate-700 dark:bg-slate-900"
      />
    );
  }
  if (textPlain) {
    return (
      <pre className="whitespace-pre-wrap break-words rounded border border-slate-200 bg-white p-4 font-sans text-sm text-slate-800 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-200">
        {textPlain}
      </pre>
    );
  }
  return <div className="text-sm text-slate-500">这封邮件没有可显示的正文。</div>;
}
