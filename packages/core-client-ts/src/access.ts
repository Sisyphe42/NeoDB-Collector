import createClient from 'openapi-fetch';
import { validateSessionView, validatePairingLink, validateClientList, validateActionResult, validateApiError, validateNeoDbStatus, validateNeoDbAuthorization, validateMappingProfile, validateMappingDocument, validatePreviewJobView, validatePreviewMutationResult, validatePreviewResultPage, validatePreviewAccepted, validatePreviewImportAccepted, validatePreviewAdmissionFailure, validatePreviewImdbResolutionRequest, type paths, type components } from '@neodb-collector/contracts';

export type SessionView = components['schemas']['SessionView'];
export type PairedClient = components['schemas']['PairedClient'];
export type PairingRequest = components['schemas']['PairingRequest'];
export type PairingLink = components['schemas']['PairingLink'];
export type NeoDbStatus = components['schemas']['NeoDbStatus'];
export type MappingProfile = components['schemas']['MappingProfile'];
export type ProfileFormat = components['schemas']['ProfileFormat'];
export type PreviewJobView = components['schemas']['PreviewJobView'];
export type PreviewCancelRequest = components['schemas']['PreviewCancelRequest'];
export type PreviewMutationResult = components['schemas']['PreviewMutationResult'];
export type PreviewResultPage = components['schemas']['PreviewResultPage'];
export type PreviewImportSelection = components['schemas']['PreviewImportSelection'];
export type PreviewImdbResolutionRequest = components['schemas']['PreviewImdbResolutionRequest'];
export type PreviewResolutionChoice = components['schemas']['PreviewResolutionChoice'];
export type PreviewCatalogReferenceRequest = components['schemas']['PreviewCatalogReferenceRequest'];
export type PreviewResumeRequest = components['schemas']['PreviewResumeRequest'];
export type PreviewAccepted = components['schemas']['PreviewAccepted'];
export type PreviewImportAccepted = components['schemas']['PreviewImportAccepted'];
export type PreviewAdmissionFailure = components['schemas']['PreviewAdmissionFailure'];
export class AccessClientError extends Error {
  constructor(public readonly code: string, public readonly field?: string) { super('Local access request failed.'); }
}

export class PreviewAdmissionClientError extends AccessClientError {
  constructor(code: string, field: string | undefined, public readonly jobId: string, public readonly attempt: number) {
    super(code, field);
    this.name = 'PreviewAdmissionClientError';
  }
}

export function createAccessClient(options: { baseUrl?: string; fetch?: typeof fetch; timeoutMs?: number } = {}) {
  const client = createClient<paths>({ ...options, credentials: 'same-origin', redirect: 'error' });
  let csrf: string | undefined;
  const requestOptions = (callerSignal?: AbortSignal, defaultTimeoutMs = 5000) => {
    const timeout = AbortSignal.timeout(options.timeoutMs ?? defaultTimeoutMs);
    return { signal: callerSignal ? AbortSignal.any([callerSignal, timeout]) : timeout };
  };
  const mutationOptions = (callerSignal?: AbortSignal) => ({ ...requestOptions(callerSignal), headers: { 'X-CSRF-Token': csrf ?? '' } });
  const admissionMutationOptions = (callerSignal?: AbortSignal) => ({ ...requestOptions(callerSignal, 30_000), headers: { 'X-CSRF-Token': csrf ?? '' } });
  const rawBodySerializer = (body: string | PreviewImdbResolutionRequest) => {
    if (typeof body !== 'string') throw new AccessClientError('request_invalid');
    return body;
  };
  async function checked<T>(request: Promise<{ data?: unknown; error?: unknown; response: Response }>, validate: (value: unknown) => value is T, expectedStatus?: number, failureValidate?: (value: unknown) => value is PreviewAdmissionFailure): Promise<T> {
    try {
      const result = await request;
      if (!result.response.ok) {
        if (result.response.status === 401) csrf = undefined;
        if (failureValidate && failureValidate(result.error)) {
          const failure = result.error;
          throw new PreviewAdmissionClientError(failure.error.code, failure.error.field ?? undefined, failure.jobId, failure.attempt);
        }
        throw new AccessClientError(validateApiError(result.error) ? result.error.code : 'request_failed', validateApiError(result.error) ? result.error.field ?? undefined : undefined);
      }
      if (expectedStatus !== undefined && result.response.status !== expectedStatus) throw new AccessClientError('invalid_response');
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
    async previewJob(id: string, signal?: AbortSignal) {
      return checked(client.GET('/api/v1/previews/{id}', { ...requestOptions(signal), params: { path: { id } } }), validatePreviewJobView);
    },
    async previewResults(id: string, query: NonNullable<paths['/api/v1/previews/{id}/results']['get']['parameters']['query']>, signal?: AbortSignal) {
      return checked(client.GET('/api/v1/previews/{id}/results', { ...requestOptions(signal), params: { path: { id }, query } }), validatePreviewResultPage);
    },
    async submitImdbRatings(csv: string, selection: PreviewImportSelection, signal?: AbortSignal) {
      const optionsForRequest = admissionMutationOptions(signal);
      return checked(
        client.POST('/api/v1/imports/imdb/ratings', {
          ...optionsForRequest,
          headers: { ...optionsForRequest.headers, 'Content-Type': 'text/csv' },
          params: { query: selection },
          body: csv,
          bodySerializer: rawBodySerializer,
        }),
        validatePreviewImportAccepted,
        202,
        validatePreviewAdmissionFailure,
      );
    },
    async submitImdbRatingsWithResolution(body: PreviewImdbResolutionRequest, selection: PreviewImportSelection, signal?: AbortSignal) {
      if (!validatePreviewImdbResolutionRequest(body)) throw new AccessClientError('request_invalid');
      return checked(
        client.POST('/api/v1/imports/imdb/ratings', {
          ...admissionMutationOptions(signal),
          params: { query: selection },
          body,
        }),
        validatePreviewImportAccepted,
        202,
        validatePreviewAdmissionFailure,
      );
    },
    async resumePreview(id: string, body: PreviewResumeRequest, signal?: AbortSignal) {
      return checked(
        client.POST('/api/v1/previews/{id}/resume', {
          ...admissionMutationOptions(signal),
          params: { path: { id } },
          body,
        }),
        validatePreviewAccepted,
        202,
        validatePreviewAdmissionFailure,
      );
    },
    async cancelPreview(id: string, expectedAttempt: number, signal?: AbortSignal) {
      return checked(client.POST('/api/v1/previews/{id}/cancel', { ...mutationOptions(signal), params: { path: { id } }, body: { expectedAttempt } }), validatePreviewMutationResult);
    },
    async deletePreview(id: string, signal?: AbortSignal) {
      return checked(client.DELETE('/api/v1/previews/{id}', { ...mutationOptions(signal), params: { path: { id } } }), validatePreviewMutationResult);
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
