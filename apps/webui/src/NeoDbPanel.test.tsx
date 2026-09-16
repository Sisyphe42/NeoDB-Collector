import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { createAccessClient } from '@neodb-collector/core-client';
import { NeoDbPanel } from './NeoDbPanel.js';
afterEach(cleanup);
it('loads actual status, starts read authorization and uses the returned destination', async () => {
  const requests: Request[] = [];
  const client = createAccessClient({ baseUrl: 'http://localhost', fetch: async input => {
    const request = new Request(input); requests.push(request);
    return Response.json(request.method === 'POST'
      ? { authorizationUrl: 'https://neodb.social/oauth/authorize?scope=read&state=one-time', expiresAt: 1600 }
      : { connected: false, baseUrl: null, account: null, pending: false });
  } });
  const navigate = vi.fn();
  render(<NeoDbPanel client={client} navigate={navigate} />);
  await screen.findByText('未连接');
  fireEvent.click(screen.getByRole('button', { name: '授权 NeoDB' }));
  await vi.waitFor(() => expect(navigate).toHaveBeenCalledWith('https://neodb.social/oauth/authorize?scope=read&state=one-time'));
  expect(await requests.find(r => r.method === 'POST')?.json()).toEqual({ baseUrl: 'https://neodb.social' });
});
it('cancels pending authorization and refreshes the retained account', async () => {
  let pending = true;
  const client = createAccessClient({ baseUrl: 'http://localhost', fetch: async input => {
    const request = new Request(input);
    if (request.method === 'DELETE') { pending = false; return Response.json({ ok: true }); }
    return Response.json({ connected: true, baseUrl: 'https://neodb.social', account: 'alice', pending });
  } });
  render(<NeoDbPanel client={client} />);
  fireEvent.click(await screen.findByRole('button', { name: '取消 NeoDB 授权' }));
  await vi.waitFor(() => expect(screen.queryByRole('button', { name: '取消 NeoDB 授权' })).toBeNull());
  expect(screen.getByText('alice · https://neodb.social')).toBeTruthy();
});
