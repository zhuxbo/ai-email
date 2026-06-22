// Tests for #47: AddAccountDialog resets all fields (including sensitive auth code)
// on close/cancel so state does not persist across open cycles.

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { AddAccountDialog } from './add-account-dialog';
import { useMailStore } from '../lib/store/mail';

function inputValue(el: HTMLElement): string {
  return (el as HTMLInputElement).value;
}

vi.mock('../lib/tauri', () => ({
  accountAdd: vi.fn().mockResolvedValue({
    id: 'acc-1',
    email: 'test@qq.com',
    provider: 'qq',
    imapHost: 'imap.qq.com',
    imapPort: 993,
    smtpHost: 'smtp.qq.com',
    smtpPort: 465,
    createdAt: '2026-06-23T00:00:00Z',
    lastSyncedAt: null,
  }),
  syncInbox: vi.fn().mockResolvedValue({ newMessageCount: 0 }),
}));

beforeEach(() => {
  useMailStore.setState({ accounts: [] } as never);
  vi.clearAllMocks();
});

describe('AddAccountDialog', () => {
  it('open=false 时不渲染对话框', () => {
    render(<AddAccountDialog open={false} onClose={vi.fn()} />);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('open=true 时渲染表单', () => {
    render(<AddAccountDialog open onClose={vi.fn()} />);
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });

  // ---- #47: 取消按钮关闭后字段清空 ----

  it('取消后授权码字段清空', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();

    const { rerender } = render(<AddAccountDialog open onClose={onClose} />);

    // 填入授权码
    const authInput = screen.getByPlaceholderText(/授权码/i);
    await user.type(authInput, 'supersecretcode123');
    expect(inputValue(authInput)).toBe('supersecretcode123');

    // 点取消 → onClose 被调用
    await user.click(screen.getByRole('button', { name: '取消' }));
    expect(onClose).toHaveBeenCalled();

    // 重新打开对话框（模拟再次打开）
    rerender(<AddAccountDialog open onClose={onClose} />);

    // 授权码应为空（状态已清空）
    const authInputAfter = screen.getByPlaceholderText(/授权码/i);
    expect(inputValue(authInputAfter)).toBe('');
  });

  it('取消后邮箱与显示名字段清空', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();

    const { rerender } = render(<AddAccountDialog open onClose={onClose} />);

    const emailInput = screen.getByPlaceholderText('you@qq.com');
    await user.type(emailInput, 'alice@qq.com');

    await user.click(screen.getByRole('button', { name: '取消' }));

    rerender(<AddAccountDialog open onClose={onClose} />);

    const emailInputAfter = screen.getByPlaceholderText('you@qq.com');
    expect(inputValue(emailInputAfter)).toBe('');
  });

  it('点遮罩层关闭后授权码清空', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();

    const { rerender } = render(<AddAccountDialog open onClose={onClose} />);

    const authInput = screen.getByPlaceholderText(/授权码/i);
    await user.type(authInput, 'mysecret');

    // 点击遮罩层（dialog 容器本身）触发 onClose
    const backdrop = screen.getByRole('dialog');
    await user.click(backdrop);
    expect(onClose).toHaveBeenCalled();

    rerender(<AddAccountDialog open onClose={onClose} />);

    expect(inputValue(screen.getByPlaceholderText(/授权码/i))).toBe('');
  });

  it('提交成功后授权码清空', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();

    const { rerender } = render(<AddAccountDialog open onClose={onClose} />);

    await user.type(screen.getByPlaceholderText('you@qq.com'), 'test@qq.com');
    await user.type(screen.getByPlaceholderText(/授权码/i), 'validcode16chars');

    await user.click(screen.getByRole('button', { name: '添加并同步' }));

    // 提交完成后重新打开，授权码应为空
    rerender(<AddAccountDialog open onClose={onClose} />);

    expect(inputValue(screen.getByPlaceholderText(/授权码/i))).toBe('');
  });
});
