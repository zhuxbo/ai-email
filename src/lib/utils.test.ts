import { describe, it, expect } from 'vitest';
import { decodeModifiedUtf7, errMsg } from './utils';

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
