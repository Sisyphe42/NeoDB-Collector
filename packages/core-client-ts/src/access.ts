import createClient from 'openapi-fetch';
import { validateSessionView, validatePairingLink, validateClientList, validateActionResult, validateApiError, validateNeoDbStatus, validateNeoDbAuthorization, validateMappingProfile, validateMappingDocument, type paths, type components } from '@neodb-collector/contracts';

export type SessionView = components['schemas']['SessionView'];
export type PairedClient = components['schemas']['PairedClient'];
export type PairingRequest = components['schemas']['PairingRequest'];
export type PairingLink = components['schemas']['PairingLink'];
export type NeoDbStatus = components['schemas']['NeoDbStatus'];
export type MappingProfile = components['schemas']['MappingProfile'];
export type ProfileFormat = components['schemas']['ProfileFormat'];
export class AccessClientError extends Error {
  constructor(public readonly code: string, public readonly field?: string) { super('Local access request failed.'); }
}

export function createAccessClient(options: { baseUrl?: string; fetch?: typeof fetch; timeoutMs?: number } = {}) {
  const client = createClient<paths>({ ...options, credentials: 'same-origin', redirect: 'error' });
  let csrf: string | undefined;
  const requestOptions = () => ({ signal: AbortSignal.timeout(options.timeoutMs ?? 5000) });
  const mutationOptions = () => ({ ...requestOptions(), headers: { 'X-CSRF-Token': csrf ?? '' } });
  async function checked<T>(request: Promise<{ data?: unknown; error?: unknown; response: Response }>, validate: (value: unknown) => value is T): Promise<T> {
    try {
      const result = await request;
      if (!result.response.ok) {
        if (result.response.status === 401) csrf = undefined;
        throw new AccessClientError(validateApiError(result.error) ? result.error.code : 'request_failed', validateApiError(result.error) ? result.error.field ?? undefined : undefined);
      }
      if (!validate(result.data)) throw new AccessClientError('invalid_response');
      return result.data;
    } catch (error) {
      if (error instanceof AccessClientError) throw error;
      throw new AccessClientError('unavailable');
    }
  }
  return {
    async defaultMapping() {
      return checked(client.GET('/api/v1/mapping-profiles/defaults/imdb', requestOptions()), validateMappingProfile);
    },
    async convertMapping(body: components['schemas']['ConvertMappingDocument']) {
      return checked(client.POST('/api/v1/mapping-profiles/documents/convert', { ...requestOptions(), body }), validateMappingDocument);
    },
    async mappingProfile(id: string) {
      return checked(client.GET('/api/v1/mapping-profiles/{id}', { ...requestOptions(), params: { path: { id } } }), validateMappingProfile);
    },
    async validateMapping(body: components['schemas']['MappingDocument']) {
      return checked(client.POST('/api/v1/mapping-profiles/validate', { ...requestOptions(), body }), validateMappingProfile);
    },
    async saveMapping(id: string, body: components['schemas']['SaveMappingProfile']) {
      return checked(client.PUT('/api/v1/mapping-profiles/{id}', { ...mutationOptions(), params: { path: { id } }, body }), validateMappingProfile);
    },
    async neodbStatus() {
      return checked(client.GET('/api/v1/settings/neodb', requestOptions()), validateNeoDbStatus);
    },
    async startNeoDb(baseUrl: string) {
      return checked(client.POST('/api/v1/oauth/neodb', { ...mutationOptions(), signal: AbortSignal.timeout(options.timeoutMs ?? 30000), body: { baseUrl } }), validateNeoDbAuthorization);
    },
    async cancelNeoDb() {
      await checked(client.DELETE('/api/v1/oauth/neodb', mutationOptions()), validateActionResult);
    },
    async session(key?: string): Promise<SessionView> {
      const view = await checked(key === undefined
        ? client.GET('/api/v1/session', requestOptions())
        : client.POST('/api/v1/session/bootstrap', { ...requestOptions(), body: { key } }), validateSessionView);
      csrf = view.csrfToken;
      return view;
    },
    async logout() {
      await checked(client.DELETE('/api/v1/session', mutationOptions()), validateActionResult);
      csrf = undefined;
    },
    async clients() {
      return (await checked(client.GET('/api/v1/clients', requestOptions()), validateClientList)).clients;
    },
    async createPairing(body: PairingRequest) {
      return checked(client.POST('/api/v1/pairings', { ...mutationOptions(), body }), validatePairingLink);
    },
    async revoke(id: string) {
      await checked(client.DELETE('/api/v1/clients/{id}', { ...mutationOptions(), params: { path: { id } } }), validateActionResult);
    },
  };
}
export type AccessClient = ReturnType<typeof createAccessClient>;
