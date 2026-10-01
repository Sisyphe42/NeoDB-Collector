import { describe, expect, it } from 'vitest';
import { validatePreviewCancelRequest, validatePreviewMutationResult } from '@neodb-collector/contracts';
import { createAccessClient } from './access.js';

describe('preview management client', () => {
  it('validates the strict cancel request and mutation result shapes', () => {
    expect(validatePreviewCancelRequest({ expectedAttempt: 1 })).toBe(true);
    expect(validatePreviewCancelRequest({ expectedAttempt: 4_294_967_295 })).toBe(true);
    expect(validatePreviewCancelRequest({ expectedAttempt: 0 })).toBe(false);
    expect(validatePreviewCancelRequest({ expectedAttempt: -1 })).toBe(false);
    expect(validatePreviewCancelRequest({ expectedAttempt: 1.5 })).toBe(false);
    expect(validatePreviewCancelRequest({ expectedAttempt: 4_294_967_296 })).toBe(false);
    expect(validatePreviewCancelRequest({ expectedAttempt: Infinity })).toBe(false);
    expect(validatePreviewCancelRequest({})).toBe(false);
    expect(validatePreviewCancelRequest({ expectedAttempt: 1, unexpected: true })).toBe(false);

    expect(validatePreviewMutationResult({ applied: true })).toBe(true);
    expect(validatePreviewMutationResult({ applied: false })).toBe(true);
    expect(validatePreviewMutationResult({})).toBe(false);
    expect(validatePreviewMutationResult({ applied: 'true' })).toBe(false);
    expect(validatePreviewMutationResult({ applied: true, unexpected: true })).toBe(false);
  });

  it('sends encoded mutation paths, JSON cancel body, CSRF and no DELETE body', async () => {
    const id = 'preview /?x=1#fragment';
    const seen: Request[] = [];
    const client = createAccessClient({
      baseUrl: 'http://localhost:19423',
      fetch: async input => {
        const request = input instanceof Request ? input : new Request(input);
        seen.push(request);
        if (request.url.endsWith('/session/bootstrap')) return Response.json({ csrfToken: 'csrf', expiresAt: 10_000 });
        if (request.method === 'POST') return Response.json({ applied: true });
        return Response.json({ applied: false });
      },
    });

    await client.session('one-use-key');
    expect(await client.cancelPreview(id, 3)).toEqual({ applied: true });
    expect(await client.deletePreview(id)).toEqual({ applied: false });

    const encodedId = encodeURIComponent(id);
    const cancelRequest = seen[1];
    const deleteRequest = seen[2];
    expect(cancelRequest?.method).toBe('POST');
    expect(cancelRequest?.url).toBe(`http://localhost:19423/api/v1/previews/${encodedId}/cancel`);
    expect(await cancelRequest?.json()).toEqual({ expectedAttempt: 3 });
    expect(cancelRequest?.headers.get('content-type')).toContain('application/json');
    expect(cancelRequest?.headers.get('x-csrf-token')).toBe('csrf');
    expect(cancelRequest?.credentials).toBe('same-origin');
    expect(cancelRequest?.redirect).toBe('error');

    expect(deleteRequest?.method).toBe('DELETE');
    expect(deleteRequest?.url).toBe(`http://localhost:19423/api/v1/previews/${encodedId}`);
    expect(deleteRequest?.body).toBeNull();
    expect(deleteRequest?.headers.get('x-csrf-token')).toBe('csrf');
    expect(deleteRequest?.credentials).toBe('same-origin');
    expect(deleteRequest?.redirect).toBe('error');
  });

  it('accepts a false refusal without inventing a status', async () => {
    const client = createAccessClient({
      baseUrl: 'http://localhost',
      fetch: async () => Response.json({ applied: false }),
    });

    await expect(client.deletePreview('preview_1')).resolves.toEqual({ applied: false });
  });

  it('rejects malformed mutation replies and sanitizes static failures without retrying', async () => {
    for (const reply of [{}, { applied: 'yes' }, { applied: true, unexpected: true }]) {
      await expect(createAccessClient({
        baseUrl: 'http://localhost',
        fetch: async () => Response.json(reply),
      }).cancelPreview('preview_1', 1)).rejects.toMatchObject({ code: 'invalid_response' });
    }

    const failures: readonly (readonly [string, string])[] = [
      ['preview_cancel_failed', 'cancelPreview'],
      ['preview_delete_failed', 'deletePreview'],
    ];
    for (const [code, operation] of failures) {
      let calls = 0;
      const client = createAccessClient({
        baseUrl: 'http://localhost',
        fetch: async () => {
          calls++;
          return Response.json({ code, message: 'private management detail', retryable: false }, { status: 503 });
        },
      });
      const request = operation === 'cancelPreview'
        ? client.cancelPreview('preview_1', 1)
        : client.deletePreview('preview_1');
      await expect(request).rejects.toMatchObject({ code, message: 'Local access request failed.' });
      expect(calls).toBe(1);
    }
  });

  it('bounds mutation requests and respects caller cancellation while retaining Requests', async () => {
    const pendingRequests: Request[] = [];
    const client = createAccessClient({
      baseUrl: 'http://localhost',
      timeoutMs: 20,
      fetch: async input => {
        const request = input instanceof Request ? input : new Request(input);
        pendingRequests.push(request);
        return new Promise<Response>((_, reject) => {
          if (request.signal.aborted) reject(request.signal.reason);
          else request.signal.addEventListener('abort', () => reject(request.signal.reason), { once: true });
        });
      },
    });

    await expect(client.cancelPreview('preview_1', 1)).rejects.toMatchObject({ code: 'unavailable' });
    const controller = new AbortController();
    const cancelled = client.deletePreview('preview_1', controller.signal);
    controller.abort();
    await expect(cancelled).rejects.toMatchObject({ code: 'unavailable' });
    expect(pendingRequests).toHaveLength(2);
    expect(pendingRequests.every(request => request.signal.aborted)).toBe(true);
  });
});
