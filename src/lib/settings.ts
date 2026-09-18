import { invoke, isTauri } from '@tauri-apps/api/core';
import { gatewayFetch } from './gateway';

/**
 * Single source of truth for reading and writing gateway settings.
 *
 * There used to be two: SettingsPage went over trusted Tauri IPC while App.tsx
 * used the HTTP `/api/config` route for the same job, so the same settings had
 * two client-side paths that could drift apart. Both now call these.
 *
 * Inside the desktop app, settings travel over IPC — in-process, never over the
 * network, and never needing the gateway token. In browser dev there is no IPC,
 * so the HTTP route is the fallback; the backend validates identically either
 * way (`config::validate_patch`).
 */

export type SettingsMap = Record<string, string>;

/** Raised when the gateway requires a token the UI does not have. */
export class SettingsAuthRequired extends Error {
  constructor() {
    super('Gateway authentication is required. Reopen the desktop app or provide the administrator token.');
    this.name = 'SettingsAuthRequired';
  }
}

function normalize(data: unknown): SettingsMap {
  if (!data || typeof data !== 'object' || Array.isArray(data)) {
    throw new Error('Invalid configuration format');
  }
  if ((data as { authentication_required?: boolean }).authentication_required) {
    throw new SettingsAuthRequired();
  }
  return Object.fromEntries(
    Object.entries(data as Record<string, unknown>).map(([key, value]) => [
      key,
      value == null ? '' : String(value),
    ])
  );
}

export async function readSettings(signal?: AbortSignal): Promise<SettingsMap> {
  if (isTauri()) return normalize(await invoke('read_settings'));
  const res = await gatewayFetch('/api/config', { signal });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return normalize(await res.json());
}

export async function saveSettings(patch: SettingsMap): Promise<void> {
  if (isTauri()) {
    await invoke('save_settings', { payload: patch });
    return;
  }
  const res = await gatewayFetch('/api/config', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(patch),
  });
  const data = await res.json().catch(() => null);
  if (!res.ok || data?.success === false) {
    throw new Error(data?.error || `Server returned status code ${res.status}`);
  }
}
