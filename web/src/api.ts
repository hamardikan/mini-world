import type {
  CapabilitiesV1,
  CommandResultV1,
  CommandV1,
  ErrorV1,
  SnapshotV1,
} from './types';

export const API_BASE = import.meta.env.VITE_API_BASE || '/v1';

async function decode<T>(response: Response): Promise<T> {
  const body: unknown = await response.json();
  if (!response.ok) {
    throw body as ErrorV1;
  }
  return body as T;
}

export async function getCapabilities(signal?: AbortSignal): Promise<CapabilitiesV1> {
  return decode<CapabilitiesV1>(await fetch(`${API_BASE}/capabilities`, { signal }));
}

export async function getSnapshot(signal?: AbortSignal): Promise<SnapshotV1> {
  return decode<SnapshotV1>(await fetch(`${API_BASE}/snapshot`, { signal }));
}

export async function postStep(command: CommandV1, signal?: AbortSignal): Promise<CommandResultV1> {
  return decode<CommandResultV1>(
    await fetch(`${API_BASE}/commands`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(command),
      signal,
    }),
  );
}
