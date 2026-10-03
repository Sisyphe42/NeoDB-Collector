import { describe, expect, it, vi } from 'vitest';
import {
  validatePreviewAccepted,
  validatePreviewAdmissionFailure,
  validatePreviewImportAccepted,
  validatePreviewImdbResolutionRequest,
  validatePreviewResolutionChoice,
} from '@neodb-collector/contracts';
import { createAccessClient, PreviewAdmissionClientError } from './access.js';

const preview = {
  jobId: 'preview_1',
  attempt: 1,
  executable: false,
} as const;

const resumedPreview = {
  ...preview,
  attempt: 5,
} as const;

const accepted = {
  preview,
  observationCount: 2,
  skippedRowCount: 1,
  skippedColumnCount: 3,
  duplicateRowCount: 0,
} as const;

const selection = {
  profileId: 'imdb_ratings',
  profileVersion: 1,
} as const;

const structured = {
  csv: 'Const,Your Rating,Title Type\n',
  resolutionInputs: [{ kind: 'search' as const, ordinal: 0, sourceRecordId: 'tt1234567', query: 'Example' }],
};

const apiError = {
  code: 'preview_runtime_busy',
  message: 'private runtime detail',
  retryable: true,
} as const;

describe('preview submission client', () => {
  it('sends one strict structured submission with the existing query and mutation boundary', async () => {
    const seen: Request[] = [];
    const body = {
      csv: 'Const,Your Rating,Date Rated,Title,URL,Title Type,IMDb Rating,Runtime (mins),Year,Genres,Num Votes,Release Date,Directors\ntt1234567,8,2026-01-01,Example,https://www.imdb.com/title/tt1234567/,movie,7.0,90,2020,Drama,10,2020-01-01,Director\n',
      resolutionInputs: [
        { kind: 'search', ordinal: 0, sourceRecordId: 'tt1234567', query: 'Example' },
      ],
    } satisfies import('./access.js').PreviewImdbResolutionRequest;
    const client = createAccessClient({
      baseUrl: 'http://localhost:19423',
      fetch: async input => {
        const request = input instanceof Request ? input : new Request(input);
        seen.push(request);
        if (request.url.endsWith('/session')) return Response.json({ csrfToken: 'csrf', expiresAt: 10_000 });
        return Response.json(accepted, { status: 202 });
      },
    });

    await client.session();
    await expect(client.submitImdbRatingsWithResolution(body, selection)).resolves.toEqual(accepted);

    const request = seen[1];
    expect(request?.method).toBe('POST');
    expect(request?.url).toBe('http://localhost:19423/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1');
    expect(request?.headers.get('content-type')).toContain('application/json');
    expect(request?.headers.get('x-csrf-token')).toBe('csrf');
    expect(await request?.json()).toEqual(body);
    expect(seen).toHaveLength(2);
  });

  it('validates closed structured request variants before transport', async () => {
    const search = { kind: 'search', ordinal: 0, sourceRecordId: 'tt1234567', query: 'Example' };
    const manual = { kind: 'manual', ordinal: 1, sourceRecordId: 'tt7654321', item: { uuid: 'Item123', mediaType: 'tv', model: 'TVShow' } };
    expect(validatePreviewResolutionChoice(search)).toBe(true);
    expect(validatePreviewResolutionChoice(manual)).toBe(true);
    expect(validatePreviewImdbResolutionRequest({ csv: 'header\n', resolutionInputs: [search, manual] })).toBe(true);
    for (const invalid of [
      { ...search, unexpected: true },
      { ...search, kind: 'native' },
      { ...search, ordinal: 10_000 },
      { ...search, sourceRecordId: 'tt1' },
      { ...manual, item: { ...manual.item, extra: true } },
      { ...manual, item: { ...manual.item, uuid: '../item' } },
    ]) expect(validatePreviewResolutionChoice(invalid)).toBe(false);

    let calls = 0;
    const client = createAccessClient({ fetch: async () => { calls++; return Response.json(accepted, { status: 202 }); } });
    await expect(client.submitImdbRatingsWithResolution(
      { csv: 'header\n', resolutionInputs: [{ ...search, kind: 'native' } as never] },
      selection,
    )).rejects.toMatchObject({ code: 'request_invalid' });
    expect(calls).toBe(0);
  });

  it('sends raw CSV bytes, strict selection query, CSRF and only accepts HTTP 202', async () => {
    const seen: Request[] = [];
    const csv = '\uFEFFtitle,rating\ntt1234567,8\n';
    const client = createAccessClient({
      baseUrl: 'http://localhost:19423',
      fetch: async input => {
        const request = input instanceof Request ? input : new Request(input);
        seen.push(request);
        if (request.url.endsWith('/session')) return Response.json({ csrfToken: 'csrf', expiresAt: 10_000 });
        return Response.json(accepted, { status: 202 });
      },
    });

    await client.session();
    await expect(client.submitImdbRatings(csv, selection)).resolves.toEqual(accepted);

    const request = seen[1];
    expect(request).toBeDefined();
    if (!request) throw new Error('No upload request was sent.');
    expect(request?.method).toBe('POST');
    expect(request?.url).toBe('http://localhost:19423/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1');
    // Request.text() strips UTF-8 BOM while decoding; compare transport bytes.
    expect(Array.from(new Uint8Array(await request.arrayBuffer()))).toEqual(Array.from(new TextEncoder().encode(csv)));
    expect(request?.headers.get('content-type')).toBe('text/csv');
    expect(request?.headers.get('x-csrf-token')).toBe('csrf');
    expect(request?.credentials).toBe('same-origin');
    expect(request?.redirect).toBe('error');
  });

  it('sends JSON resume body on an encoded path and preserves the shared mutation boundary', async () => {
    const id = 'preview /?x=1#fragment';
    const seen: Request[] = [];
    const client = createAccessClient({
      baseUrl: 'http://localhost:19423',
      fetch: async input => {
        const request = input instanceof Request ? input : new Request(input);
        seen.push(request);
        if (request.url.endsWith('/session')) return Response.json({ csrfToken: 'csrf', expiresAt: 10_000 });
        return Response.json(resumedPreview, { status: 202 });
      },
    });

    await client.session();
    await expect(client.resumePreview(id, { expectedAttempt: 4 })).resolves.toEqual(resumedPreview);

    const request = seen[1];
    expect(request?.method).toBe('POST');
    expect(request?.url).toBe(`http://localhost:19423/api/v1/previews/${encodeURIComponent(id)}/resume`);
    expect(await request?.json()).toEqual({ expectedAttempt: 4 });
    expect(request?.headers.get('content-type')).toContain('application/json');
    expect(request?.headers.get('x-csrf-token')).toBe('csrf');
    expect(request?.credentials).toBe('same-origin');
    expect(request?.redirect).toBe('error');
  });

  it('accepts strict non-executable receipts and bounded count fields only', () => {
    expect(validatePreviewAccepted(preview)).toBe(true);
    expect(validatePreviewImportAccepted(accepted)).toBe(true);
    expect(validatePreviewAdmissionFailure({ jobId: 'preview_1', attempt: 1, error: apiError })).toBe(true);

    expect(validatePreviewAccepted({ ...preview, executable: true })).toBe(false);
    expect(validatePreviewAccepted({ ...preview, unexpected: true })).toBe(false);
    expect(validatePreviewImportAccepted({ ...accepted, unexpected: true })).toBe(false);
    expect(validatePreviewImportAccepted({
      ...accepted,
      observationCount: 10_000,
      skippedRowCount: 10_000,
      skippedColumnCount: 10_485_760,
      duplicateRowCount: 10_000,
    })).toBe(true);
    expect(validatePreviewImportAccepted({ ...accepted, observationCount: 10_001 })).toBe(false);
    expect(validatePreviewImportAccepted({ ...accepted, observationCount: -1 })).toBe(false);
    expect(validatePreviewImportAccepted({ ...accepted, skippedRowCount: 10_001 })).toBe(false);
    expect(validatePreviewImportAccepted({ ...accepted, skippedRowCount: 1.5 })).toBe(false);
    expect(validatePreviewImportAccepted({ ...accepted, skippedColumnCount: Infinity })).toBe(false);
    expect(validatePreviewImportAccepted({ ...accepted, skippedColumnCount: 10_485_761 })).toBe(false);
    expect(validatePreviewImportAccepted({ ...accepted, duplicateRowCount: 4_294_967_296 })).toBe(false);
    expect(validatePreviewImportAccepted({ ...accepted, duplicateRowCount: 10_001 })).toBe(false);

    const admissionFailure = { jobId: 'preview_1', attempt: 1, error: apiError };
    expect(validatePreviewAdmissionFailure(admissionFailure)).toBe(true);
    expect(validatePreviewAdmissionFailure({ ...admissionFailure, unexpected: true })).toBe(false);
    for (const jobId of ['', 'a'.repeat(129), 'preview/1']) {
      expect(validatePreviewAccepted({ ...preview, jobId })).toBe(false);
      expect(validatePreviewAdmissionFailure({ ...admissionFailure, jobId })).toBe(false);
    }
    for (const attempt of [0, -1, 4_294_967_296, 1.5]) {
      expect(validatePreviewAccepted({ ...preview, attempt })).toBe(false);
      expect(validatePreviewAdmissionFailure({ ...admissionFailure, attempt })).toBe(false);
    }
  });

  it('rejects malformed or executable upload and resume receipts, including status 200', async () => {
    const malformedResume = [
      { ...preview, executable: true },
      { ...preview, unexpected: true },
      { ...preview, attempt: 0 },
    ];
    for (const body of malformedResume) {
      await expect(createAccessClient({
        baseUrl: 'http://localhost',
        fetch: async () => Response.json(body, { status: 202 }),
      }).resumePreview('preview_1', { expectedAttempt: 1 })).rejects.toMatchObject({ code: 'invalid_response' });
    }
    const malformedImport = [
      { ...accepted, observationCount: -1 },
      { ...accepted, skippedColumnCount: 10_485_761 },
      { ...accepted, duplicateRowCount: 4_294_967_296 },
      { ...accepted, unexpected: true },
    ];
    for (const body of malformedImport) {
      await expect(createAccessClient({
        baseUrl: 'http://localhost',
        fetch: async () => Response.json(body, { status: 202 }),
      }).submitImdbRatings('title,rating\n', selection)).rejects.toMatchObject({ code: 'invalid_response' });
      await expect(createAccessClient({
        baseUrl: 'http://localhost',
        fetch: async () => Response.json(body, { status: 202 }),
      }).submitImdbRatingsWithResolution(structured, selection)).rejects.toMatchObject({ code: 'invalid_response' });
    }

    await expect(createAccessClient({
      baseUrl: 'http://localhost',
      fetch: async () => Response.json(preview, { status: 200 }),
    }).resumePreview('preview_1', { expectedAttempt: 1 })).rejects.toMatchObject({ code: 'invalid_response' });
    await expect(createAccessClient({
      baseUrl: 'http://localhost',
      fetch: async () => Response.json(accepted, { status: 200 }),
    }).submitImdbRatings('title,rating\n', selection)).rejects.toMatchObject({ code: 'invalid_response' });
  });

  it('sanitizes ordinary admission errors and exposes metadata only for a validated failure envelope', async () => {
    let ordinaryCalls = 0;
    const ordinary = createAccessClient({
      baseUrl: 'http://localhost',
      fetch: async () => {
        ordinaryCalls++;
        return Response.json(apiError, { status: 503 });
      },
    });
    await expect(ordinary.submitImdbRatings('title,rating\n', selection)).rejects.toMatchObject({
      code: apiError.code,
      message: 'Local access request failed.',
    });
    expect(ordinaryCalls).toBe(1);

    const failure = {
      jobId: 'preview_42',
      attempt: 2,
      error: {
        code: 'preview_persistence_uncertain',
        message: 'do not expose this server detail',
        field: 'attempt',
        retryable: false,
      },
    } as const;
    let admissionCalls = 0;
    const admission = createAccessClient({
      baseUrl: 'http://localhost',
      fetch: async () => {
        admissionCalls++;
        return Response.json(failure, { status: 503 });
      },
    });
    const admissionRequest = admission.submitImdbRatings('title,rating\n', selection);
    await expect(admissionRequest).rejects.toBeInstanceOf(PreviewAdmissionClientError);
    await expect(admissionRequest).rejects.toMatchObject({
      code: 'preview_persistence_uncertain',
      field: 'attempt',
      jobId: 'preview_42',
      attempt: 2,
      message: 'Local access request failed.',
    });
    expect(admissionCalls).toBe(1);

    let structuredAdmissionCalls = 0;
    const structuredAdmission = createAccessClient({
      baseUrl: 'http://localhost',
      fetch: async () => {
        structuredAdmissionCalls++;
        return Response.json(failure, { status: 503 });
      },
    });
    await expect(structuredAdmission.submitImdbRatingsWithResolution(structured, selection)).rejects.toMatchObject({
      code: 'preview_persistence_uncertain', jobId: 'preview_42', attempt: 2,
    });
    expect(structuredAdmissionCalls).toBe(1);

    let resumeAdmissionCalls = 0;
    const resumeAdmission = createAccessClient({
      baseUrl: 'http://localhost',
      fetch: async () => {
        resumeAdmissionCalls++;
        return Response.json({ ...failure, jobId: 'preview_43', attempt: 3 }, { status: 503 });
      },
    });
    const resumeAdmissionRequest = resumeAdmission.resumePreview('preview_1', { expectedAttempt: 2 });
    await expect(resumeAdmissionRequest).rejects.toMatchObject({
      code: 'preview_persistence_uncertain',
      field: 'attempt',
      jobId: 'preview_43',
      attempt: 3,
      message: 'Local access request failed.',
    });
    await expect(resumeAdmissionRequest).rejects.toBeInstanceOf(PreviewAdmissionClientError);
    expect(resumeAdmissionCalls).toBe(1);
  });

  it('does not invent IDs for malformed failure envelopes and never retries', async () => {
    let calls = 0;
    const malformedFailure = {
      jobId: 'preview_42',
      attempt: 0,
      error: apiError,
    };
    const client = createAccessClient({
      baseUrl: 'http://localhost',
      fetch: async () => {
        calls++;
        return Response.json(malformedFailure, { status: 503 });
      },
    });

    let thrown: unknown;
    try {
      await client.resumePreview('preview_1', { expectedAttempt: 1 });
    } catch (error) {
      thrown = error;
    }
    expect(thrown).toMatchObject({ code: 'request_failed', message: 'Local access request failed.' });
    expect(thrown).not.toBeInstanceOf(PreviewAdmissionClientError);
    expect(thrown).not.toHaveProperty('jobId');
    expect(calls).toBe(1);
  });

  it('clears in-memory CSRF after 401 before the next mutation', async () => {
    let call = 0;
    const seen: Request[] = [];
    const client = createAccessClient({
      baseUrl: 'http://localhost',
      fetch: async input => {
        const request = input instanceof Request ? input : new Request(input);
        seen.push(request);
        call++;
        if (call === 1) return Response.json({ csrfToken: 'csrf', expiresAt: 10_000 });
        if (call === 2) return Response.json({ code: 'authentication_required', message: 'redacted', retryable: false }, { status: 401 });
        return Response.json(preview, { status: 202 });
      },
    });

    await client.session();
    await expect(client.submitImdbRatings('title,rating\n', selection)).rejects.toMatchObject({ code: 'authentication_required' });
    await expect(client.resumePreview('preview_1', { expectedAttempt: 1 })).resolves.toEqual(preview);
    expect(seen[2]?.headers.get('x-csrf-token')).toBe('');
  });

  it('bounds admission calls and respects caller cancellation while retaining incoming Requests', async () => {
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

    await expect(client.submitImdbRatings('title,rating\n', selection)).rejects.toMatchObject({ code: 'unavailable' });
    await expect(client.submitImdbRatingsWithResolution(structured, selection)).rejects.toMatchObject({ code: 'unavailable' });
    const structuredController = new AbortController();
    const cancelledStructured = client.submitImdbRatingsWithResolution(structured, selection, structuredController.signal);
    structuredController.abort();
    await expect(cancelledStructured).rejects.toMatchObject({ code: 'unavailable' });
    const controller = new AbortController();
    const cancelled = client.resumePreview('preview_1', { expectedAttempt: 1 }, controller.signal);
    controller.abort();
    await expect(cancelled).rejects.toMatchObject({ code: 'unavailable' });
    expect(pendingRequests).toHaveLength(4);
    expect(pendingRequests.every(request => request.signal.aborted)).toBe(true);
  });

  it('uses a 30-second default for admission, keeps GETs at 5 seconds, and honors explicit overrides', async () => {
    const timeouts: number[] = [];
    const timeoutSpy = vi.spyOn(AbortSignal, 'timeout').mockImplementation(milliseconds => {
      timeouts.push(milliseconds);
      return new AbortController().signal;
    });
    try {
      const client = createAccessClient({
        baseUrl: 'http://localhost',
        fetch: async () => Response.json(accepted, { status: 202 }),
      });
      await client.submitImdbRatings('title,rating\n', selection);
      await client.submitImdbRatingsWithResolution(structured, selection);
      await expect(client.previewJob('preview_1')).rejects.toMatchObject({ code: 'invalid_response' });

      const overridden = createAccessClient({
        baseUrl: 'http://localhost',
        timeoutMs: 1_234,
        fetch: async () => Response.json(preview, { status: 202 }),
      });
      await overridden.resumePreview('preview_1', { expectedAttempt: 1 });
      expect(timeouts).toEqual([30_000, 30_000, 5_000, 1_234]);
    } finally {
      timeoutSpy.mockRestore();
    }
  });
});
