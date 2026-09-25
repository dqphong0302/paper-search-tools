import { useCallback, useEffect, useState } from 'react';
import { TelemetryStats } from '../types';
import { gatewayFetch, initGateway } from '../lib/gateway';

const POLL_MS = 10_000;

/** Resolves the gateway port once, then polls telemetry while the window is visible. */
export function useGatewayStatus() {
  const [port, setPort] = useState<number | null>(null);
  const [telemetry, setTelemetry] = useState<TelemetryStats | null>(null);
  const [online, setOnline] = useState<boolean | null>(null);

  useEffect(() => {
    void initGateway().then(setPort);
  }, []);

  const refresh = useCallback(async () => {
    if (port === null) return;
    try {
      const res = await gatewayFetch('/api/telemetry');
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      setTelemetry(await res.json());
      setOnline(true);
    } catch {
      setOnline(false);
    }
  }, [port]);

  useEffect(() => {
    if (port === null) return;
    void refresh();

    // Closing the window hides it to the tray rather than quitting, so poll only
    // while the UI is visible and refresh once on becoming visible again.
    let interval: number | undefined;
    const stop = () => {
      if (interval !== undefined) window.clearInterval(interval);
      interval = undefined;
    };
    const start = () => {
      if (interval === undefined) interval = window.setInterval(() => void refresh(), POLL_MS);
    };
    const onVisibilityChange = () => {
      if (document.hidden) return stop();
      void refresh();
      start();
    };

    if (!document.hidden) start();
    document.addEventListener('visibilitychange', onVisibilityChange);
    return () => {
      stop();
      document.removeEventListener('visibilitychange', onVisibilityChange);
    };
  }, [port, refresh]);

  return { port, telemetry, online, refresh };
}
