import { describe, expect, it } from 'vitest';
import { createAccessClient } from './access.js';

describe('access client', () => {
  it('keeps CSRF in memory and sends it only for mutations', async () => {
    const seen: Request[] = [];
    const client = createAccessClient({ baseUrl: 'http://127.0.0.1:19423', fetch: async input => {
      const request = new Request(input); seen.push(request);
      const body = request.url.endsWith('/bootstrap') ? { csrfToken: 'csrf', expiresAt: 10000 }
        : request.method === 'POST' ? { uri: 'http://127.0.0.1:19423/pair#key=key', expiresAt: 1000 }
        : request.method === 'DELETE' ? { ok: true } : { clients: [] };
      return Response.json(body);
    } });
    await client.session('bootstrap-secret');
    await client.clients();
    await client.createPairing({ clientName: 'CLI', clientKind: 'cli' });
    await client.revoke('client-id');
    await client.logout();
    expect(await seen[0]?.json()).toEqual({ key: 'bootstrap-secret' });
    expect(seen.every(request => !request.url.includes('secret'))).toBe(true);
    expect(seen[1]?.headers.get('x-csrf-token')).toBeNull();
    expect(seen[2]?.headers.get('x-csrf-token')).toBe('csrf');
    expect(seen[3]?.headers.get('x-csrf-token')).toBe('csrf');
    expect(seen.every(request => request.redirect === 'error')).toBe(true);
  });
  it('validates replies and strips network details', async () => {
    await expect(createAccessClient({ baseUrl: 'http://localhost', fetch: async () => Response.json({ csrfToken: 1 }) }).session()).rejects.toMatchObject({ code: 'invalid_response' });
    await expect(createAccessClient({ baseUrl: 'http://localhost', fetch: async () => { throw Error('secret upstream body'); } }).session()).rejects.toMatchObject({ code: 'unavailable', message: 'Local access request failed.' });
  });
  it('forgets CSRF after unauthorized responses', async () => {
    let index = 0;
    let sent: string | null = null;
    const client = createAccessClient({ baseUrl: 'http://localhost', fetch: async input => {
      index++;
      if (index === 1) return Response.json({ csrfToken: 'private', expiresAt: 10000 });
      if (index === 2) return Response.json({ code: 'authentication_required', message: 'redacted', retryable: false }, { status: 401 });
      sent = new Request(input).headers.get('x-csrf-token');
      return Response.json({ ok: true });
    } });
    await client.session();
    await expect(client.clients()).rejects.toMatchObject({ code: 'authentication_required' });
    await client.logout();
    expect(sent).toBe('');
  });
});
