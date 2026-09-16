import createClient from 'openapi-fetch';
import { validateSessionView, validatePairingLink, validateClientList, validateActionResult, validateApiError, type paths, type components } from '@neodb-collector/contracts';

export type SessionView = components['schemas']['SessionView'];
export type PairedClient = components['schemas']['PairedClient'];
export type PairingRequest = components['schemas']['PairingRequest'];
export type PairingLink = components['schemas']['PairingLink'];
export class AccessClientError extends Error {
  constructor(public readonly code: string) { super('Local access request failed.'); }
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
        throw new AccessClientError(validateApiError(result.error) ? result.error.code : 'request_failed');
      }
      if (!validate(result.data)) throw new AccessClientError('invalid_response');
      return result.data;
    } catch (error) {
      if (error instanceof AccessClientError) throw error;
      throw new AccessClientError('unavailable');
    }
  }
  return {
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
