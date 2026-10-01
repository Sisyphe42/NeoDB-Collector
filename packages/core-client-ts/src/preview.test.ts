import { describe, expect, it } from 'vitest';
import { validatePreviewJobView, validatePreviewResultPage } from '@neodb-collector/contracts';
import { createAccessClient } from './access.js';

const job = {
  id: 'preview_1',
  status: 'completed',
  attempt: 2,
  createdAt: 1_700_000_000,
  updatedAt: 1_700_000_005,
  expiresAt: 1_702_592_000,
  observationCount: 7,
  profileId: 'imdb_ratings',
  profileVersion: 3,
  baseUrl: 'https://neodb.example',
  account: 'alice',
  bindingState: 'current',
  historical: true,
  executable: false,
};

const item = {
  uuid: 'item-1',
  canonicalUrl: 'https://neodb.example/item/item-1',
  detailUrl: 'https://neodb.example/api/items/item-1',
  mediaType: 'movie',
  model: 'Movie',
  parentUuid: null,
  externalIds: [
    { scheme: 'imdb:title', value: 'tt1234567' },
  ],
};

const contradictoryItem = {
  ...item,
  externalIds: [
    { scheme: 'imdb:title', value: 'tt1234567' },
    { scheme: 'imdb:title', value: 'tt7654321' },
  ],
};

const result = (ordinal: number, outcome: unknown) => ({
  ordinal,
  attempt: 2,
  committedAt: 1_700_000_010 + ordinal,
  sourceConnectorId: 'imdb:ratings_csv',
  sourceRecordId: `record-${ordinal}`,
  sourceFingerprint: `sha256-${ordinal}`,
  profileId: 'imdb_ratings',
  profileVersion: 3,
  baseUrl: 'https://neodb.example',
  account: 'alice',
  historical: true,
  executable: false,
  outcome,
});

const comparedOutcome = {
  kind: 'compared',
  item,
  observedAt: 1_700_000_001,
  comparison: {
    profileId: 'imdb_ratings',
    profileVersion: 3,
    fields: [
      {
        ruleId: 'shelf_type',
        field: 'shelf_type',
        suggestion: 'complete',
        sink: { kind: 'known', value: 'complete' },
        status: 'skipped',
        reason: 'already_equal',
      },
      {
        ruleId: 'rating_grade',
        field: 'rating_grade',
        suggestion: 8,
        sink: { kind: 'empty' },
        status: 'accepted',
        reason: 'sink_empty_accepts_suggestion',
      },
      {
        ruleId: 'rating_unknown',
        field: 'rating_grade',
        suggestion: 8,
        sink: { kind: 'unknown' },
        status: 'needs_review',
        reason: 'sink_state_unknown',
      },
    ],
  },
  provenance: 'automatic',
};

const manualOutcome = {
  kind: 'compared',
  item,
  observedAt: 1_700_000_002,
  comparison: {
    profileId: 'imdb_ratings',
    profileVersion: 3,
    fields: [{
      ruleId: 'shelf_type',
      field: 'shelf_type',
      suggestion: 'complete',
      sink: { kind: 'known', value: 'dropped' },
      status: 'rejected',
      reason: 'mapping_rule_rejected',
    }],
  },
  provenance: 'manual',
};

const page = {
  jobId: job.id,
  attempt: 2,
  offset: 0,
  limit: 100,
  historical: true,
  executable: false,
  items: [
    result(0, comparedOutcome),
    result(1, manualOutcome),
    result(2, { kind: 'needs_review', item: contradictoryItem, reason: 'multiple identities need confirmation' }),
    result(3, {
      kind: 'search_candidates',
      candidates: [{ title: 'A title', item }],
      moreAvailable: true,
    }),
    result(4, { kind: 'rate_limited' }),
    result(5, { kind: 'unavailable' }),
    result(6, { kind: 'unsupported' }),
  ],
};

describe('preview inspection client', () => {
  it('validates historical metadata, every outcome and comparison evidence', async () => {
    expect(validatePreviewJobView(job)).toBe(true);
    expect(validatePreviewResultPage(page)).toBe(true);

    const client = createAccessClient({
      baseUrl: 'http://localhost:19423',
      fetch: async input => {
        const request = new Request(input);
        return Response.json(request.url.includes('/results') ? page : job);
      },
    });

    expect(await client.previewJob(job.id)).toEqual(job);
    expect(await client.previewResults(job.id, { attempt: 2, offset: 0, limit: 100 })).toEqual(page);

    await expect(createAccessClient({
      baseUrl: 'http://localhost:19423',
      fetch: async () => Response.json({ ...job, unexpected: true }),
    }).previewJob(job.id)).rejects.toMatchObject({ code: 'invalid_response' });
  });

  it('uses encoded paths and generated query parameters without body or CSRF', async () => {
    const seen: Request[] = [];
    const client = createAccessClient({
      baseUrl: 'http://localhost:19423',
      fetch: async input => {
        const request = new Request(input);
        seen.push(request);
        if (request.url.endsWith('/session')) return Response.json({ csrfToken: 'csrf', expiresAt: 10_000 });
        return Response.json(request.url.includes('/results') ? page : job);
      },
    });

    await client.session();
    const encodedId = encodeURIComponent('job /?x=1#fragment');
    await client.previewJob('job /?x=1#fragment');
    await client.previewResults('job /?x=1#fragment', { attempt: 2, offset: 7, limit: 50 });

    expect(seen[1]?.url).toBe(`http://localhost:19423/api/v1/previews/${encodedId}`);
    const resultsRequest = seen[2];
    expect(resultsRequest?.url).toBe(`http://localhost:19423/api/v1/previews/${encodedId}/results?attempt=2&offset=7&limit=50`);
    for (const request of seen.slice(1)) {
      expect(request.method).toBe('GET');
      expect(request.body).toBeNull();
      expect(request.headers.get('x-csrf-token')).toBeNull();
      expect(request.credentials).toBe('same-origin');
      expect(request.redirect).toBe('error');
    }
  });

  it('rejects unknown, missing, extra, wrong-variant and invalid numeric/flag shapes', () => {
    const extraJob = { ...job, secret: 'must not cross the wire' };
    expect(validatePreviewJobView(extraJob)).toBe(false);

    const missingJob: Record<string, unknown> = { ...job };
    delete missingJob.id;
    expect(validatePreviewJobView(missingJob)).toBe(false);
    expect(validatePreviewJobView({ ...job, status: 'paused' })).toBe(false);
    expect(validatePreviewJobView({ ...job, attempt: 0 })).toBe(false);
    expect(validatePreviewJobView({ ...job, profileVersion: 0 })).toBe(false);
    expect(validatePreviewJobView({ ...job, historical: false })).toBe(false);
    expect(validatePreviewJobView({ ...job, executable: true })).toBe(false);

    const missingOutcome = structuredClone(page);
    const missingItem = missingOutcome.items[0];
    if (!missingItem) throw new Error('fixture must contain a result');
    Reflect.deleteProperty(missingItem, 'outcome');
    expect(validatePreviewResultPage(missingOutcome)).toBe(false);

    const extraOutcome = structuredClone(page);
    const extraItem = extraOutcome.items[0];
    if (!extraItem) throw new Error('fixture must contain a result');
    extraItem.outcome = { ...comparedOutcome, unexpected: true };
    expect(validatePreviewResultPage(extraOutcome)).toBe(false);

    const wrongVariant = structuredClone(page);
    const wrongItem = wrongVariant.items[0];
    if (!wrongItem) throw new Error('fixture must contain a result');
    wrongItem.outcome = { kind: 'not_a_preview_outcome' };
    expect(validatePreviewResultPage(wrongVariant)).toBe(false);
    expect(validatePreviewResultPage({ ...page, attempt: 0 })).toBe(false);
    expect(validatePreviewResultPage({ ...page, limit: 101 })).toBe(false);
    expect(validatePreviewResultPage({ ...page, historical: false })).toBe(false);
    expect(validatePreviewResultPage({ ...page, executable: true })).toBe(false);

    const nonfiniteCompared = structuredClone(comparedOutcome);
    nonfiniteCompared.comparison.fields[1]!.suggestion = Infinity;
    expect(validatePreviewResultPage({ ...page, items: [result(0, nonfiniteCompared), ...page.items.slice(1)] })).toBe(false);
  });

  it('sanitizes preview HTTP errors and does not retry', async () => {
    const cases: readonly (readonly [number, string])[] = [
      [404, 'preview_job_not_found'],
      [409, 'preview_attempt_changed'],
      [503, 'preview_unavailable'],
    ];
    for (const [status, code] of cases) {
      let calls = 0;
      const client = createAccessClient({
        baseUrl: 'http://localhost',
        fetch: async () => {
          calls++;
          return Response.json({ code, message: 'private upstream detail', retryable: status === 503 }, { status });
        },
      });
      await expect(client.previewJob('job')).rejects.toMatchObject({ code, message: 'Local access request failed.' });
      expect(calls).toBe(1);
    }
  });

  it('normalizes network failures without leaking details', async () => {
    await expect(createAccessClient({
      baseUrl: 'http://localhost',
      fetch: async () => { throw new Error('secret upstream response'); },
    }).previewJob('job')).rejects.toMatchObject({ code: 'unavailable', message: 'Local access request failed.' });
  });

  it('bounds pending GETs and respects caller cancellation while retaining Requests', async () => {
    const pendingRequests: Request[] = [];
    const client = createAccessClient({
      baseUrl: 'http://localhost',
      timeoutMs: 20,
      fetch: async input => {
        const request = new Request(input);
        pendingRequests.push(request);
        return new Promise<Response>((_, reject) => {
          if (request.signal.aborted) reject(request.signal.reason);
          else request.signal.addEventListener('abort', () => reject(request.signal.reason), { once: true });
        });
      },
    });

    await expect(client.previewJob('job')).rejects.toMatchObject({ code: 'unavailable' });
    const controller = new AbortController();
    const cancelled = client.previewResults('job', { attempt: 2 }, controller.signal);
    controller.abort();
    await expect(cancelled).rejects.toMatchObject({ code: 'unavailable' });
    expect(pendingRequests).toHaveLength(2);
    expect(pendingRequests.every(request => request.signal.aborted)).toBe(true);
  });
});
