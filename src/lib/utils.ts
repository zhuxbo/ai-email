// Shared front-end helpers. errMsg 把 catch 到的 unknown 错误规整成可展示字符串：
// string 原样返回、Error 取 message、其余 JSON 序列化。各 store 的 error 字段统一用它。

export function errMsg(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return JSON.stringify(e);
}
