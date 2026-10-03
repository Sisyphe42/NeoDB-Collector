import { StrictMode } from 'react';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { AccessClientError, type AccessClient } from '@neodb-collector/core-client';
import { AccessPanel } from './AccessPanel.js';
import { startSession } from './session.js';

afterEach(cleanup);
function client(): AccessClient {
  return { session: vi.fn().mockResolvedValue({ csrfToken: 'csrf', expiresAt: 10000 }),
    neodbStatus: vi.fn().mockResolvedValue({ connected: false, baseUrl: null, account: null, pending: false }),
    startNeoDb: vi.fn(), cancelNeoDb: vi.fn(),
    mappingProfile: vi.fn(), validateMapping: vi.fn(), saveMapping: vi.fn(),
    previewJob: vi.fn(), previewResults: vi.fn(),
    cancelPreview: vi.fn(), deletePreview: vi.fn(),
    submitImdbRatings: vi.fn(), submitImdbRatingsWithResolution: vi.fn(), resumePreview: vi.fn(),
    defaultMapping: vi.fn(), convertMapping: vi.fn(),
    logout: vi.fn().mockResolvedValue(undefined), clients: vi.fn().mockResolvedValue([]),
    createPairing: vi.fn().mockResolvedValue({ uri: 'http://127.0.0.1:19423/pair#key=example', expiresAt: Date.now() / 1000 + 600 }),
    revoke: vi.fn().mockResolvedValue(undefined) };
}
it('clears the fragment before one-time exchange and survives StrictMode', async () => {
  const api = client();
  const history = { replaceState: vi.fn() };
  const promise = startSession(api, { hash: '#key=one-use', pathname: '/', search: '' }, history);
  expect(history.replaceState).toHaveBeenCalledWith(null, '', '/');
  render(<StrictMode><AccessPanel client={api} initialSession={promise} /></StrictMode>);
  expect(await screen.findByText('已授权')).toBeTruthy();
  expect(api.session).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByRole('button', { name: '生成配对链接' }));
  expect(await screen.findByDisplayValue('http://127.0.0.1:19423/pair#key=example')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: '隐藏链接' }));
  expect(screen.queryByDisplayValue('http://127.0.0.1:19423/pair#key=example')).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: '退出本地授权' }));
  await waitFor(() => expect(screen.getByText('未授权')).toBeTruthy());
});
it('guides expired sessions without showing credential details', async () => {
  const api = client();
  vi.mocked(api.session).mockRejectedValue(new AccessClientError('challenge_invalid'));
  render(<AccessPanel client={api} initialSession={startSession(api, { hash: '', pathname: '/', search: '' }, { replaceState: vi.fn() })} />);
  expect(await screen.findByRole('alert')).toBeTruthy();
  expect(screen.queryByRole('button', { name: '生成配对链接' })).toBeNull();
});
it('removes the fragment but never exchanges it with an incompatible Core', async () => {
  const api = client();
  const history = { replaceState: vi.fn() };
  await expect(startSession(api, { hash: '#key=one-use', pathname: '/', search: '' }, history, async () => {
    expect(history.replaceState).toHaveBeenCalled();
    throw new Error('incompatible');
  })).rejects.toThrow('incompatible');
  expect(history.replaceState).toHaveBeenCalled();
  expect(api.session).not.toHaveBeenCalled();
});
