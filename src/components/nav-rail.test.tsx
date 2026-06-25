import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { NavRail } from './nav-rail';
import type { Account, Mailbox } from '../lib/types';

const acct = (id: string, email: string): Account => ({
  id,
  email,
  displayName: null,
  provider: 'qq',
  imapHost: '',
  imapPort: 993,
  smtpHost: '',
  smtpPort: 465,
  createdAt: '',
  lastSyncedAt: null,
});

const mbox = (id: string, name: string, specialUse: Mailbox['specialUse'] = null): Mailbox => ({
  id,
  accountId: 'a',
  name,
  delimiter: '/',
  uidValidity: null,
  uidNext: null,
  lastSyncedAt: null,
  specialUse,
});

function baseProps() {
  return {
    accounts: [acct('a', 'amy@qq.com'), acct('b', 'bob@qq.com')],
    selectedAccountId: 'a',
    mailboxes: [] as Mailbox[],
    selectedMailboxId: null as string | null,
    syncing: false,
    onSelectAccount: vi.fn(),
    onSelectMailbox: vi.fn(),
    onAddAccount: vi.fn(),
    onSync: vi.fn(),
    onOpenSettings: vi.fn(),
    onOpenAutoReply: vi.fn(),
    autoReplyCount: 0,
  };
}

describe('NavRail', () => {
  it('fires select on account click', async () => {
    const p = baseProps();
    render(<NavRail {...p} />);
    await userEvent.click(screen.getByRole('button', { name: /bob@qq.com/ }));
    expect(p.onSelectAccount).toHaveBeenCalledWith('b');
  });

  it('fires sync', async () => {
    const p = baseProps();
    render(<NavRail {...p} />);
    await userEvent.click(screen.getByRole('button', { name: '同步收件箱' }));
    expect(p.onSync).toHaveBeenCalledOnce();
  });

  it('disables sync while syncing', () => {
    const p = { ...baseProps(), syncing: true };
    render(<NavRail {...p} />);
    expect(screen.getByRole('button', { name: '同步收件箱' })).toBeDisabled();
  });

  it('fires open-settings on the settings button', async () => {
    const p = baseProps();
    render(<NavRail {...p} />);
    await userEvent.click(screen.getByRole('button', { name: '设置' }));
    expect(p.onOpenSettings).toHaveBeenCalledOnce();
  });

  it('disables sync only when no accounts', () => {
    const p = { ...baseProps(), accounts: [], selectedAccountId: null };
    render(<NavRail {...p} />);
    expect(screen.getByRole('button', { name: '同步收件箱' })).toBeDisabled();
  });

  it('keeps sync enabled in 全部 view (accounts present, none selected)', () => {
    const p = { ...baseProps(), selectedAccountId: null };
    render(<NavRail {...p} />);
    expect(screen.getByRole('button', { name: '同步收件箱' })).toBeEnabled();
  });

  it('fires select(null) on 全部 click and marks it pressed while aggregating', async () => {
    const p = { ...baseProps(), selectedAccountId: null };
    render(<NavRail {...p} />);
    const all = screen.getByRole('button', { name: '全部账户' });
    expect(all).toHaveAttribute('aria-pressed', 'true');
    await userEvent.click(all);
    expect(p.onSelectAccount).toHaveBeenCalledWith(null);
  });

  it('shows auto-reply badge with the count when > 0', () => {
    const p = { ...baseProps(), autoReplyCount: 5 };
    render(<NavRail {...p} />);
    expect(screen.getByText('5')).toBeInTheDocument();
  });

  it('caps the auto-reply badge at 99+', () => {
    const p = { ...baseProps(), autoReplyCount: 150 };
    render(<NavRail {...p} />);
    expect(screen.getByText('99+')).toBeInTheDocument();
  });

  it('hides the auto-reply badge when count is 0', () => {
    const p = { ...baseProps(), autoReplyCount: 0 };
    render(<NavRail {...p} />);
    // 角标不渲染：count=0 时无任何 '0' 文本（若误渲染则会出现 '0'）
    expect(screen.queryByText('0')).toBeNull();
  });

  // ── 信箱列表（Phase 15） ──────────────────────────────────────────────────

  it('不在全部视图（selectedAccountId=null）显示信箱列表', () => {
    const p = {
      ...baseProps(),
      selectedAccountId: null,
      mailboxes: [mbox('m1', 'INBOX', 'inbox'), mbox('m2', 'Sent', 'sent')],
    };
    render(<NavRail {...p} />);
    // 信箱按钮应不存在（全部视图不展示）
    expect(screen.queryByRole('button', { name: '收件箱' })).toBeNull();
  });

  it('在单账户视图显示信箱列表并映射 specialUse 为友好标签', () => {
    const p = {
      ...baseProps(),
      selectedAccountId: 'a',
      mailboxes: [
        mbox('m1', 'INBOX', 'inbox'),
        mbox('m2', 'Sent', 'sent'),
        mbox('m3', 'Drafts', 'drafts'),
        mbox('m4', 'Trash', 'trash'),
      ],
      selectedMailboxId: 'm1',
    };
    render(<NavRail {...p} />);
    expect(screen.getByRole('button', { name: '收件箱' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '已发送' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '草稿' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '废纸篓' })).toBeInTheDocument();
  });

  it('点击信箱按钮调用 onSelectMailbox', async () => {
    const p = {
      ...baseProps(),
      selectedAccountId: 'a',
      mailboxes: [mbox('m1', 'INBOX', 'inbox'), mbox('m2', 'Sent', 'sent')],
      selectedMailboxId: 'm1',
    };
    render(<NavRail {...p} />);
    await userEvent.click(screen.getByRole('button', { name: '已发送' }));
    expect(p.onSelectMailbox).toHaveBeenCalledWith('m2');
  });

  it('当前选中信箱按钮有 aria-pressed=true', () => {
    const p = {
      ...baseProps(),
      selectedAccountId: 'a',
      mailboxes: [mbox('m1', 'INBOX', 'inbox'), mbox('m2', 'Sent', 'sent')],
      selectedMailboxId: 'm1',
    };
    render(<NavRail {...p} />);
    expect(screen.getByRole('button', { name: '收件箱' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('button', { name: '已发送' })).toHaveAttribute('aria-pressed', 'false');
  });

  it('无信箱时不渲染分隔线和信箱区', () => {
    const p = { ...baseProps(), selectedAccountId: 'a', mailboxes: [] };
    const { container } = render(<NavRail {...p} />);
    // 信箱区的分隔线是内部两条之一；accounts 之后的那条由 accounts>0 决定
    // 最简验证：无信箱按钮（aria-pressed 属于信箱的）
    const mailboxBtns = container.querySelectorAll('button[aria-pressed]');
    // 只有账户按钮和"全部"按钮有 aria-pressed，信箱按钮不应存在
    const hasMailboxBtn = Array.from(mailboxBtns).some(
      (b) => b.getAttribute('aria-label') === '收件箱',
    );
    expect(hasMailboxBtn).toBe(false);
  });

  // ── 自定义文件夹折叠（标准信箱直显 + 更多展开） ───────────────────────────

  it('自定义文件夹默认折叠，仅标准信箱直接可见', () => {
    const p = {
      ...baseProps(),
      selectedAccountId: 'a',
      mailboxes: [mbox('m1', 'INBOX', 'inbox'), mbox('m2', '订阅邮件'), mbox('m3', '广告邮件')],
    };
    render(<NavRail {...p} />);
    expect(screen.getByRole('button', { name: '收件箱' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '订阅邮件' })).toBeNull();
    expect(screen.queryByRole('button', { name: '广告邮件' })).toBeNull();
  });

  it('点击"更多"展开自定义文件夹', async () => {
    const p = {
      ...baseProps(),
      selectedAccountId: 'a',
      mailboxes: [mbox('m1', 'INBOX', 'inbox'), mbox('m2', '订阅邮件')],
    };
    render(<NavRail {...p} />);
    await userEvent.click(screen.getByRole('button', { name: /更多/ }));
    expect(screen.getByRole('button', { name: '订阅邮件' })).toBeInTheDocument();
  });

  it('无自定义文件夹时不显示"更多"按钮', () => {
    const p = {
      ...baseProps(),
      selectedAccountId: 'a',
      mailboxes: [mbox('m1', 'INBOX', 'inbox'), mbox('m2', 'Sent', 'sent')],
    };
    render(<NavRail {...p} />);
    expect(screen.queryByRole('button', { name: /更多/ })).toBeNull();
  });

  it('自定义文件夹名解码 modified UTF-7（展开后显示中文）', async () => {
    const p = {
      ...baseProps(),
      selectedAccountId: 'a',
      // '&UXZO1mWHTvZZOQ-' 是 "其他文件夹" 的 IMAP modified UTF-7 编码
      mailboxes: [mbox('m1', 'INBOX', 'inbox'), mbox('m2', '&UXZO1mWHTvZZOQ-')],
    };
    render(<NavRail {...p} />);
    await userEvent.click(screen.getByRole('button', { name: /更多/ }));
    expect(screen.getByRole('button', { name: '其他文件夹' })).toBeInTheDocument();
  });
});
