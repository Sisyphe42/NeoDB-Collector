import type { AccessClient } from '@neodb-collector/core-client';

// Called once before React mounts, so StrictMode cannot replay a single-use exchange.
export function startSession(client: AccessClient, location: Pick<Location, 'hash' | 'pathname' | 'search'> = window.location, history: Pick<History, 'replaceState'> = window.history, compatible?: () => Promise<unknown>) {
  const key = new URLSearchParams(location.hash.slice(1)).get('key');
  if (location.hash) history.replaceState(null, '', location.pathname + location.search);
  return compatible ? compatible().then(() => client.session(key ?? undefined)) : client.session(key ?? undefined);
}
