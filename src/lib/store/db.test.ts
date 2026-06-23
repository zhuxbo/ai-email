import { describe, it, expect, beforeEach } from 'vitest';
import { useDbStore } from './db';

beforeEach(() => {
  useDbStore.setState({ status: 'loading', errorMessage: null });
});

describe('db store 状态机', () => {
  it('初始状态为 loading', () => {
    expect(useDbStore.getState().status).toBe('loading');
    expect(useDbStore.getState().errorMessage).toBeNull();
  });

  it('loading → ready：收到 db://ready 后 setReady()', () => {
    useDbStore.getState().setReady();
    expect(useDbStore.getState().status).toBe('ready');
    expect(useDbStore.getState().errorMessage).toBeNull();
  });

  it('loading → error：收到 db://error 后 setError(message)', () => {
    useDbStore.getState().setError('数据库初始化超时（超过 30 秒）');
    expect(useDbStore.getState().status).toBe('error');
    expect(useDbStore.getState().errorMessage).toBe('数据库初始化超时（超过 30 秒）');
  });

  it('error 状态保留 errorMessage', () => {
    const msg = '磁盘空间不足，无法创建数据库文件';
    useDbStore.getState().setError(msg);
    expect(useDbStore.getState().errorMessage).toBe(msg);
  });

  // --- 状态机只前进（终态不可逆）---

  it('兜底查询：只收到 getDbStatus 返回 ready、无事件时也能进入 ready', () => {
    // 模拟 getDbStatus 返回 ready，直接调用 setReady（App.tsx 中的处理逻辑）
    useDbStore.getState().setReady();
    expect(useDbStore.getState().status).toBe('ready');
  });

  it('ready 后再次 setReady 幂等，不改变状态', () => {
    useDbStore.getState().setReady();
    useDbStore.getState().setReady();
    expect(useDbStore.getState().status).toBe('ready');
  });

  it('ready 后调用 setError 不得倒退', () => {
    useDbStore.getState().setReady();
    useDbStore.getState().setError('迟到的错误信号');
    // 状态机只前进——ready 是终态，不得被 error 覆盖
    expect(useDbStore.getState().status).toBe('ready');
    expect(useDbStore.getState().errorMessage).toBeNull();
  });

  it('error 后调用 setReady 不得倒退', () => {
    useDbStore.getState().setError('初始化失败');
    useDbStore.getState().setReady();
    // 状态机只前进——error 是终态，不得被 ready 覆盖
    expect(useDbStore.getState().status).toBe('error');
    expect(useDbStore.getState().errorMessage).toBe('初始化失败');
  });

  it('error 后再次 setError 幂等，保持原始错误消息', () => {
    useDbStore.getState().setError('磁盘满');
    useDbStore.getState().setError('迟到的第二个错误');
    expect(useDbStore.getState().status).toBe('error');
    // 第一个错误先到达并固定，后到的被忽略
    expect(useDbStore.getState().errorMessage).toBe('磁盘满');
  });
});
