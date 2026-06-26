import { describe, it, expect } from 'vitest';
import { decodeModifiedUtf7, errMsg, formatDateTimeCN } from './utils';

describe('errMsg', () => {
  it('string 原样', () => {
    expect(errMsg('boom')).toBe('boom');
  });
  it('Error 取 message', () => {
    expect(errMsg(new Error('nope'))).toBe('nope');
  });
});

describe('decodeModifiedUtf7', () => {
  it('解码纯中文文件夹名', () => {
    expect(decodeModifiedUtf7('&UXZO1mWHTvZZOQ-')).toBe('其他文件夹');
  });

  it('解码带 ASCII 子路径', () => {
    expect(decodeModifiedUtf7('&UXZO1mWHTvZZOQ-/Sectigo')).toBe('其他文件夹/Sectigo');
  });

  it('解码多段中文路径', () => {
    expect(decodeModifiedUtf7('&UXZO1mWHTvZZOQ-/&jURlmQ-')).toBe('其他文件夹/资料');
  });

  it('纯 ASCII 名原样返回', () => {
    expect(decodeModifiedUtf7('INBOX')).toBe('INBOX');
    expect(decodeModifiedUtf7('Sent Messages')).toBe('Sent Messages');
  });

  it('&- 表示字面 &', () => {
    expect(decodeModifiedUtf7('A &- B')).toBe('A & B');
  });

  it('空串返回空串', () => {
    expect(decodeModifiedUtf7('')).toBe('');
  });
});

describe('formatDateTimeCN', () => {
  // 无时区偏移的串按本地时区解析，读取也用本地时区 → 断言与运行时区无关。
  it('格式化为中文年月日时分', () => {
    expect(formatDateTimeCN('2026-06-26T15:49:00')).toBe('2026年6月26日 15:49');
  });

  it('月日不补零、时分补零', () => {
    expect(formatDateTimeCN('2026-01-05T09:05:00')).toBe('2026年1月5日 09:05');
  });

  it('null 返回空串', () => {
    expect(formatDateTimeCN(null)).toBe('');
  });

  it('非法日期返回空串', () => {
    expect(formatDateTimeCN('not-a-date')).toBe('');
  });

  it('带时区偏移的 UTC（Z）串也能格式化', () => {
    // 实际邮件 sentAt 多为带偏移串。按本地时区显示，故只断言结构与年月（日依时区落 25-27）。
    expect(formatDateTimeCN('2026-06-26T07:49:00Z')).toMatch(/^2026年6月2[567]日 \d{2}:\d{2}$/);
  });
});
