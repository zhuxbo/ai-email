// Provider presets used to pre-fill the add-account form. Hosts/ports are stable per
// provider; the user only needs to enter email + authorization code. Extend this table
// when we add 163 / Gmail in later sprints.

export type ProviderId = 'qq' | 'imap';

export interface ProviderPreset {
  id: ProviderId;
  label: string;
  imapHost: string;
  imapPort: number;
  smtpHost: string;
  smtpPort: number;
  authCodeHelp: string;
}

const QQ_PRESET: ProviderPreset = {
  id: 'qq',
  label: 'QQ 邮箱',
  imapHost: 'imap.qq.com',
  imapPort: 993,
  smtpHost: 'smtp.qq.com',
  smtpPort: 465,
  authCodeHelp: '在 QQ 邮箱网页端 设置 → 账户 中开启 IMAP/SMTP 服务后获取 16 位授权码。',
};

const IMAP_PRESET: ProviderPreset = {
  id: 'imap',
  label: '其他 IMAP',
  imapHost: '',
  imapPort: 993,
  smtpHost: '',
  smtpPort: 465,
  authCodeHelp: '使用账号密码或服务商提供的应用专用密码。',
};

export const PROVIDERS: ProviderPreset[] = [QQ_PRESET, IMAP_PRESET];

export function providerById(id: string): ProviderPreset {
  return PROVIDERS.find((p) => p.id === id) ?? QQ_PRESET;
}
