import { useCallback, useEffect, useRef, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import type { Update } from '@tauri-apps/plugin-updater';

const LATER_KEY = 'scholargate_update_later';

function laterVersion(): string | null {
  try {
    return localStorage.getItem(LATER_KEY);
  } catch {
    return null;
  }
}

/**
 * Checks for an update shortly after launch. Instead of a blocking native
 * dialog, it exposes the offer so the shell can show an in-app banner.
 */
export function useAutoUpdate() {
  const updateRef = useRef<Update | null>(null);
  const [version, setVersion] = useState<string | null>(null);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState('');

  useEffect(() => {
    if (!isTauri()) return;
    const timer = window.setTimeout(() => {
      void (async () => {
        try {
          const { check } = await import('@tauri-apps/plugin-updater');
          const update = await check({ timeout: 30_000 });
          if (!update) return;
          if (laterVersion() === update.version) {
            await update.close();
            return;
          }
          updateRef.current = update;
          setVersion(update.version);
        } catch (err) {
          console.error('Automatic update check failed', err);
        }
      })();
    }, 3_000);
    return () => window.clearTimeout(timer);
  }, []);

  const install = useCallback(async () => {
    const update = updateRef.current;
    if (!update) return;
    setInstalling(true);
    setError('');
    try {
      await update.downloadAndInstall();
      const { relaunch } = await import('@tauri-apps/plugin-process');
      await relaunch();
    } catch (err) {
      console.error('Update install failed', err);
      setError('ScholarGate could not install the update. Please try again later.');
      setInstalling(false);
    }
  }, []);

  const later = useCallback(async () => {
    const update = updateRef.current;
    if (!update) return;
    try {
      localStorage.setItem(LATER_KEY, update.version);
    } catch {
      /* the offer comes back next launch */
    }
    updateRef.current = null;
    setVersion(null);
    setError('');
    await update.close().catch(() => {});
  }, []);

  return { version, installing, error, install, later };
}
