import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';

import { AiModelsPanel } from './ai-settings-dialog';
import { useAiStore } from '../lib/store/ai';

const mkModel = (over: Record<string, unknown> = {}) => ({
  id: 'm1',
  displayName: 'Claude',
  provider: 'anthropic',
  modelId: 'claude-opus-4-8',
  baseUrl: null,
  createdAt: '2026-06-25T00:00:00Z',
  ...over,
});

function setStore(over: Record<string, unknown>) {
  useAiStore.setState({
    models: [],
    roleDefaults: [],
    loadAiConfig: vi.fn().mockResolvedValue(undefined),
    updateModel: vi.fn().mockResolvedValue(undefined),
    removeModel: vi.fn().mockResolvedValue(undefined),
    addModel: vi.fn().mockResolvedValue(undefined),
    setRoleDefault: vi.fn().mockResolvedValue(undefined),
    clearRoleDefault: vi.fn().mockResolvedValue(undefined),
    ...over,
  } as never);
}

// AddModelSection 与 ModelEditForm 都有「显示名」label；用编辑表单的「保存」按钮锚定其所属 <form>，
// 再在该 form 范围内查字段，避免歧义。
function editForm(): HTMLElement {
  const form = screen.getByRole('button', { name: '保存' }).closest('form');
  if (!form) throw new Error('edit form not found');
  return form;
}

beforeEach(() => {
  setStore({});
});

describe('ModelEditForm 提交', () => {
  function openEdit() {
    fireEvent.click(screen.getByRole('button', { name: '编辑' }));
  }

  it('API key 留空→不带 apiKey 字段（留空＝保持原 key）', async () => {
    const updateModel = vi.fn().mockResolvedValue(undefined);
    setStore({ models: [mkModel()], updateModel });
    render(<AiModelsPanel />);
    openEdit();
    fireEvent.click(within(editForm()).getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(updateModel).toHaveBeenCalledTimes(1);
    });
    const call = updateModel.mock.calls[0] as [string, Record<string, unknown>];
    expect(call[0]).toBe('m1');
    expect(call[1].displayName).toBe('Claude');
    expect(call[1].modelId).toBe('claude-opus-4-8');
    expect(Object.prototype.hasOwnProperty.call(call[1], 'apiKey')).toBe(false);
  });

  it('显示名留空白→回落原显示名', async () => {
    const updateModel = vi.fn().mockResolvedValue(undefined);
    setStore({ models: [mkModel()], updateModel });
    render(<AiModelsPanel />);
    openEdit();
    fireEvent.change(within(editForm()).getByLabelText('显示名'), { target: { value: '   ' } });
    fireEvent.click(within(editForm()).getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(updateModel).toHaveBeenCalledTimes(1);
    });
    const call = updateModel.mock.calls[0] as [string, Record<string, unknown>];
    expect(call[1].displayName).toBe('Claude');
  });

  it('填写 API key→去空白后带上', async () => {
    const updateModel = vi.fn().mockResolvedValue(undefined);
    setStore({ models: [mkModel()], updateModel });
    render(<AiModelsPanel />);
    openEdit();
    fireEvent.change(within(editForm()).getByPlaceholderText('留空＝保持原 key 不变'), {
      target: { value: '  sk-new  ' },
    });
    fireEvent.click(within(editForm()).getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(updateModel).toHaveBeenCalledTimes(1);
    });
    const call = updateModel.mock.calls[0] as [string, Record<string, unknown>];
    expect(call[1].apiKey).toBe('sk-new');
  });
});
