// Shared front-end helpers. errMsg 把 catch 到的 unknown 错误规整成可展示字符串：
// string 原样返回、Error 取 message、其余 JSON 序列化。各 store 的 error 字段统一用它。

export function errMsg(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return JSON.stringify(e);
}

// 统一的中文日期时间格式：2026年6月26日 15:49（本地时区）。空/非法返回空串。
// 列表、会话流、详情头部统一用它，消除此前 ISO 串 / toLocaleString / 相对时间三种并存。
export function formatDateTimeCN(iso: string | null): string {
  if (!iso) return '';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  const hh = String(d.getHours()).padStart(2, '0');
  const mi = String(d.getMinutes()).padStart(2, '0');
  return `${String(d.getFullYear())}年${String(d.getMonth() + 1)}月${String(d.getDate())}日 ${hh}:${mi}`;
}

// IMAP modified UTF-7（RFC 3501 §5.1.3）解码。IMAP 文件夹名用它编码非 ASCII：ASCII 原样，
// `&...-` 段是 modified base64（用 , 代替 /）编码的 UTF-16BE，`&-` 表示字面 `&`。QQ 等服务器
// 据此编码中文文件夹名（"其他文件夹" → "&UXZO1mWHTvZZOQ-"）。后端存 raw（IMAP SELECT 需要原始
// 名），前端显示时解码。
export function decodeModifiedUtf7(name: string): string {
  let out = '';
  let i = 0;
  while (i < name.length) {
    const ch = name.charAt(i);
    if (ch === '&') {
      const end = name.indexOf('-', i);
      if (end === -1) {
        // 容错：缺结束符，剩余原样输出
        out += name.slice(i);
        break;
      }
      const seg = name.slice(i + 1, end);
      out += seg === '' ? '&' : decodeBase64Utf16Be(seg);
      i = end + 1;
    } else {
      out += ch;
      i += 1;
    }
  }
  return out;
}

function decodeBase64Utf16Be(seg: string): string {
  const b64 = seg.replace(/,/g, '/');
  const padded = b64 + '='.repeat((4 - (b64.length % 4)) % 4);
  const bin = atob(padded);
  let out = '';
  // 每 2 字节 = 一个 UTF-16BE code unit
  for (let k = 0; k + 1 < bin.length; k += 2) {
    out += String.fromCharCode((bin.charCodeAt(k) << 8) | bin.charCodeAt(k + 1));
  }
  return out;
}
