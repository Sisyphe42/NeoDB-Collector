import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { CoreClientError, type Snapshot } from '@neodb-collector/core-client';
import { App } from './App.js';
afterEach(cleanup);
const snapshot: Snapshot = { health: { status: 'ok', storage: 'ready', coreVersion: '0.1.0', apiVersion: 'v1', schemaVersion: 1 }, capabilities: { schemaVersion: 1, connectorIds: [], mediaTypes: [], routes: {} } };
describe('local status console', () => {
  it('starts loading, displays real status and refreshes', async () => {
    const load = vi.fn().mockResolvedValue(snapshot);
    render(<App loadStatus={load} />);
    expect(screen.getByText('正在连接本地服务')).toBeTruthy();
    expect(await screen.findByText('本地服务已连接')).toBeTruthy();
    expect(screen.getByText('尚无可用的平台连接')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '重新检查' }));
    await waitFor(() => expect(load).toHaveBeenCalledTimes(2));
    expect(await screen.findByText('本地服务已连接')).toBeTruthy();
  });
  it.each([['unavailable', '无法连接本地服务'], ['incompatible', '客户端与服务版本不兼容'], ['invalid_response', '服务响应格式异常']] as const)('renders %s', async (kind, label) => {
    render(<App loadStatus={async () => { throw new CoreClientError(kind, 'test'); }} />);
    expect(await screen.findByText(label)).toBeTruthy();
    expect(screen.queryByText('尚无可用的平台连接')).toBeNull();
  });
  it('renders degraded database health without claiming ready', async () => {
    render(<App loadStatus={async () => ({ ...snapshot, health: { ...snapshot.health, status: 'degraded', storage: 'unavailable' } })} />);
    expect(await screen.findByText('本地数据库不可用')).toBeTruthy();
  });
});
