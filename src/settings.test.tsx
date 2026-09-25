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
  getGatewayPort: () => 8795,
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
    await render(<SettingsPage />);
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
    await render(<SettingsPage />);
    expect(activeSourceCount()).toBe(defaults.length);
  });

  it('restores the defaults after the selection was emptied', async () => {
    config({ domain_preset: 'custom', enabled_sources: '' });
    await render(<SettingsPage />);
    expect(activeSourceCount()).toBe(0);
    await click('#restore-default-sources');
    expect(activeSourceCount()).toBe(defaults.length);
  });

  // Switching to Custom used to hand over an empty list, silently turning every
  // source off; it now starts from whatever was active a moment earlier.
  it('carries the active sources over when switching to a custom selection', async () => {
    config({ domain_preset: 'biomedical', enabled_sources: '' });
    await render(<SettingsPage />);
    const before = activeSourceCount();
    expect(before).toBeGreaterThan(0);
    await click('#preset-custom');
    expect(activeSourceCount()).toBe(before);
  });

  it('keeps distinct VJOL/SearXNG sources instead of substituting other connectors', async () => {
    config({ domain_preset: 'custom', enabled_sources: 'vjol,searxng' });
    await render(<SettingsPage />);
    expect(activeSourceCount()).toBe(2);
    fetchMock.mockResolvedValueOnce(response({ success: true }));
    await click('#save-settings');
    const saved = JSON.parse(String(fetchMock.mock.lastCall?.[1]?.body));
    expect(saved.enabled_sources).toBe('vjol,searxng');
  });

  it('drops obsolete unavailable source IDs loaded from an older configuration', async () => {
    config({ domain_preset: 'custom', enabled_sources: 'openalex,papers_with_code,missing,openalex' });
    await render(<SettingsPage />);
    expect(activeSourceCount()).toBe(1);
    fetchMock.mockResolvedValueOnce(response({ success: true }));
    await click('#save-settings');
    const saved = JSON.parse(String(fetchMock.mock.lastCall?.[1]?.body));
    expect(saved.enabled_sources).toBe('openalex');
  });
});

describe('AI client setup', () => {
  const client = (overrides: Partial<AiClientStatus> = {}): AiClientStatus => ({
    id: 'codex', name: 'Codex', detected: true,
    mcp_path: '/Users/me/.codex/config.toml', mcp_format: 'toml',
    mcp_installed: false, mcp_managed: false, mcp_entry: null, mcp_error: null,
    skills_path: '/Users/me/.codex/skills',
    skills: [
      { name: 'paper-search', source: 'builtin:paper-search', installed: false, managed: false, enabled: false, up_to_date: false },
      { name: 'paper-collect', source: 'builtin:paper-collect', installed: false, managed: false, enabled: false, up_to_date: false },
      { name: 'research-resume', source: 'builtin:research-resume', installed: false, managed: false, enabled: false, up_to_date: false },
    ],
    skills_error: null, note: 'Start a new Codex session after installing.', token_note: null,
    ...overrides,
  });

  it('shows what is already wired up and installs on request', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const installed = client({ mcp_installed: true, mcp_managed: true, mcp_entry: 'scholargate' });
    vi.mocked(invoke).mockResolvedValueOnce([client()]).mockResolvedValueOnce(installed);
    await render(<AiClients />);
    expect(host.textContent).toContain('Codex');
    expect(host.textContent).toContain('NOT CONFIGURED');
    expect(host.textContent).toContain('/Users/me/.codex/config.toml');

    await click('#ai-client-codex-install-all');
    expect(invoke).toHaveBeenLastCalledWith('setup_ai_client', { client: 'codex', action: 'install_all' });
    expect(host.textContent).toContain('CONFIGURED');
    expect(host.textContent).toContain('Start a new Codex session');
  });

  it('never offers to overwrite an entry the app did not install', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValueOnce([
      client({ mcp_installed: true, mcp_managed: false, mcp_entry: 'my-own-gateway' }),
    ]);
    await render(<AiClients />);
    expect(host.querySelector<HTMLButtonElement>('#ai-client-codex-install-all')!.disabled).toBe(true);
    expect(host.querySelector('#ai-client-codex-remove-mcp')).toBeNull();
    expect(host.textContent).toContain('my-own-gateway');
  });

  it('offers MCP-only setup when a client has no skills support', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const claude = client({
      id: 'claude_desktop', name: 'Claude Desktop', skills_path: null, skills: [],
    });
    vi.mocked(invoke).mockResolvedValueOnce([claude]).mockResolvedValueOnce({ ...claude, mcp_installed: true });
    await render(<AiClients />);
    await click('#ai-client-claude_desktop-install-mcp');
    expect(invoke).toHaveBeenLastCalledWith('setup_ai_client', {
      client: 'claude_desktop', action: 'install_mcp',
    });
  });

  it('reports a client that is not installed instead of offering to set it up', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValueOnce([client({ detected: false })]);
    await render(<AiClients />);
    expect(host.textContent).toContain('NOT INSTALLED');
    expect(host.querySelector('#ai-client-codex-install-all')).toBeNull();
  });

  it('offers an update when a managed bundled skill is stale', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const staleSkills = client().skills.map((skill, index) => ({
      ...skill, installed: true, managed: true, enabled: true, up_to_date: index !== 0,
    }));
    const before = client({ skills: staleSkills });
    const after = client({ skills: staleSkills.map((skill) => ({ ...skill, up_to_date: true })) });
    vi.mocked(invoke).mockResolvedValueOnce([before]).mockResolvedValueOnce(after);
    await render(<AiClients />);
    expect(host.textContent).toContain('paper-search · update');
    expect(host.textContent).toContain('Install MCP + skills');
    await click('#ai-client-codex-install-all');
    expect(invoke).toHaveBeenLastCalledWith('setup_ai_client', { client: 'codex', action: 'install_all' });
  });
});

describe('connection settings', () => {
  it('replaces a newly saved secret with the write-only sentinel in the form', async () => {
    config();
    await render(<SettingsPage />);
    const tabs = Array.from(host.querySelectorAll<HTMLButtonElement>('button'));
    await act(async () => { tabs.find((button) => button.textContent?.includes('Connections & Keys'))!.click(); });
    await type('#setting-openai_api_key', 'sk-new-secret');
    fetchMock.mockResolvedValueOnce(response({ success: true }));
    await click('#save-settings');
    expect(host.querySelector<HTMLInputElement>('#setting-openai_api_key')?.value).toBe('__SG_KEEP__');
    expect(host.textContent).toContain('Saved securely');
  });

  it('keeps every catalog credential in the controlled save payload', async () => {
    config();
    await render(<SettingsPage />);
    const tabs = Array.from(host.querySelectorAll<HTMLButtonElement>('button'));
    await act(async () => { tabs.find((button) => button.textContent?.includes('Connections & Keys'))!.click(); });

    for (const key of new Set(searchCatalog.sources.flatMap((source) => source.credentials))) {
      expect(host.querySelector(`#setting-${key}`), `missing controlled field for ${key}`).not.toBeNull();
    }

    fetchMock.mockResolvedValueOnce(response({ success: true }));
    await click('#save-settings');
    const saved = JSON.parse(String(fetchMock.mock.lastCall?.[1]?.body));
    expect(saved.core_api_key).toBe('');
  });

  it('exposes Groq and tests its saved key through the gateway', async () => {
    config({ groq_api_key: '__SG_KEEP__' });
    await render(<SettingsPage />);
    const tabs = Array.from(host.querySelectorAll<HTMLButtonElement>('button'));
    await act(async () => { tabs.find((button) => button.textContent?.includes('Connections & Keys'))!.click(); });
    expect(host.textContent).toContain('Groq');
    const buttons = Array.from(host.querySelectorAll<HTMLButtonElement>('button')).filter((button) => button.textContent?.trim() === 'Test');
    const groqButton = buttons[buttons.length - 1]!;
    fetchMock.mockResolvedValueOnce(response({ success: true, message: 'Groq API key verified successfully!', latency_ms: 20 }));
    await act(async () => { groqButton.click(); });
    expect(fetchMock).toHaveBeenLastCalledWith('/api/test-llm', expect.objectContaining({ method: 'POST' }));
    expect(JSON.parse(String(fetchMock.mock.lastCall?.[1]?.body)).provider).toBe('groq');
  });

  it('tests the dedicated web-search URL before saving', async () => {
    config({ web_search_url: 'http://localhost:8080' });
    await render(<SettingsPage />);
    const tabs = Array.from(host.querySelectorAll<HTMLButtonElement>('button'));
    await act(async () => { tabs.find((button) => button.textContent?.includes('Gateway & Security'))!.click(); });
    fetchMock.mockResolvedValueOnce(response({ success: true, message: 'Connected', latency_ms: 12 }));
    await click('#test-web-search');
    expect(fetchMock).toHaveBeenLastCalledWith('/api/test-searxng', expect.objectContaining({ method: 'POST' }));
    expect(JSON.parse(String(fetchMock.mock.lastCall?.[1]?.body))).toEqual({
      url: 'http://localhost:8080', categories: 'general', engines: '',
    });
  });

  it('saves search pacing and the write-only outbound proxy setting', async () => {
    config({ search_delay_ms: '2500', proxy_enabled: 'false', proxy_url: '' });
    await render(<SettingsPage />);
    const tabs = Array.from(host.querySelectorAll<HTMLButtonElement>('button'));
    await act(async () => { tabs.find((button) => button.textContent?.includes('Gateway & Security'))!.click(); });
    expect(host.querySelector<HTMLInputElement>('#search-delay-ms')?.value).toBe('2500');
    await act(async () => { host.querySelector<HTMLInputElement>('#proxy-enabled')!.click(); });
    await type('#setting-proxy_url', 'http://user:pass@127.0.0.1:8080');
    fetchMock.mockResolvedValueOnce(response({ success: true }));
    await click('#save-settings');
    const saved = JSON.parse(String(fetchMock.mock.lastCall?.[1]?.body));
    expect(saved.search_delay_ms).toBe('2500');
    expect(saved.proxy_enabled).toBe('true');
    expect(saved.proxy_url).toBe('http://user:pass@127.0.0.1:8080');
    expect(host.querySelector<HTMLInputElement>('#setting-proxy_url')?.value).toBe('__SG_KEEP__');
  });
});
