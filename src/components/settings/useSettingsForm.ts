import { useCallback, useEffect, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { setGatewayToken } from '../../lib/gateway';
import { readSettings, saveSettings } from '../../lib/settings';
import { CREDENTIAL_LABELS, defaultConfig, KEEP_SENTINEL, isConfiguredSecret, SettingsConfig, SOURCES_LIST } from './model';

/** Loads, edits and saves the gateway settings as one form. */
export function useSettingsForm() {
  const [config, setConfig] = useState<SettingsConfig>(defaultConfig);
  const [loaded, setLoaded] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    const controller = new AbortController();
    setLoaded(false);
    setLoading(true);
    void (async () => {
      try {
        const data = await readSettings(controller.signal);
        if (controller.signal.aborted) return;
        setConfig((prev) => {
          const normalized = { ...data } as Partial<SettingsConfig>;
          if (typeof normalized.enabled_sources === 'string') {
            const available = new Set(SOURCES_LIST.map((source) => source.id));
            normalized.enabled_sources = normalized.enabled_sources.split(',')
              .map((source) => source.trim().toLowerCase())
              .filter((source, index, list) => available.has(source) && list.indexOf(source) === index)
              .join(',');
          }
          return { ...prev, ...normalized };
        });
        setLoaded(true);
        setError(null);
      } catch (e) {
        if (!controller.signal.aborted) setError(`Unable to load gateway settings; saving is locked to prevent overriding with defaults: ${(e as Error).message}`);
      } finally {
        if (!controller.signal.aborted) setLoading(false);
      }
    })();
    return () => controller.abort();
  }, [attempt]);

  const set = useCallback((field: string, value: string) => setConfig((prev) => ({ ...prev, [field]: value })), []);
  const update = useCallback((patch: Partial<SettingsConfig>) => setConfig((prev) => ({ ...prev, ...patch })), []);

  // A webview sign-in stores its session natively; mark the field as saved.
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten: undefined | (() => void);
    void import('@tauri-apps/api/event').then(({ listen }) => listen<string>('settings-session-updated', (event) => {
      if (!disposed && CREDENTIAL_LABELS[`${event.payload}_session`]) set(`${event.payload}_session`, KEEP_SENTINEL);
    })).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch(() => undefined);
    return () => { disposed = true; unlisten?.(); };
  }, [set]);

  const save = async () => {
    if (!loaded || saving) return;
    setError(null);
    setSaved(false);
    setSaving(true);
    try {
      await saveSettings(config as unknown as Record<string, string>);
      if (!isConfiguredSecret(config.mcp_auth_token)) setGatewayToken(config.mcp_auth_token || null);
      // Saved secrets are write-only from here on.
      setConfig((current) => {
        const next = { ...current } as Record<string, string>;
        for (const key of Object.keys(CREDENTIAL_LABELS)) {
          if (CREDENTIAL_LABELS[key].secret && next[key] && next[key] !== KEEP_SENTINEL) next[key] = KEEP_SENTINEL;
        }
        return next as SettingsConfig;
      });
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      setSaved(false);
      setError(`Failed to save settings. Changes remain in form: ${(e as Error).message}`);
    } finally {
      setSaving(false);
    }
  };

  return {
    config, set, update, loaded, loading, saving, saved, error, setError, save,
    reload: () => setAttempt((n) => n + 1),
  };
}
