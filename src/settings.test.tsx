// @vitest-environment jsdom
import { act, type ReactNode } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SettingsPage } from './components/SettingsPage';
import { AiClients, type AiClientStatus } from './components/AiClients';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { gatewayFetch } from './lib/gateway';
import searchCatalog from './lib/searchCatalog.json';

vi.mock('./lib/gateway', () => ({
  DEFAULT_GATEWAY_PORT: 8795,
  initGateway: vi.fn(),
  gatewayFetch: vi.fn(),
  gatewayUrl: (path: string) => `http://localhost:8795${path}`,
  setGatewayToken: vi.fn(),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }));

const fetchMock = vi.mocked(gatewayFetch);
const response = (data: unknown, status = 200) => new Response(JSON.stringify(data), { status });
let host: HTMLDivElement;
let root: Root;

async function render(node: ReactNode) {
  await act(async () => { root.render(node); });
}
async function click(selector: string) {
  const element = host.querySelector<HTMLButtonElement>(selector);
  expect(element, `missing ${selector}`).not.toBeNull();
  await act(async () => { element!.click(); });
}
async function type(selector: string, value: string) {
  await act(async () => {
    const input = host.querySelector<HTMLInputElement>(selector)!;
    // React tracks the last value it wrote, so assigning `.value` directly is
    // ignored; go through the native setter the way a real keystroke does.
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!
      .set!.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
}
/** The saved settings the page loads before it renders anything editable. */
function config(overrides: Record<string, string> = {}) {
  fetchMock.mockImplementation(async (url, init) => {
    if (url === '/api/config' && init?.method !== 'POST') {
      return response({ domain_preset: 'auto', enabled_sources: '', ...overrides });
    }
    throw new Error(`Unexpected request ${url}`);
  });
}
const activeSourceCount = () =>
  Number(host.textContent?.match(/(\d+) sources active/)?.[1] ?? -1);

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.resetAllMocks();
  localStorage.clear();
  vi.mocked(isTauri).mockReturnValue(false);
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => { root.unmount(); });
  host.remove();
  vi.restoreAllMocks();
});

describe('source health checks', () => {
  /** Reach the detailed card for one source, which is where Check lives. */
  async function openSource(id: string, name: string) {
    config();
    await render(<SettingsPage port={8795} />);
    await type('input[placeholder^="Filter"]', name);
    expect(host.querySelector(`#check-source-${id}`)).not.toBeNull();
  }

  it('reports what the source answered, including how long it took', async () => {
    await openSource('openalex', 'OpenAlex');
    fetchMock.mockResolvedValueOnce(response({
      id: 'openalex', name: 'OpenAlex', ok: true, count: 5, elapsed_ms: 1234,
      query: 'climate change', needs_setup: false, error: null,
    }));
    await click('#check-source-openalex');
    expect(fetchMock).toHaveBeenLastCalledWith('/api/source/check', expect.objectContaining({ method: 'POST' }));
    expect(JSON.parse(String(fetchMock.mock.lastCall?.[1]?.body))).toEqual({ id: 'openalex' });
    expect(host.querySelector('[data-testid="health-openalex"]')?.textContent).toBe('5 results · 1.2s');
  });

  // A source that answers with nothing is working; only the query missed.
  it('separates an empty answer from a failure and from a missing credential', async () => {
    await openSource('openalex', 'OpenAlex');
    fetchMock.mockResolvedValueOnce(response({
      id: 'openalex', name: 'OpenAlex', ok: true, count: 0, elapsed_ms: 800,
      query: 'climate change', needs_setup: false, error: null,
    }));
    await click('#check-source-openalex');
    expect(host.querySelector('[data-testid="health-openalex"]')?.textContent)
      .toContain('Reachable, 0 results');

    fetchMock.mockResolvedValueOnce(response({
      id: 'openalex', name: 'OpenAlex', ok: false, count: 0, elapsed_ms: 12000,
      query: 'climate change', needs_setup: false, error: 'openalex: connection timed out',
    }));
    await click('#check-source-openalex');
    expect(host.querySelector('[data-testid="health-openalex"]')?.textContent)
      .toBe('openalex: connection timed out');

    fetchMock.mockResolvedValueOnce(response({
      id: 'openalex', name: 'OpenAlex', ok: false, count: 0, elapsed_ms: 40,
      query: 'climate change', needs_setup: true, error: 'openalex: requires an API key',
    }));
    await click('#check-source-openalex');
    expect(host.querySelector('[data-testid="health-openalex"]')?.textContent)
      .toBe('openalex: requires an API key');
  });

  it('says so when the gateway itself refuses the check', async () => {
    await openSource('openalex', 'OpenAlex');
    fetchMock.mockResolvedValueOnce(response({ error: 'Unknown or unsupported source' }, 400));
    await click('#check-source-openalex');
    expect(host.querySelector('[data-testid="health-openalex"]')?.textContent)
      .toBe('Unknown or unsupported source');
  });
});

describe('default sources', () => {
  const defaults = searchCatalog.presets.find((preset) => preset.id === 'auto')!.sources;

  it('starts a fresh install on the default preset rather than no sources at all', async () => {
    config();
    await render(<SettingsPage port={8795} />);
    expect(activeSourceCount()).toBe(defaults.length);
  });

  it('restores the defaults after the selection was emptied', async () => {
    config({ domain_preset: 'custom', enabled_sources: '' });
    await render(<SettingsPage port={8795} />);
    expect(activeSourceCount()).toBe(0);
    await click('#restore-default-sources');
    expect(activeSourceCount()).toBe(defaults.length);
  });

  // Switching to Custom used to hand over an empty list, silently turning every
  // source off; it now starts from whatever was active a moment earlier.
  it('carries the active sources over when switching to a custom selection', async () => {
    config({ domain_preset: 'biomedical', enabled_sources: '' });
    await render(<SettingsPage port={8795} />);
    const before = activeSourceCount();
    expect(before).toBeGreaterThan(0);
    await click('#preset-custom');
    expect(activeSourceCount()).toBe(before);
  });
});

describe('AI client setup', () => {
  const client = (overrides: Partial<AiClientStatus> = {}): AiClientStatus => ({
    id: 'codex', name: 'Codex', detected: true,
    mcp_path: '/Users/me/.codex/config.toml', mcp_format: 'toml',
    mcp_installed: false, mcp_managed: false, mcp_entry: null, mcp_error: null,
    skills_path: '/Users/me/.codex/skills',
    skills: [
      { name: 'paper-search', source: 'builtin:paper-search', installed: false, managed: false, enabled: false },
      { name: 'paper-collect', source: 'builtin:paper-collect', installed: false, managed: false, enabled: false },
      { name: 'research-resume', source: 'builtin:research-resume', installed: false, managed: false, enabled: false },
    ],
    skills_error: null, note: 'Start a new Codex session after installing.', token_note: null,
    ...overrides,
  });

  it('shows what is already wired up and installs on request', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const installed = client({ mcp_installed: true, mcp_managed: true, mcp_entry: 'scholargateway' });
    vi.mocked(invoke).mockResolvedValueOnce([client()]).mockResolvedValueOnce(installed);
    await render(<AiClients />);
    expect(host.textContent).toContain('Codex');
    expect(host.textContent).toContain('NOT CONNECTED');
    expect(host.textContent).toContain('/Users/me/.codex/config.toml');

    await click('#ai-client-codex-install-mcp');
    expect(invoke).toHaveBeenLastCalledWith('setup_ai_client', { client: 'codex', action: 'install_mcp' });
    expect(host.textContent).toContain('CONNECTED');
    expect(host.textContent).toContain('Start a new Codex session');
  });

  it('never offers to overwrite an entry the app did not install', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValueOnce([
      client({ mcp_installed: true, mcp_managed: false, mcp_entry: 'my-own-gateway' }),
    ]);
    await render(<AiClients />);
    expect(host.querySelector<HTMLButtonElement>('#ai-client-codex-install-mcp')!.disabled).toBe(true);
    expect(host.querySelector('#ai-client-codex-remove-mcp')).toBeNull();
    expect(host.textContent).toContain('my-own-gateway');
  });

  it('reports a client that is not installed instead of offering to set it up', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValueOnce([client({ detected: false })]);
    await render(<AiClients />);
    expect(host.textContent).toContain('NOT INSTALLED');
    expect(host.querySelector('#ai-client-codex-install-mcp')).toBeNull();
  });
});
