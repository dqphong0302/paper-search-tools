import { useCallback, useState } from 'react';
import { gatewayFetch } from '../../lib/gateway';
import { TestResult } from './ui';

/** Runs "Test connection" probes and keeps one result per id (provider, searxng…). */
export function useConnectionTests() {
  const [results, setResults] = useState<Record<string, TestResult>>({});

  const set = useCallback((id: string, result: TestResult) => setResults((prev) => ({ ...prev, [id]: result })), []);

  const run = useCallback(async (id: string, path: string, body: unknown) => {
    set(id, { loading: true });
    try {
      const res = await gatewayFetch(path, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
      const data = await res.json().catch(() => null);
      set(id, {
        loading: false,
        success: res.ok && data?.success === true,
        message: data?.message || `Server returned status code ${res.status}`,
        latency: data?.latency_ms,
      });
    } catch (e) {
      set(id, { loading: false, success: false, message: `Error: ${(e as Error).message}` });
    }
  }, [set]);

  const fail = useCallback((id: string, message: string) => set(id, { loading: false, success: false, message }), [set]);

  return { results, run, fail };
}
