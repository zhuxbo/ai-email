import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';

import { MaintenancePanel } from './maintenance-panel';
import * as tauri from '../lib/tauri';

vi.mock('../lib/updater', () => ({
  detectUpdatePlatform: vi.fn(() => 'unsupported'),
}));

vi.mock('../lib/tauri', async (importOriginal) => {
  const actual = await importOriginal<typeof tauri>();
  return {
    ...actual,
    cacheClear: vi.fn().mockResolvedValue({
      messageBodiesDeleted: 0,
      aiResultsDeleted: 0,
    }),
    androidUpdateCheck: vi.fn().mockResolvedValue(null),
    androidUpdateOpenDownload: vi.fn().mockResolvedValue(undefined),
    macosUpdateCheck: vi.fn().mockResolvedValue(null),
    macosUpdateOpenDownload: vi.fn().mockResolvedValue(undefined),
  };
});

describe('MaintenancePanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  function createDeferred<T>() {
    let resolve!: (value: T | PromiseLike<T>) => void;
    let reject!: (reason?: unknown) => void;
    const promise = new Promise<T>((res, rej) => {
      resolve = res;
      reject = rej;
    });
    return { promise, resolve, reject };
  }

  it('点击清理按钮调用 cacheClear 并显示清理数量', async () => {
    vi.mocked(tauri.cacheClear).mockResolvedValueOnce({
      messageBodiesDeleted: 3,
      aiResultsDeleted: 2,
    });

    render(<MaintenancePanel />);
    fireEvent.click(screen.getByRole('button', { name: '清理本地缓存' }));

    await waitFor(() => {
      expect(tauri.cacheClear).toHaveBeenCalledOnce();
    });
    expect(await screen.findByText('已清理 3 条正文缓存、2 条 AI 缓存。')).toBeInTheDocument();
  });

  it('清理失败时显示错误且允许重试', async () => {
    vi.mocked(tauri.cacheClear).mockRejectedValueOnce(new Error('数据库被占用'));

    render(<MaintenancePanel />);
    const button = screen.getByRole('button', { name: '清理本地缓存' });
    fireEvent.click(button);

    expect(await screen.findByText('数据库被占用')).toBeInTheDocument();
    expect(button).not.toBeDisabled();
  });

  it('无更新时显示已是最新版本', async () => {
    render(<MaintenancePanel updatePlatform="android" />);

    fireEvent.click(screen.getByRole('button', { name: '检查软件更新' }));

    expect(await screen.findByText('已是最新版本。')).toBeInTheDocument();
  });

  it('unsupported 平台显示不可检查文案且不调用更新命令', () => {
    render(<MaintenancePanel updatePlatform="unsupported" />);

    expect(screen.getByText('当前平台暂不支持应用内更新检查。')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '检查软件更新' })).toBeDisabled();
    expect(tauri.androidUpdateCheck).not.toHaveBeenCalled();
    expect(tauri.macosUpdateCheck).not.toHaveBeenCalled();
  });

  it('Android 有更新时展示版本并调用下载入口', async () => {
    vi.mocked(tauri.androidUpdateCheck).mockResolvedValueOnce({
      version: '0.2.0',
      versionCode: 2000,
      notes: '修复更新',
      pubDate: '2026-07-10T00:00:00Z',
      apkUrl:
        'https://github.com/zhuxbo/ai-email/releases/download/v0.2.0/ai-email_0.2.0_arm64-v8a.apk',
      apkSize: 123456,
      sha256: 'abc',
    });

    render(<MaintenancePanel updatePlatform="android" />);
    fireEvent.click(screen.getByRole('button', { name: '检查软件更新' }));

    expect(await screen.findByText(/发现新版本 0.2.0/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '下载 Android 更新' }));

    await waitFor(() => {
      expect(tauri.androidUpdateOpenDownload).toHaveBeenCalledOnce();
    });
  });

  it('Android 检查失败时显示错误', async () => {
    vi.mocked(tauri.androidUpdateCheck).mockRejectedValueOnce(new Error('网络异常'));

    render(<MaintenancePanel updatePlatform="android" />);
    fireEvent.click(screen.getByRole('button', { name: '检查软件更新' }));

    expect(await screen.findByText('网络异常')).toBeInTheDocument();
  });

  it('macOS 有更新时展示版本和说明，并打开 DMG 下载链接', async () => {
    const dmgUrl =
      'https://github.com/zhuxbo/ai-email/releases/download/v0.2.0/ai-email_0.2.0_aarch64.dmg';
    vi.mocked(tauri.macosUpdateCheck).mockResolvedValueOnce({
      version: '0.2.0',
      notes: '修复更新',
      pubDate: '2026-07-10T00:00:00Z',
      dmgUrl,
    });

    render(<MaintenancePanel updatePlatform="macos" />);
    fireEvent.click(screen.getByRole('button', { name: '检查软件更新' }));

    expect(await screen.findByText(/发现新版本 0.2.0/)).toBeInTheDocument();
    expect(screen.getByText('修复更新')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '下载 macOS 更新' }));

    await waitFor(() => {
      expect(tauri.macosUpdateOpenDownload).toHaveBeenCalledOnce();
    });
    expect(tauri.macosUpdateOpenDownload).toHaveBeenCalledWith(dmgUrl);
    expect(
      await screen.findByText('已打开 DMG 下载链接，请将新应用拖入“应用程序”覆盖旧版本。'),
    ).toBeInTheDocument();
  });

  it('macOS 打开 DMG 下载链接失败时显示错误', async () => {
    vi.mocked(tauri.macosUpdateCheck).mockResolvedValueOnce({
      version: '0.2.0',
      notes: '修复更新',
      pubDate: '2026-07-10T00:00:00Z',
      dmgUrl:
        'https://github.com/zhuxbo/ai-email/releases/download/v0.2.0/ai-email_0.2.0_aarch64.dmg',
    });
    vi.mocked(tauri.macosUpdateOpenDownload).mockRejectedValueOnce(new Error('下载链接无法打开'));

    render(<MaintenancePanel updatePlatform="macos" />);
    fireEvent.click(screen.getByRole('button', { name: '检查软件更新' }));
    expect(await screen.findByText(/发现新版本 0.2.0/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '下载 macOS 更新' }));

    expect(await screen.findByText('下载链接无法打开')).toBeInTheDocument();
  });

  it('下载阶段检查按钮不显示正在检查', async () => {
    const deferred = createDeferred<undefined>();
    vi.mocked(tauri.androidUpdateCheck).mockResolvedValueOnce({
      version: '0.2.0',
      versionCode: 2000,
      notes: '修复更新',
      pubDate: '2026-07-10T00:00:00Z',
      apkUrl:
        'https://github.com/zhuxbo/ai-email/releases/download/v0.2.0/ai-email_0.2.0_arm64-v8a.apk',
      apkSize: 123456,
      sha256: 'abc',
    });
    vi.mocked(tauri.androidUpdateOpenDownload).mockReturnValueOnce(deferred.promise);

    render(<MaintenancePanel updatePlatform="android" />);
    fireEvent.click(screen.getByRole('button', { name: '检查软件更新' }));
    expect(await screen.findByText(/发现新版本 0.2.0/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '下载 Android 更新' }));

    expect(screen.getByRole('button', { name: '检查软件更新' })).toBeDisabled();
    expect(screen.queryByRole('button', { name: '正在检查…' })).not.toBeInTheDocument();

    deferred.resolve(undefined);
    await waitFor(() => {
      expect(tauri.androidUpdateOpenDownload).toHaveBeenCalledOnce();
    });
  });

  it('缓存清理结果与更新检查结果互不覆盖', async () => {
    vi.mocked(tauri.cacheClear).mockResolvedValueOnce({
      messageBodiesDeleted: 1,
      aiResultsDeleted: 2,
    });
    vi.mocked(tauri.androidUpdateCheck).mockRejectedValueOnce(new Error('更新服务不可用'));

    render(<MaintenancePanel updatePlatform="android" />);

    fireEvent.click(screen.getByRole('button', { name: '清理本地缓存' }));
    expect(await screen.findByText('已清理 1 条正文缓存、2 条 AI 缓存。')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '检查软件更新' }));
    expect(await screen.findByText('更新服务不可用')).toBeInTheDocument();
    expect(screen.getByText('已清理 1 条正文缓存、2 条 AI 缓存。')).toBeInTheDocument();
  });
});
