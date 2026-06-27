import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

import { SettingsDialog } from './settings-dialog';

// 面板内容各自有 store 依赖；这里只验设置中心的 tab/开关逻辑，故把两个 Panel 打桩。
vi.mock('./account-settings-dialog', () => ({
  AccountsPanel: () => <div data-testid="accounts-panel" />,
}));
vi.mock('./ai-settings-dialog', () => ({
  AiModelsPanel: () => <div data-testid="ai-panel" />,
}));
vi.mock('./auto-sync-panel', () => ({
  AutoSyncPanel: () => <div data-testid="auto-sync-panel" />,
}));

describe('SettingsDialog', () => {
  it('open=false 时不渲染', () => {
    render(<SettingsDialog open={false} onClose={vi.fn()} />);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('open=true 默认显示「账户」tab', () => {
    render(<SettingsDialog open onClose={vi.fn()} />);
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByTestId('accounts-panel')).toBeInTheDocument();
    expect(screen.queryByTestId('ai-panel')).toBeNull();
    expect(screen.getByRole('button', { name: '账户' })).toHaveAttribute('aria-pressed', 'true');
  });

  it('点「AI 模型」tab 切换面板', () => {
    render(<SettingsDialog open onClose={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'AI 模型' }));
    expect(screen.getByTestId('ai-panel')).toBeInTheDocument();
    expect(screen.queryByTestId('accounts-panel')).toBeNull();
    expect(screen.getByRole('button', { name: 'AI 模型' })).toHaveAttribute('aria-pressed', 'true');
  });

  it('点「收信」tab 渲染 AutoSyncPanel', () => {
    render(<SettingsDialog open onClose={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: '收信' }));
    expect(screen.getByTestId('auto-sync-panel')).toBeInTheDocument();
    expect(screen.queryByTestId('accounts-panel')).toBeNull();
    expect(screen.getByRole('button', { name: '收信' })).toHaveAttribute('aria-pressed', 'true');
  });

  it('点关闭按钮触发 onClose', () => {
    const onClose = vi.fn();
    render(<SettingsDialog open onClose={onClose} />);
    fireEvent.click(screen.getByRole('button', { name: '关闭' }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('点遮罩关闭、点内容区不关闭（stopPropagation）', () => {
    const onClose = vi.fn();
    render(<SettingsDialog open onClose={onClose} />);
    fireEvent.click(screen.getByRole('heading', { name: '设置中心' }));
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('dialog'));
    expect(onClose).toHaveBeenCalledOnce();
  });
});
