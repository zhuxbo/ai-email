import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

import { AccountsPanel } from './account-settings-dialog';
import { useMailStore } from '../lib/store/mail';

const mkAccount = (over: Record<string, unknown> = {}) => ({
  id: 'a1',
  email: 'amy@qq.com',
  provider: 'qq',
  displayName: 'Amy',
  imapHost: 'imap.qq.com',
  imapPort: 993,
  smtpHost: 'smtp.qq.com',
  smtpPort: 465,
  createdAt: '2026-06-25T00:00:00Z',
  lastSyncedAt: null,
  ...over,
});

function setStore(over: Record<string, unknown>) {
  useMailStore.setState({
    accounts: [],
    updateAccount: vi.fn().mockResolvedValue(undefined),
    removeAccount: vi.fn().mockResolvedValue(undefined),
    ...over,
  } as never);
}

beforeEach(() => {
  setStore({});
});
afterEach(() => {
  vi.restoreAllMocks();
});

describe('AccountsPanel 列表', () => {
  it('无账户显示引导', () => {
    render(<AccountsPanel />);
    expect(screen.getByText(/还没有账户/)).toBeInTheDocument();
  });

  it('有账户渲染显示名 + 编辑/删除按钮', () => {
    setStore({ accounts: [mkAccount()] });
    render(<AccountsPanel />);
    expect(screen.getByText('Amy')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '编辑' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '删除' })).toBeInTheDocument();
  });
});

describe('AccountRow 删除二次确认', () => {
  it('confirm 通过才调用 removeAccount', () => {
    const removeAccount = vi.fn().mockResolvedValue(undefined);
    setStore({ accounts: [mkAccount()], removeAccount });
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    render(<AccountsPanel />);
    fireEvent.click(screen.getByRole('button', { name: '删除' }));
    expect(removeAccount).toHaveBeenCalledWith('a1');
  });

  it('confirm 取消则不删除', () => {
    const removeAccount = vi.fn().mockResolvedValue(undefined);
    setStore({ accounts: [mkAccount()], removeAccount });
    vi.spyOn(window, 'confirm').mockReturnValue(false);
    render(<AccountsPanel />);
    fireEvent.click(screen.getByRole('button', { name: '删除' }));
    expect(removeAccount).not.toHaveBeenCalled();
  });
});

describe('AccountEditForm 提交', () => {
  function openEdit() {
    fireEvent.click(screen.getByRole('button', { name: '编辑' }));
  }

  it('非法 IMAP 端口报错且不提交', async () => {
    const updateAccount = vi.fn().mockResolvedValue(undefined);
    setStore({ accounts: [mkAccount()], updateAccount });
    render(<AccountsPanel />);
    openEdit();
    fireEvent.change(screen.getByLabelText('IMAP port'), { target: { value: '0' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    expect(await screen.findByText(/IMAP 端口必须/)).toBeInTheDocument();
    expect(updateAccount).not.toHaveBeenCalled();
  });

  it('显示名留空→null，授权码留空→不带 authCode 字段', async () => {
    const updateAccount = vi.fn().mockResolvedValue(undefined);
    setStore({ accounts: [mkAccount()], updateAccount });
    render(<AccountsPanel />);
    openEdit();
    fireEvent.change(screen.getByLabelText('显示名'), { target: { value: '' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(updateAccount).toHaveBeenCalledTimes(1);
    });
    const call = updateAccount.mock.calls[0] as [string, Record<string, unknown>];
    expect(call[0]).toBe('a1');
    expect(call[1].displayName).toBeNull();
    expect(call[1].imapHost).toBe('imap.qq.com');
    expect(call[1].imapPort).toBe(993);
    expect(call[1].smtpPort).toBe(465);
    // 留空授权码必须完全不出现该字段（前端二次守卫，配合后端 secret_to_store）。
    expect(Object.prototype.hasOwnProperty.call(call[1], 'authCode')).toBe(false);
  });

  it('填写授权码→去空白后带上 authCode', async () => {
    const updateAccount = vi.fn().mockResolvedValue(undefined);
    setStore({ accounts: [mkAccount()], updateAccount });
    render(<AccountsPanel />);
    openEdit();
    fireEvent.change(screen.getByPlaceholderText('留空＝保持原授权码不变'), {
      target: { value: '  newcode  ' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(updateAccount).toHaveBeenCalledTimes(1);
    });
    const call = updateAccount.mock.calls[0] as [string, Record<string, unknown>];
    expect(call[1].authCode).toBe('newcode');
  });
});
