import { describe, expect, it } from 'vitest';
import { createCoreClient } from './index.js';
import { validateHealth } from '@neodb-collector/contracts';

const health = { status: 'ok', storage: 'ready', coreVersion: '0.1.0', apiVersion: 'v1', schemaVersion: 1 };
const capabilities = { schemaVersion: 1, connectorIds: [], mediaTypes: ['movie'], routes: {} };
function client(h: unknown = health, c: unknown = capabilities, healthStatus = 200) {
  return createCoreClient({ baseUrl: 'http://127.0.0.1:19423', fetch: async (input) => {
    const isHealth = new Request(input).url.endsWith('/health');
    return new Response(JSON.stringify(isHealth ? h : c), { status: isHealth ? healthStatus : 200, headers: { 'content-type': 'application/json' } });
  } });
}
describe('versioned Core client', () => {
  it('accepts a valid empty capability document', async () => { expect((await client().status()).capabilities.connectorIds).toEqual([]); });
  it('validates received values, not just compile-time types', async () => {
    expect(validateHealth({ ...health, schemaVersion: '1' })).toBe(false);
    expect(validateHealth({ ...health, schemaVersion: -1 })).toBe(false);
    expect(validateHealth({ ...health, schemaVersion: 4294967296 })).toBe(false);
    await expect(client({ status: 'ok' }).status()).rejects.toMatchObject({ kind: 'invalid_response' });
  });
  it('rejects unsupported versions', async () => {
    await expect(client({ ...health, apiVersion: 'v2' }).status()).rejects.toMatchObject({ kind: 'incompatible' });
    await expect(client(health, { ...capabilities, schemaVersion: 2 }).status()).rejects.toMatchObject({ kind: 'incompatible' });
  });
  it('retains degraded storage from HTTP 503', async () => {
    expect((await client({ ...health, status: 'degraded', storage: 'unavailable' }, capabilities, 503).status()).health.status).toBe('degraded');
  });
  it('normalizes network, malformed JSON and HTTP failures', async () => {
    await expect(createCoreClient({ fetch: async () => { throw new Error('private detail'); } }).status()).rejects.toMatchObject({ kind: 'unavailable' });
    await expect(client(health, capabilities, 500).status()).rejects.toMatchObject({ kind: 'unavailable' });
    await expect(createCoreClient({ baseUrl: 'http://localhost', fetch: async () => new Response('<html>') }).status()).rejects.toMatchObject({ kind: 'unavailable' });
  });
  it('bounds requests with a timeout and accepts caller cancellation', async () => {
    const timedClient = createCoreClient({ baseUrl: 'http://localhost', timeoutMs: 10, fetch: async input => {
      const { signal } = new Request(input);
      return new Promise<Response>((_, reject) => {
        if (signal.aborted) reject(signal.reason);
        else signal.addEventListener('abort', () => reject(signal.reason), { once: true });
      });
    } });
    await expect(timedClient.status()).rejects.toMatchObject({ kind: 'unavailable' });
    const controller = new AbortController();
    controller.abort();
    await expect(timedClient.status(controller.signal)).rejects.toMatchObject({ kind: 'unavailable' });
  });
});
