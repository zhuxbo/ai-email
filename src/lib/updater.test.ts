import { describe, expect, it } from 'vitest';

import { detectUpdatePlatform } from './updater';

describe('detectUpdatePlatform', () => {
  it('识别 Android 与 macOS，其它平台返回 unsupported', () => {
    expect(detectUpdatePlatform('Mozilla/5.0 (Linux; Android 15)')).toBe('android');
    expect(detectUpdatePlatform('Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)')).toBe('macos');
    expect(detectUpdatePlatform('Mozilla/5.0 (X11; Linux x86_64)')).toBe('unsupported');
  });
});
