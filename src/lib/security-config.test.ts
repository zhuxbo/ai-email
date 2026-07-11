import { describe, expect, it } from 'vitest';

import capability from '../../src-tauri/capabilities/default.json';
import fileProviderPaths from '../../src-tauri/gen/android/app/src/main/res/xml/file_paths.xml?raw';
import tauriConfig from '../../src-tauri/tauri.conf.json';

describe('生产安全配置', () => {
  it('Tauri 主窗口启用 CSP 并关闭不需要的 WebView 能力', () => {
    expect(tauriConfig.app.security.csp).toEqual(expect.stringContaining("default-src 'self'"));
    expect(tauriConfig.app.security.csp).toEqual(expect.stringContaining("object-src 'none'"));
    expect(tauriConfig.app.security.csp).toEqual(expect.stringContaining("form-action 'none'"));
    expect(tauriConfig.app.security.devCsp).toEqual(expect.stringContaining('ws://localhost:1421'));
    expect(tauriConfig.app.windows[0]?.allowLinkPreview).toBe(false);
    expect(tauriConfig.app.windows[0]?.generalAutofillEnabled).toBe(false);
  });

  it('默认 capability 不暴露 opener/dialog 插件权限', () => {
    expect(capability.permissions).not.toContain('opener:default');
    expect(capability.permissions).not.toContain('dialog:default');
  });

  it('不配置 Tauri updater 插件且不放宽默认 opener/dialog 权限', () => {
    expect(tauriConfig).not.toHaveProperty('plugins.updater');
    expect(capability.permissions).toEqual(['core:default']);
    expect(capability.permissions).not.toContain('opener:default');
    expect(capability.permissions).not.toContain('dialog:default');
    expect(capability.permissions.join('\n')).not.toContain('shell:');
  });

  it('Android FileProvider 不暴露整个 external storage', () => {
    expect(fileProviderPaths).not.toContain('<external-path');
    expect(fileProviderPaths).toContain('path="attachments/"');
  });
});
