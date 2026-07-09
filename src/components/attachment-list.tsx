// 附件懒加载列表组件。hasAttachment 为真时才拉取元信息（active 守卫防竞态）。
// 下载走后端原生保存对话框 → messageAttachmentSave，失败 surface 到全局错误条。

import { useEffect, useState } from 'react';

import * as tauri from '../lib/tauri';
import { useMailStore } from '../lib/store/mail';
import type { AttachmentMeta } from '../lib/types';

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${String(bytes)} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

interface Props {
  messageId: string;
  hasAttachment: boolean;
}

export function AttachmentList({ messageId, hasAttachment }: Props) {
  const [attachments, setAttachments] = useState<AttachmentMeta[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!hasAttachment) {
      setAttachments([]);
      setLoading(false);
      return;
    }
    let active = true;
    setLoading(true);
    void tauri
      .messageAttachments(messageId)
      .then((atts) => {
        if (active) {
          setAttachments(atts);
          setLoading(false);
        }
      })
      .catch(() => {
        if (active) {
          setAttachments([]);
          setLoading(false);
        }
      });
    return () => {
      active = false;
    };
  }, [messageId, hasAttachment]);

  if (!hasAttachment) return null;

  async function downloadAttachment(index: number) {
    try {
      await tauri.messageAttachmentSave(messageId, index);
    } catch (e) {
      useMailStore.setState({ error: e instanceof Error ? e.message : '下载附件失败' });
    }
  }

  return (
    <div className="rounded border border-slate-200 bg-white p-3 dark:border-slate-700 dark:bg-slate-900">
      <div className="mb-2 text-xs font-medium text-slate-600 dark:text-slate-300">
        附件{attachments.length > 0 ? ` (${String(attachments.length)})` : ''}
      </div>
      {loading ? (
        <div className="text-xs text-slate-500 dark:text-slate-400">正在读取附件…</div>
      ) : attachments.length === 0 ? (
        <div className="text-xs text-slate-500 dark:text-slate-400">没有可下载的附件。</div>
      ) : (
        <ul className="flex flex-wrap gap-2">
          {attachments.map((a, i) => (
            <li key={`${a.filename}-${String(i)}`}>
              <button
                type="button"
                onClick={() => void downloadAttachment(i)}
                className="flex items-center gap-1 rounded border border-slate-200 px-2 py-1 text-xs text-slate-700 hover:bg-slate-100 dark:border-slate-700 dark:text-slate-200 dark:hover:bg-slate-800"
                title="下载附件"
                aria-label={`${a.filename} · ${formatSize(a.size)}`}
              >
                📎 {a.filename} · {formatSize(a.size)}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
