// Provider presets used to pre-fill the add-account form. Hosts/ports are stable per
// provider; the user only needs to enter email + authorization code (or an app-specific
// password). All presets use implicit-TLS port 465 for SMTP — the backend has no STARTTLS
// fallback, so providers that only offer STARTTLS on 587 would need manual host/port entry.

export type ProviderId = 'qq' | 'exmail' | 'gmail' | 'imap';

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

const EXMAIL_PRESET: ProviderPreset = {
  id: 'exmail',
  label: '腾讯企业邮',
  imapHost: 'imap.exmail.qq.com',
  imapPort: 993,
  smtpHost: 'smtp.exmail.qq.com',
  smtpPort: 465,
  authCodeHelp: '在腾讯企业邮 设置 → 收发信设置 开启 IMAP/SMTP，并使用「客户端专用密码」。',
};

const GMAIL_PRESET: ProviderPreset = {
  id: 'gmail',
  label: 'Gmail',
  imapHost: 'imap.gmail.com',
  imapPort: 993,
  smtpHost: 'smtp.gmail.com',
  smtpPort: 465,
  authCodeHelp: '需先开启两步验证，再到 Google 账户 → 安全性 → 应用专用密码 生成 16 位密码。',
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

export const PROVIDERS: ProviderPreset[] = [QQ_PRESET, EXMAIL_PRESET, GMAIL_PRESET, IMAP_PRESET];

export function providerById(id: string): ProviderPreset {
  return PROVIDERS.find((p) => p.id === id) ?? QQ_PRESET;
}
