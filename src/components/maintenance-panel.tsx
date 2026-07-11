import { useState } from 'react';

import * as tauri from '../lib/tauri';
import type { AndroidUpdateInfo, MacosUpdateInfo } from '../lib/types';
import { detectUpdatePlatform, type UpdatePlatform } from '../lib/updater';

interface MaintenancePanelProps {
  updatePlatform?: UpdatePlatform;
}

type AvailableUpdate =
  | { platform: 'android'; info: AndroidUpdateInfo }
  | { platform: 'macos'; info: MacosUpdateInfo };

export function MaintenancePanel({
  updatePlatform = detectUpdatePlatform(),
}: MaintenancePanelProps) {
  const [cacheBusy, setCacheBusy] = useState(false);
  const [cacheMessage, setCacheMessage] = useState<string | null>(null);
  const [cacheError, setCacheError] = useState<string | null>(null);
  const [updatePhase, setUpdatePhase] = useState<'idle' | 'checking' | 'applying'>('idle');
  const [updateMessage, setUpdateMessage] = useState<string | null>(null);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const [availableUpdate, setAvailableUpdate] = useState<AvailableUpdate | null>(null);
  const updateBusy = updatePhase !== 'idle';

  async function clearCache() {
    setCacheBusy(true);
    setCacheMessage(null);
    setCacheError(null);
    try {
      const report = await tauri.cacheClear();
      setCacheMessage(
        `已清理 ${String(report.messageBodiesDeleted)} 条正文缓存、${String(
          report.aiResultsDeleted,
        )} 条 AI 缓存。`,
      );
    } catch (e) {
      setCacheError(e instanceof Error ? e.message : '清理本地缓存失败');
    } finally {
      setCacheBusy(false);
    }
  }

  async function checkUpdate() {
    if (updatePlatform === 'unsupported') {
      return;
    }

    setUpdatePhase('checking');
    setUpdateMessage(null);
    setUpdateError(null);
    setAvailableUpdate(null);

    try {
      if (updatePlatform === 'android') {
        const update = await tauri.androidUpdateCheck();
        if (!update) {
          setUpdateMessage('已是最新版本。');
          return;
        }
        setAvailableUpdate({ platform: 'android', info: update });
        return;
      }

      const update = await tauri.macosUpdateCheck();
      if (!update) {
        setUpdateMessage('已是最新版本。');
        return;
      }
      setAvailableUpdate({ platform: 'macos', info: update });
    } catch (e) {
      setUpdateError(e instanceof Error ? e.message : '检查软件更新失败');
    } finally {
      setUpdatePhase('idle');
    }
  }

  async function runUpdate() {
    if (!availableUpdate) {
      return;
    }

    setUpdatePhase('applying');
    setUpdateMessage(null);
    setUpdateError(null);

    try {
      if (availableUpdate.platform === 'android') {
        await tauri.androidUpdateOpenDownload(availableUpdate.info.apkUrl);
        setUpdateMessage('已打开下载链接。');
        return;
      }

      await tauri.macosUpdateOpenDownload(availableUpdate.info.dmgUrl);
      setUpdateMessage('已打开 DMG 下载链接，请将新应用拖入“应用程序”覆盖旧版本。');
    } catch (e) {
      setUpdateError(e instanceof Error ? e.message : '执行软件更新失败');
    } finally {
      setUpdatePhase('idle');
    }
  }

  return (
    <div className="space-y-4">
      <section className="space-y-3">
        <h3 className="text-sm font-semibold text-slate-900 dark:text-slate-100">软件更新</h3>
        <button
          type="button"
          onClick={() => void checkUpdate()}
          disabled={updateBusy || updatePlatform === 'unsupported'}
          className="rounded border border-slate-300 px-3 py-2 text-sm text-slate-700 hover:bg-slate-100 disabled:cursor-not-allowed disabled:opacity-60 dark:border-slate-700 dark:text-slate-100 dark:hover:bg-slate-800"
        >
          {updatePhase === 'checking' ? '正在检查…' : '检查软件更新'}
        </button>
        {updatePlatform === 'unsupported' && (
          <div className="text-sm text-slate-600 dark:text-slate-300">
            当前平台暂不支持应用内更新检查。
          </div>
        )}
        {availableUpdate && (
          <div className="space-y-2 text-sm text-slate-700 dark:text-slate-200">
            <div>{`发现新版本 ${availableUpdate.info.version}`}</div>
            {availableUpdate.platform === 'macos' && <div>{availableUpdate.info.notes}</div>}
            <button
              type="button"
              onClick={() => void runUpdate()}
              disabled={updateBusy}
              className="rounded border border-slate-300 px-3 py-2 text-sm text-slate-700 hover:bg-slate-100 disabled:cursor-not-allowed disabled:opacity-60 dark:border-slate-700 dark:text-slate-100 dark:hover:bg-slate-800"
            >
              {availableUpdate.platform === 'android' ? '下载 Android 更新' : '下载 macOS 更新'}
            </button>
          </div>
        )}
        {updateMessage && (
          <div className="text-sm text-emerald-700 dark:text-emerald-300">{updateMessage}</div>
        )}
        {updateError && <div className="text-sm text-red-600 dark:text-red-300">{updateError}</div>}
      </section>
      <section className="space-y-3">
        <h3 className="text-sm font-semibold text-slate-900 dark:text-slate-100">本地缓存</h3>
        <button
          type="button"
          onClick={() => void clearCache()}
          disabled={cacheBusy}
          className="rounded border border-slate-300 px-3 py-2 text-sm text-slate-700 hover:bg-slate-100 disabled:cursor-not-allowed disabled:opacity-60 dark:border-slate-700 dark:text-slate-100 dark:hover:bg-slate-800"
        >
          {cacheBusy ? '正在清理…' : '清理本地缓存'}
        </button>
        {cacheMessage && (
          <div className="text-sm text-emerald-700 dark:text-emerald-300">{cacheMessage}</div>
        )}
        {cacheError && <div className="text-sm text-red-600 dark:text-red-300">{cacheError}</div>}
      </section>
    </div>
  );
}
