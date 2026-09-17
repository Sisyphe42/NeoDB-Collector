import createClient from 'openapi-fetch';
export { createAccessClient, AccessClientError, type AccessClient, type SessionView, type PairedClient, type PairingLink, type NeoDbStatus, type MappingProfile, type ProfileFormat } from './access.js';
import { API_VERSION, SCHEMA_VERSION, validateCapabilities, validateHealth, type paths, type components } from '@neodb-collector/contracts';

export type Health = components['schemas']['HealthDocument'];
export type Capabilities = components['schemas']['CapabilityDocument'];
export type Snapshot = { health: Health; capabilities: Capabilities };
export class CoreClientError extends Error {
  constructor(public readonly kind: 'unavailable' | 'incompatible' | 'invalid_response', message: string) { super(message); }
}

export function createCoreClient(options: { baseUrl?: string; fetch?: typeof fetch; timeoutMs?: number } = {}) {
  const client = createClient<paths>({ ...options });
  return {
    async status(signal?: AbortSignal): Promise<Snapshot> {
      const timeout = AbortSignal.timeout(options.timeoutMs ?? 5000);
      const combined = signal ? AbortSignal.any([signal, timeout]) : timeout;
      try {
        const [healthResult, capabilityResult] = await Promise.all([
          client.GET('/api/v1/health', { signal: combined }),
          client.GET('/api/v1/capabilities', { signal: combined }),
        ]);
        // A degraded health document is deliberately returned with HTTP 503.
        const health: unknown = healthResult.response.status === 503 ? healthResult.error : healthResult.data;
        const capabilities: unknown = capabilityResult.data;
        if (![200, 503].includes(healthResult.response.status) || capabilityResult.response.status !== 200) {
          throw new CoreClientError('unavailable', 'Core returned an unexpected HTTP status.');
        }
        if (!validateHealth(health) || !validateCapabilities(capabilities)) {
          throw new CoreClientError('invalid_response', 'Core response does not match its contract.');
        }
        if (health.apiVersion !== API_VERSION || health.schemaVersion !== SCHEMA_VERSION || capabilities.schemaVersion !== SCHEMA_VERSION) {
          throw new CoreClientError('incompatible', 'Core API or schema version is unsupported.');
        }
        return { health, capabilities };
      } catch (error) {
        if (error instanceof CoreClientError) throw error;
        throw new CoreClientError('unavailable', 'Cannot reach the local Core service.');
      }
    },
  };
}
