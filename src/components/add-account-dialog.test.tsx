// Tests for:
//   #47: AddAccountDialog resets all fields (including sensitive auth code)
//        on close/cancel so state does not persist across open cycles.
//   #69: Port inputs must parse as integers in 1–65535; decimals/0/out-of-range are rejected.

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { AddAccountDialog, parsePort } from './add-account-dialog';
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

  // ---- #52: submitting 时点遮罩不打断提交 ----

  it('提交进行中点遮罩不触发 onClose', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();

    // Grab the already-mocked accountAdd and make it hang temporarily.
    const tauriMock = await import('../lib/tauri');
    const accountAddMock = tauriMock.accountAdd as ReturnType<typeof vi.fn>;

    let resolveSubmit!: () => void;
    // Use a separate hanging promise to simulate an in-flight submit. The cast is needed
    // because mockImplementationOnce's generic resolver types don't align with the real
    // return type — this is a test-only pattern; production code never does this.
    const hangingSubmit = new Promise<void>((resolve) => {
      resolveSubmit = resolve;
    });
    accountAddMock.mockImplementationOnce(() => hangingSubmit as never);

    render(<AddAccountDialog open onClose={onClose} />);

    await user.type(screen.getByPlaceholderText('you@qq.com'), 'test@qq.com');
    await user.type(screen.getByPlaceholderText(/授权码/i), 'validcode16chars');

    // Start submit (don't await — it will hang until resolveSubmit is called).
    const submitBtn = screen.getByRole('button', { name: '添加并同步' });
    void user.click(submitBtn);

    // Give the submit handler a tick to set submitting=true.
    await new Promise((r) => setTimeout(r, 0));

    // Click the backdrop while submitting.
    await user.click(screen.getByRole('dialog'));

    // onClose must NOT have been called.
    expect(onClose).not.toHaveBeenCalled();

    // Unblock the submit so component cleanup is deterministic.
    resolveSubmit();
  });

  // ---- #69: 端口字段整数校验 ----
  // parsePort 是从组件导出的纯函数，对照后端 validate_port(i32) 的契约测试。

  it('#69 parsePort: 有效整数端口返回数字', () => {
    expect(parsePort('993')).toBe(993);
    expect(parsePort('465')).toBe(465);
    expect(parsePort('1')).toBe(1);
    expect(parsePort('65535')).toBe(65535);
  });

  it('#69 parsePort: 0 返回 null', () => {
    expect(parsePort('0')).toBeNull();
  });

  it('#69 parsePort: 负数返回 null', () => {
    expect(parsePort('-1')).toBeNull();
    expect(parsePort('-993')).toBeNull();
  });

  it('#69 parsePort: 超过 65535 返回 null', () => {
    expect(parsePort('65536')).toBeNull();
    expect(parsePort('99999')).toBeNull();
  });

  it('#69 parsePort: 小数返回 null（拒绝非整数）', () => {
    expect(parsePort('993.5')).toBeNull();
    expect(parsePort('0.5')).toBeNull();
    expect(parsePort('1.0')).toBe(1); // 1.0 截断为 1，整数有效
  });

  it('#69 parsePort: 空字符串返回 null', () => {
    expect(parsePort('')).toBeNull();
  });

  it('#69 parsePort: 非数字字符串返回 null', () => {
    expect(parsePort('abc')).toBeNull();
    expect(parsePort('993abc')).toBeNull();
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
