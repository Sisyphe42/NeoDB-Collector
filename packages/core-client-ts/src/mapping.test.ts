import { describe, expect, it } from 'vitest';
import { validateMappingProfile } from '@neodb-collector/contracts';
import { createAccessClient } from './access.js';

const profile = {
  schemaVersion: 1, id: 'imdb_ratings', version: 1,
  sourceConnectorId: 'imdb:ratings_csv', sinkConnectorId: 'neodb', enabled: false,
  rules: [{ id: 'rating', enabled: true, target: 'rating_grade', operation: { kind: 'clamp', from: 'rating', min: 0, max: 10 } }],
};

describe('mapping client', () => {
  it('uses generated requests and validators with CSRF only on save', async () => {
    const seen: Request[] = [];
    const client = createAccessClient({ baseUrl: 'http://localhost:19423', fetch: async input => {
      const request = new Request(input); seen.push(request);
      return Response.json(request.url.endsWith('/session') ? { csrfToken: 'csrf', expiresAt: 10000 } : profile);
    } });
    await client.session();
    const document = JSON.stringify(profile);
    expect(await client.validateMapping({ format: 'json', document })).toEqual(profile);
    expect(await client.saveMapping(profile.id, { format: 'json', document, expectedVersion: 0 })).toEqual(profile);
    expect(await client.mappingProfile(profile.id)).toEqual(profile);
    expect(seen[1]?.headers.get('x-csrf-token')).toBeNull();
    expect(seen[2]?.headers.get('x-csrf-token')).toBe('csrf');
    expect(seen[3]?.headers.get('x-csrf-token')).toBeNull();
    expect(await seen[2]?.json()).toEqual({ format: 'json', document, expectedVersion: 0 });
  });
  it('rejects invalid wire shapes and nonfinite double values', async () => {
    expect(validateMappingProfile(profile)).toBe(true);
    const invalid = structuredClone(profile);
    invalid.rules[0]!.operation.max = Infinity;
    expect(validateMappingProfile(invalid)).toBe(false);
    expect(validateMappingProfile({ ...profile, unexpected: true })).toBe(false);
    await expect(createAccessClient({ baseUrl: 'http://localhost', fetch: async () => Response.json({ ...profile, version: '1' }) }).mappingProfile(profile.id)).rejects.toMatchObject({ code: 'invalid_response' });
  });
  it('preserves conflict codes without echoing server diagnostics', async () => {
    const client = createAccessClient({ baseUrl: 'http://localhost', fetch: async () => Response.json({ code: 'mapping_version_conflict', message: 'private document', retryable: false }, { status: 409 }) });
    await expect(client.saveMapping(profile.id, { format: 'json', document: JSON.stringify(profile), expectedVersion: 0 })).rejects.toMatchObject({ code: 'mapping_version_conflict', message: 'Local access request failed.' });
  });
});
