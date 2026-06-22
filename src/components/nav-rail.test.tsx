import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { NavRail } from './nav-rail';
import type { Account } from '../lib/types';

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

function baseProps() {
  return {
    accounts: [acct('a', 'amy@qq.com'), acct('b', 'bob@qq.com')],
    selectedAccountId: 'a',
    syncing: false,
    onSelectAccount: vi.fn(),
    onAddAccount: vi.fn(),
    onSync: vi.fn(),
    onRemoveAccount: vi.fn(),
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

  it('fires remove on right-click of an account', () => {
    const p = baseProps();
    render(<NavRail {...p} />);
    fireEvent.contextMenu(screen.getByRole('button', { name: /amy@qq.com/ }));
    expect(p.onRemoveAccount).toHaveBeenCalledWith('a');
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
});
